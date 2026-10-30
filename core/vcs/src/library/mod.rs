// SPDX-License-Identifier: MIT
//! Component libraries in git repositories (mitcad#64) and the community
//! library (mitcad#63).
//!
//! A library is a git repository with a manifest at its root
//! (`mitcad-library.json`, [`manifest`]): its components are ordinary
//! Mitcad designs, often with a configuration table of sizes, and preview
//! images. A community index is a git repository too
//! (`mitcad-index.json` and an entry per library in `libraries/`); no
//! server is needed.
//!
//! - **Cache** ([`LibraryCache`]): bare clones in the user's data folder
//!   (`MITCAD_LIBRARIES_DIR`), one per URL, made and updated with the
//!   system's git only when the user asks (`library_fetch`); git's
//!   automatic garbage collection is off there, so versions designs were
//!   made with stay readable after a library's history moved on.
//! - **Versions** ([`LibraryRepo::versions`]): the tags, newest first by
//!   their numbers, and the default branch's tip. Designs are read straight
//!   from a commit's tree with gix, so designs can use different versions
//!   of one library at the same time.
//! - **Resolver** ([`LibraryResolver`]): the model's
//!   [`mitcad_model::LinkResolver`]: a component of a library at the commit
//!   a design recorded for it.
//! - **BUILD parts** ([`build`], mitcad#375): a component's `build` block
//!   and supplier fields, their checks, and the fitting of parts to a
//!   junction or a picker (`library_build_candidates`).
//!
//! Library content is data from elsewhere: repositories are opened with
//! gix's isolated options (none of the user's or the system's git
//! configuration), paths in manifests stay inside the repository, symbolic
//! links are not followed, and sizes are limited. Nothing in a library is
//! run.
//!
//! The JSON commands are in [`api`] (`core/model-api/src/commands.md`,
//! "Component libraries").

pub mod api;
pub mod build;
pub mod manifest;
// Mount frames of any part (mitcad#525).
pub mod mounts;
mod publish;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::refs::Target;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use mitcad_model::file::BREP_DIR;
use mitcad_model::{BlobStore, LibraryRequest, LibrarySource, LinkResolver, MemoryStore, Sha256};
use serde::Serialize;

use crate::VcsError;
use crate::remote::{Control, ErrorClass, GitCli, RemoteError, check_url, redact};
use manifest::{
    INDEX_ENTRIES, INDEX_FORMAT, INDEX_MANIFEST, IndexEntry, IndexManifest, LIBRARY_MANIFEST,
    MAX_COMPONENT, MAX_MANIFEST, Manifest, read_json,
};

/// The largest library a fetch keeps (its bare clone on disk).
pub const MAX_LIBRARY_SIZE: u64 = 200 << 20;

/// What a repository is by its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoKind {
    Library,
    Index,
    Unknown,
}

/// A version of a library: a tag or the default branch's tip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct LibraryVersion {
    pub rev: String,
    pub short: String,
    /// The tag (`v1.2.0`); empty for the branch's tip.
    pub label: String,
    /// The commit's date, `2026-10-05 14:03:12 +0300`.
    pub date: String,
    pub time: i64,
    pub summary: String,
    /// The default branch's tip (the newest work, maybe not released).
    pub head: bool,
}

impl LibraryVersion {
    /// `v1.2.0 (3f9a2c1)`, or `3f9a2c1 (latest)`.
    pub fn text(&self) -> String {
        if self.label.is_empty() {
            format!("{} (latest, unreleased)", self.short)
        } else {
            format!("{} ({})", self.label, self.short)
        }
    }
}

/// A library or index repository: a bare clone in the cache, or a folder
/// with a repository (a library being made).
pub struct LibraryRepo {
    repo: gix::Repository,
    dir: PathBuf,
}

impl std::fmt::Debug for LibraryRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryRepo")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

/// A manifest's text field as the model records it: none when empty.
fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

fn not_found(message: impl Into<String>) -> VcsError {
    VcsError::NotFound(message.into())
}

impl LibraryRepo {
    /// Opens the repository in `dir` with none of the user's or the
    /// system's git configuration.
    pub fn open(dir: &Path) -> Result<Self, VcsError> {
        let repo = gix::open_opts(dir, gix::open::Options::isolated()).map_err(|e| {
            VcsError::Git(format!(
                "{} is not a git repository: {}",
                dir.display(),
                crate::chain(&e)
            ))
        })?;
        Ok(Self {
            repo,
            dir: dir.to_path_buf(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where it was fetched from (`remote.origin.url`), credentials
    /// hidden.
    pub fn url(&self) -> Option<String> {
        let config = self.repo.config_snapshot();
        config
            .string("remote.origin.url")
            .map(|url| redact(&url.to_str_lossy()))
            .filter(|url| !url.is_empty())
    }

    /// The commit HEAD points to.
    pub fn head(&self) -> Result<Option<ObjectId>, VcsError> {
        Ok(self.repo.head()?.id().map(|id| id.detach()))
    }

    /// Whether the repository has commit `id`.
    pub fn contains(&self, id: ObjectId) -> bool {
        self.repo.find_commit(id).is_ok()
    }

    /// The tags and the commits they name (annotated tags peeled).
    fn tags(&self) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let mut out = Vec::new();
        let platform = self.repo.references()?;
        for reference in platform.tags()? {
            let mut reference = reference?;
            let name = reference.name().shorten().to_str_lossy().into_owned();
            if let Ok(id) = reference.peel_to_id()
                && self.contains(id.detach())
            {
                out.push((name, id.detach()));
            }
        }
        Ok(out)
    }

    /// The commit of `rev`: a full commit id, a tag, a branch, `HEAD`, or
    /// a prefix (4 or more hex digits) of a tagged commit or one on the
    /// default branch.
    pub fn resolve(&self, rev: &str) -> Result<ObjectId, VcsError> {
        let rev = rev.trim();
        check_revision(rev)?;
        let missing = || not_found(format!("the library has no version '{rev}'"));
        if rev.is_empty() || rev.eq_ignore_ascii_case("HEAD") {
            return self.head()?.ok_or_else(missing);
        }
        if rev.len() == 40
            && let Ok(id) = ObjectId::from_hex(rev.as_bytes())
        {
            return if self.contains(id) {
                Ok(id)
            } else {
                Err(missing())
            };
        }
        for name in [format!("refs/tags/{rev}"), format!("refs/heads/{rev}")] {
            if let Some(mut reference) = self.repo.try_find_reference(name.as_str())?
                && let Ok(id) = reference.peel_to_id()
            {
                return Ok(id.detach());
            }
        }
        let hex = rev.to_ascii_lowercase();
        if hex.len() >= 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            let mut found: Option<ObjectId> = None;
            let mut candidates: Vec<ObjectId> = self.tags()?.into_iter().map(|t| t.1).collect();
            if let Some(head) = self.head()? {
                for info in self
                    .repo
                    .rev_walk([head])
                    .first_parent_only()
                    .all()?
                    .take(10_000)
                {
                    candidates.push(info?.id);
                }
            }
            for id in candidates {
                if id.to_string().starts_with(&hex) {
                    if found.is_some_and(|f| f != id) {
                        return Err(not_found(format!(
                            "'{rev}' names more than one version of the library"
                        )));
                    }
                    found = Some(id);
                }
            }
            return found.ok_or_else(missing);
        }
        Err(missing())
    }

    /// The tags of `commit`, newest version first.
    pub fn labels(&self, commit: ObjectId) -> Result<Vec<String>, VcsError> {
        let mut labels: Vec<String> = self
            .tags()?
            .into_iter()
            .filter(|(_, id)| *id == commit)
            .map(|(name, _)| name)
            .collect();
        labels.sort_by(|a, b| version_order(b, a));
        Ok(labels)
    }

    /// The versions: the tags, newest first (by their version numbers, else
    /// their dates), then the default branch's tip unless a tag names it.
    pub fn versions(&self) -> Result<Vec<LibraryVersion>, VcsError> {
        let mut tags = self.tags()?;
        tags.sort_by(|a, b| version_order(&b.0, &a.0));
        let head = self.head()?;
        let mut out = Vec::new();
        for (label, id) in &tags {
            out.push(self.version(*id, label, head == Some(*id))?);
        }
        if let Some(head) = head
            && !tags.iter().any(|(_, id)| *id == head)
        {
            out.push(self.version(head, "", true)?);
        }
        Ok(out)
    }

    /// The version to show first: the newest tag, else the branch's tip.
    pub fn newest(&self) -> Result<Option<LibraryVersion>, VcsError> {
        Ok(self.versions()?.into_iter().next())
    }

    fn version(&self, id: ObjectId, label: &str, head: bool) -> Result<LibraryVersion, VcsError> {
        let commit = self.repo.find_commit(id)?;
        let time = commit.committer()?.time()?;
        let message = commit.message_raw()?.to_str_lossy().into_owned();
        Ok(LibraryVersion {
            rev: id.to_string(),
            short: id.to_hex_with_len(7).to_string(),
            label: label.to_owned(),
            date: time.format_or_unix(gix::date::time::format::ISO8601),
            time: time.seconds,
            summary: message.lines().next().unwrap_or("").trim().to_owned(),
            head,
        })
    }

    /// The blob at `path` in commit `commit`: None when there is none (or a
    /// folder); a symbolic link is an error.
    pub fn read(&self, commit: ObjectId, path: &str) -> Result<Option<Vec<u8>>, VcsError> {
        if !manifest::valid_path(path) {
            return Err(VcsError::InvalidPath(format!(
                "'{path}' is not a path inside the library"
            )));
        }
        let tree = self.repo.find_commit(commit)?.tree()?;
        let Some(entry) = tree.lookup_entry(path.split('/'))? else {
            return Ok(None);
        };
        let mode = entry.mode();
        if mode.is_link() {
            return Err(VcsError::InvalidPath(format!(
                "{path} is a symbolic link; libraries hold files"
            )));
        }
        if mode.is_tree() || mode.is_commit() {
            return Ok(None);
        }
        Ok(Some(self.repo.find_blob(entry.object_id())?.take_data()))
    }

    /// What the repository is at `commit`.
    pub fn kind(&self, commit: ObjectId) -> RepoKind {
        if matches!(self.read(commit, LIBRARY_MANIFEST), Ok(Some(_))) {
            RepoKind::Library
        } else if matches!(self.read(commit, INDEX_MANIFEST), Ok(Some(_))) {
            RepoKind::Index
        } else {
            RepoKind::Unknown
        }
    }

    /// The library's manifest at `commit` (read, not checked).
    pub fn manifest(&self, commit: ObjectId) -> Result<Manifest, VcsError> {
        let data = self.read(commit, LIBRARY_MANIFEST)?.ok_or_else(|| {
            not_found(format!(
                "version {} has no {LIBRARY_MANIFEST}: it is not a Mitcad library",
                commit.to_hex_with_len(7)
            ))
        })?;
        read_json(&data, LIBRARY_MANIFEST).map_err(VcsError::File)
    }

    /// The index's manifest and entries at `commit`; entries that cannot
    /// be read are reported apart.
    pub fn index(
        &self,
        commit: ObjectId,
    ) -> Result<(IndexManifest, Vec<IndexEntry>, Vec<String>), VcsError> {
        let data = self
            .read(commit, INDEX_MANIFEST)?
            .ok_or_else(|| not_found(format!("there is no {INDEX_MANIFEST}: not an index")))?;
        let index: IndexManifest = read_json(&data, INDEX_MANIFEST).map_err(VcsError::File)?;
        if index.format != INDEX_FORMAT || index.version != 1 {
            return Err(VcsError::File(format!(
                "{INDEX_MANIFEST} is not of a format this Mitcad reads ({INDEX_FORMAT}, version 1)"
            )));
        }
        let mut entries = Vec::new();
        let mut problems = Vec::new();
        let tree = self.repo.find_commit(commit)?.tree()?;
        if let Some(folder) = tree.lookup_entry([INDEX_ENTRIES])?
            && folder.mode().is_tree()
        {
            let folder = self.repo.find_tree(folder.object_id())?;
            for entry in folder.iter() {
                let entry = entry.map_err(|e| VcsError::Git(e.to_string()))?;
                let name = entry.filename().to_str_lossy().into_owned();
                if !name.ends_with(".json") || !entry.mode().is_blob() {
                    continue;
                }
                let data = self.repo.find_blob(entry.object_id())?.take_data();
                match read_json::<IndexEntry>(&data, &format!("{INDEX_ENTRIES}/{name}")) {
                    Ok(item) => {
                        let checked = item.problems();
                        if checked.is_ok() {
                            entries.push(item);
                        } else {
                            problems.push(format!(
                                "{INDEX_ENTRIES}/{name}: {}",
                                checked.errors.join("; ")
                            ));
                        }
                    }
                    Err(e) => problems.push(e),
                }
            }
        }
        entries.sort_by_key(|e| e.name.to_lowercase());
        Ok((index, entries, problems))
    }

    /// The B-rep data a design's text refers to, from the same commit.
    pub fn store(&self, commit: ObjectId, text: &str) -> Result<MemoryStore, VcsError> {
        let store = MemoryStore::new();
        let references =
            mitcad_model::file::brep_references(text).map_err(|e| VcsError::File(e.to_string()))?;
        for sha256 in references {
            let name = sha256.to_string();
            let path = format!("{BREP_DIR}/{}/{name}.brep.zlib", &name[..2]);
            if let Some(data) = self.read(commit, &path)? {
                store
                    .put(&sha256, &data)
                    .map_err(|e| VcsError::Io(e.to_string()))?;
            }
        }
        Ok(store)
    }

    /// The design of a component at `commit` with what the model records
    /// about it.
    pub fn source(
        &self,
        commit: ObjectId,
        component: &str,
        url: &str,
    ) -> Result<LibrarySource, VcsError> {
        let manifest = self.manifest(commit)?;
        let problems = manifest.problems();
        if !problems.is_ok() {
            return Err(VcsError::File(format!(
                "the library's manifest at {}: {}",
                commit.to_hex_with_len(7),
                problems.errors.join("; ")
            )));
        }
        let item = manifest.component(component).ok_or_else(|| {
            not_found(format!(
                "{} {} has no component '{component}'",
                manifest.id,
                commit.to_hex_with_len(7)
            ))
        })?;
        let data = self
            .read(commit, &item.path)?
            .ok_or_else(|| not_found(format!("{} is not in the library", item.path)))?;
        if data.len() > MAX_COMPONENT {
            return Err(VcsError::File(format!(
                "{} is larger than a library's design may be",
                item.path
            )));
        }
        let text = String::from_utf8(data)
            .map_err(|_| VcsError::File(format!("{} is not text", item.path)))?;
        let store = self.store(commit, &text)?;
        Ok(LibrarySource {
            store,
            rev: commit.to_string(),
            label: self.labels(commit)?.into_iter().next().unwrap_or_default(),
            library: manifest.id.clone(),
            url: if url.is_empty() {
                self.url().unwrap_or_default()
            } else {
                url.to_owned()
            },
            library_name: manifest.name.clone(),
            component: item.id.clone(),
            authors: manifest.authors.clone(),
            path: item.path.clone(),
            name: item.name.clone(),
            standard: item.standard.clone(),
            license: manifest.license_of(item).unwrap_or_default(),
            designation: item.designation.clone(),
            supplier: non_empty(&item.supplier),
            part_number: non_empty(&item.part_number),
            supplier_url: non_empty(&item.supplier_url),
            build: item.build.clone(),
            // The rows' standards and articles (mitcad#949).
            rows: manifest.named_rows(item),
            text,
        })
    }
}

/// Orders tags as versions: `v1.10.0` after `v1.9.0`; tags without numbers
/// by name.
fn version_order(a: &str, b: &str) -> std::cmp::Ordering {
    mitcad_model::configurations::natural_cmp(a, b)
}

/// The libraries the user fetched: bare clones in a folder of the user's
/// data, one per URL, and folders of more read-only ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryCache {
    /// Where fetches clone to.
    pub root: PathBuf,
    /// Folders of more libraries, read only (shipped with the
    /// application).
    pub extra: Vec<PathBuf>,
    /// The largest clone a fetch keeps, a new one or an updated one
    /// ([`MAX_LIBRARY_SIZE`]; tests set less).
    pub max_size: u64,
}

impl Default for LibraryCache {
    fn default() -> Self {
        let mut cache = Self::new(Self::default_root());
        if Self::environment_root().is_none() {
            // Older CLI versions used a different folder. Keep their fetched
            // revisions readable; future fetches use the shared application cache.
            cache
                .extra
                .push(Self::data_root().join("mitcad").join("libraries"));
        }
        cache
    }
}

/// The outcome of a fetch.
#[derive(Debug, Clone, Serialize)]
pub struct Fetched {
    pub kind: RepoKind,
    pub id: String,
    pub name: String,
    pub url: String,
    pub dir: String,
    /// Whether it was cloned now (else updated).
    pub cloned: bool,
    pub head: Option<String>,
    pub versions: Vec<LibraryVersion>,
    pub problems: manifest::Problems,
}

impl LibraryCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            extra: Vec::new(),
            max_size: MAX_LIBRARY_SIZE,
        }
    }

    /// `MITCAD_LIBRARIES_DIR`, else `libraries` in the user's data folder
    /// for Mitcad, shared by the application and command-line tools.
    pub fn default_root() -> PathBuf {
        Self::environment_root().unwrap_or_else(|| {
            Self::data_root()
                .join("Mitcad")
                .join("Mitcad")
                .join("libraries")
        })
    }

    /// The user's library of materials and print profiles (mitcad#640):
    /// `MITCAD_PRINT_LIBRARY_DIR`, else `print-library` in the user's data
    /// folder for Mitcad, beside `libraries`.
    pub fn default_print_library() -> PathBuf {
        std::env::var_os("MITCAD_PRINT_LIBRARY_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Self::data_root()
                    .join("Mitcad")
                    .join("Mitcad")
                    .join("print-library")
            })
    }

    fn environment_root() -> Option<PathBuf> {
        std::env::var_os("MITCAD_LIBRARIES_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
    }

    fn data_root() -> PathBuf {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home.map(|h| h.join("Library").join("Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .filter(|d| !d.is_empty())
                .map(PathBuf::from)
                .or_else(|| home.map(|h| h.join(".local").join("share")))
        };
        base.unwrap_or_else(std::env::temp_dir)
    }

    /// The folder a URL's clone has: `<name>-<8 hex digits>.git`.
    pub fn dir_for(&self, url: &str) -> PathBuf {
        let url = normal_url(url);
        let digest = Sha256::of(url.as_bytes()).to_string();
        let name: String = url
            .trim_end_matches(".git")
            .rsplit(['/', '\\', ':'])
            .find(|part| !part.is_empty())
            .unwrap_or("library")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .take(40)
            .collect();
        self.root.join(format!("{name}-{}.git", &digest[..8]))
    }

    /// Every repository in the cache and the extra folders.
    pub fn repos(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for folder in std::iter::once(&self.root).chain(&self.extra) {
            let Ok(entries) = fs::read_dir(folder) else {
                continue;
            };
            let mut dirs: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.extension().is_some_and(|e| e == "git"))
                .collect();
            dirs.sort();
            out.extend(dirs);
        }
        out
    }

    /// The clone of `url`, if it was fetched.
    pub fn open_url(&self, url: &str) -> Option<LibraryRepo> {
        let url = check_url(url).ok()?;
        let dir = self.dir_for(&url);
        if dir.is_dir() {
            return LibraryRepo::open(&dir).ok();
        }
        let wanted = normal_url(&url);
        self.repos().into_iter().find_map(|dir| {
            let repo = LibraryRepo::open(&dir).ok()?;
            (repo.url().map(|u| normal_url(&u)) == Some(wanted.clone())).then_some(repo)
        })
    }

    /// A clone with library `id` (at its HEAD), when `url` was not fetched
    /// here (another URL of the same library).
    pub fn open_id(&self, id: &str) -> Option<LibraryRepo> {
        self.repos().into_iter().find_map(|dir| {
            let repo = LibraryRepo::open(&dir).ok()?;
            let head = repo.head().ok()??;
            (repo.manifest(head).ok()?.id == id).then_some(repo)
        })
    }

    /// The repository to read library `id` at `rev` from: the clone of
    /// `url`, else any clone that has the commit and whose manifest there
    /// is the library's.
    pub fn find(
        &self,
        id: &str,
        url: &str,
        rev: &str,
    ) -> Result<(LibraryRepo, ObjectId), VcsError> {
        check_revision(rev.trim())?;
        let mut candidates = Vec::new();
        if let Some(repo) = self.open_url(url) {
            candidates.push(repo);
        }
        for dir in self.repos() {
            if candidates.iter().any(|r| r.dir == dir) {
                continue;
            }
            if let Ok(repo) = LibraryRepo::open(&dir) {
                candidates.push(repo);
            }
        }
        for repo in candidates {
            let Ok(commit) = repo.resolve(rev) else {
                continue;
            };
            if repo.manifest(commit).is_ok_and(|m| m.id == id) {
                return Ok((repo, commit));
            }
        }
        Err(not_found(format!(
            "library {id} at {} is not on this computer (fetch {})",
            short(rev),
            redact(url)
        )))
    }

    /// Clones `url` into the cache, or updates its clone: the branches and
    /// tags, without removing anything, so versions designs use stay. A
    /// new clone that is not a library or an index, or is too large, is
    /// removed again; an update that would make a clone too large is not
    /// taken ([`LibraryCache::max_size`], mitcad#938).
    pub fn fetch(
        &self,
        url: &str,
        git: &GitCli,
        control: Option<&Control>,
        log: &mut Vec<String>,
    ) -> Result<Fetched, VcsError> {
        let url = check_url(url)?;
        let shown = redact(&url);
        fs::create_dir_all(&self.root)
            .map_err(|e| crate::io_error("make the folder", &self.root, e))?;
        let dir = self.dir_for(&url);
        let cloned = !dir.is_dir();
        // No hooks run in a bare clone; checked objects; no automatic gc,
        // which would drop versions nothing refers to any more (an update
        // passes the same, `update_clone`).
        let safe = ["-c", "transfer.fsckObjects=true", "-c", "gc.auto=0"];
        if cloned {
            let temporary = self.root.join(format!(
                ".{}.{}.tmp",
                dir.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                std::process::id()
            ));
            let temp = temporary.to_string_lossy().into_owned();
            let mut args: Vec<&str> = safe.to_vec();
            args.extend(["clone", "--bare", "--progress", "--", &url, &temp]);
            let result = crate::remote::run_logged(
                git,
                &self.root,
                &args,
                control,
                &format!("Fetching {shown}"),
                log,
            );
            if let Err(e) = result {
                let _ = fs::remove_dir_all(&temporary);
                return Err(e);
            }
            let size = folder_size(&temporary);
            if size > self.max_size {
                let _ = fs::remove_dir_all(&temporary);
                return Err(RemoteError::new(
                    ErrorClass::TooLarge,
                    format!(
                        "{shown} is {} MB, more than the {} MB a library may have",
                        size >> 20,
                        self.max_size >> 20
                    ),
                )
                .into());
            }
            let kind = LibraryRepo::open(&temporary)
                .ok()
                .and_then(|repo| Some(repo.kind(repo.head().ok()??)));
            if kind.is_none_or(|k| k == RepoKind::Unknown) {
                let _ = fs::remove_dir_all(&temporary);
                return Err(RemoteError::new(
                    ErrorClass::NotAProject,
                    format!(
                        "{shown} is not a Mitcad library or index ({LIBRARY_MANIFEST} or \
                         {INDEX_MANIFEST} at its root)"
                    ),
                )
                .into());
            }
            fs::rename(&temporary, &dir).map_err(|e| crate::io_error("rename", &temporary, e))?;
        } else {
            self.update_clone(&dir, &url, git, control, log)?;
        }
        let repo = LibraryRepo::open(&dir)?;
        let head = repo.head()?;
        let kind = head.map_or(RepoKind::Unknown, |h| repo.kind(h));
        let (id, name, problems) = match (kind, head) {
            (RepoKind::Library, Some(head)) => {
                let manifest = repo.manifest(head)?;
                let problems = manifest.problems();
                (manifest.id, manifest.name, problems)
            }
            (RepoKind::Index, Some(head)) => {
                let (index, _, problems) = repo.index(head)?;
                (
                    String::new(),
                    index.name,
                    manifest::Problems {
                        errors: Vec::new(),
                        warnings: problems,
                    },
                )
            }
            _ => (String::new(), String::new(), manifest::Problems::default()),
        };
        Ok(Fetched {
            kind,
            id,
            name,
            url: shown,
            dir: dir.to_string_lossy().into_owned(),
            cloned,
            head: head.map(|h| h.to_string()),
            versions: repo.versions()?,
            problems,
        })
    }

    /// Updates the clone in `dir` from `url` within [`LibraryCache::max_size`]
    /// (mitcad#938): the branches and tags are fetched to [`INCOMING`]
    /// first and move to `refs/heads/` and `refs/tags/` (replacing, never
    /// removing) only when the clone is not too large then. A clone that
    /// grew past the limit gets the error `too_large`, and the objects and
    /// references the fetch brought are removed again, so the versions
    /// fetched before stay as they were.
    fn update_clone(
        &self,
        dir: &Path,
        url: &str,
        git: &GitCli,
        control: Option<&Control>,
        log: &mut Vec<String>,
    ) -> Result<(), VcsError> {
        let shown = redact(url);
        let target = dir.to_string_lossy().into_owned();
        // What an interrupted update left.
        LibraryRepo::open(dir)?.drop_incoming()?;
        let before = object_files(dir);
        let heads = format!("+refs/heads/*:{INCOMING}heads/*");
        let tags = format!("+refs/tags/*:{INCOMING}tags/*");
        let args = [
            "-c",
            "transfer.fsckObjects=true",
            "-c",
            "gc.auto=0",
            "--git-dir",
            &target,
            "fetch",
            "--progress",
            "--no-write-fetch-head",
            // Tags only by the refspec: git's tag following would write
            // them to refs/tags/ at once.
            "--no-tags",
            "--",
            url,
            &heads,
            &tags,
        ];
        let fetched = crate::remote::run_logged(
            git,
            &self.root,
            &args,
            control,
            &format!("Fetching {shown}"),
            log,
        );
        let repo = LibraryRepo::open(dir)?;
        if let Err(error) = fetched {
            let _ = repo.drop_incoming();
            return Err(error);
        }
        let size = folder_size(dir);
        if size > self.max_size {
            repo.drop_incoming()?;
            // Closed first: Windows removes no file that is open.
            drop(repo);
            let removed = remove_new_objects(dir, &before);
            log.push(format!(
                "{shown}: {} MB after the fetch, more than {} MB: {removed} new object \
                 file(s) removed, the versions fetched before kept",
                size >> 20,
                self.max_size >> 20
            ));
            return Err(RemoteError::new(
                ErrorClass::TooLarge,
                format!(
                    "{shown} would be {} MB with its new versions, more than the {} MB a \
                     library may have; the versions fetched before stay",
                    size >> 20,
                    self.max_size >> 20
                ),
            )
            .into());
        }
        repo.accept_incoming()
    }
}

/// Where an update of a clone fetches the branches and tags to first
/// ([`LibraryCache::update_clone`]).
const INCOMING: &str = "refs/incoming/";

impl LibraryRepo {
    /// The direct references whose names start with `prefix`, with the
    /// objects they point to.
    fn references_under(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let mut out = Vec::new();
        for reference in self.repo.references()?.all()? {
            let reference = reference?;
            let name = reference.name().as_bstr().to_string();
            if name.starts_with(prefix)
                && let Some(id) = reference.try_id()
            {
                out.push((name, id.detach()));
            }
        }
        Ok(out)
    }

    /// Applies reference edits in one transaction.
    fn edit_references(&self, edits: Vec<RefEdit>) -> Result<(), VcsError> {
        if edits.is_empty() {
            return Ok(());
        }
        let signature = gix::actor::Signature {
            name: "Mitcad".into(),
            email: "library-fetch@mitcad.invalid".into(),
            time: gix::date::Time::now_local_or_utc(),
        };
        let mut time = Default::default();
        let edited = self
            .repo
            .edit_references_as(edits, Some(signature.to_ref(&mut time)));
        // gix removes the empty folders above a reference whose lock it
        // releases without writing it, up to the repository's own folder: in
        // a clone whose references are all in packed-refs, that took
        // refs/heads/, refs/tags/ and then refs/ itself, and without refs/
        // neither git nor gix opens the clone any more. The folders git makes
        // in every repository stay.
        for folder in ["refs/heads", "refs/tags"] {
            let path = self.dir.join(folder);
            fs::create_dir_all(&path).map_err(|e| crate::io_error("make the folder", &path, e))?;
        }
        edited?;
        Ok(())
    }

    /// Removes the references of an update not taken ([`INCOMING`]).
    fn drop_incoming(&self) -> Result<(), VcsError> {
        let mut edits = Vec::new();
        for (name, id) in self.references_under(INCOMING)? {
            edits.push(reference_edit(
                &name,
                Change::Delete {
                    expected: PreviousValue::MustExistAndMatch(Target::Object(id)),
                    log: RefLog::AndReference,
                },
            )?);
        }
        self.edit_references(edits)
    }

    /// Moves the fetched references ([`INCOMING`]) over the branches and
    /// tags, as a forced fetch would have set them; branches and tags the
    /// remote no longer has stay.
    fn accept_incoming(&self) -> Result<(), VcsError> {
        let current: HashMap<String, ObjectId> = self
            .references_under("refs/heads/")?
            .into_iter()
            .chain(self.references_under("refs/tags/")?)
            .collect();
        let mut edits = Vec::new();
        for (name, id) in self.references_under(INCOMING)? {
            let local = format!("refs/{}", &name[INCOMING.len()..]);
            // A branch or tag the fetch found unchanged is not written again.
            if current.get(&local) != Some(&id) {
                edits.push(reference_edit(
                    &local,
                    Change::Update {
                        log: LogChange {
                            mode: RefLog::AndReference,
                            force_create_reflog: false,
                            message: "library fetch".into(),
                        },
                        expected: PreviousValue::Any,
                        new: Target::Object(id),
                    },
                )?);
            }
            edits.push(reference_edit(
                &name,
                Change::Delete {
                    expected: PreviousValue::MustExistAndMatch(Target::Object(id)),
                    log: RefLog::AndReference,
                },
            )?);
        }
        self.edit_references(edits)
    }
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

/// The files under a clone's `objects` folder.
fn object_files(dir: &Path) -> HashSet<PathBuf> {
    let mut files = HashSet::new();
    let mut pending = vec![dir.join("objects")];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(entry.path());
            }
        }
    }
    files
}

/// Removes the files under `objects` that were not in `before` (what a
/// fetch brought: new packs and loose objects; the objects already there
/// are never in them); the number removed.
fn remove_new_objects(dir: &Path, before: &HashSet<PathBuf>) -> usize {
    object_files(dir)
        .into_iter()
        .filter(|path| !before.contains(path))
        .filter(|path| fs::remove_file(path).is_ok())
        .count()
}

/// A URL compared as written, without a trailing `/`.
fn normal_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_owned()
}

fn short(rev: &str) -> String {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        rev[..7].to_owned()
    } else {
        rev.to_owned()
    }
}

/// Revisions accept commit ids and names, never revision expressions or paths.
fn check_revision(rev: &str) -> Result<(), VcsError> {
    if rev.is_empty() || rev.eq_ignore_ascii_case("HEAD") {
        return Ok(());
    }
    let name = format!("refs/tags/{rev}");
    gix::validate::reference::name(name.as_bytes().as_bstr()).map_err(|_| {
        VcsError::InvalidPath(format!(
            "'{rev}' is not a library version (a commit id, tag or branch name)"
        ))
    })?;
    Ok(())
}

/// The bytes of the files in a folder (not following links).
pub(crate) fn folder_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

/// The model's resolver of library parts: components read from the cache
/// at the commits designs recorded.
#[derive(Debug, Clone)]
pub struct LibraryResolver {
    cache: LibraryCache,
}

impl LibraryResolver {
    pub fn new(cache: LibraryCache) -> Self {
        Self { cache }
    }

    /// Installs a resolver for the process's documents.
    pub fn install(cache: LibraryCache) {
        mitcad_model::set_link_resolver(Some(Arc::new(Self::new(cache))));
    }
}

impl LinkResolver for LibraryResolver {
    /// The base library of materials and print profiles (mitcad#640): the
    /// item files Mitcad's parts library lists in its newest version on
    /// this computer; none when it was not fetched.
    fn print_library(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        use mitcad_model::print_library::{BASE_LIBRARY, MAX_ITEM};

        let Some(repo) = self.cache.open_id(BASE_LIBRARY) else {
            return Ok(Vec::new());
        };
        let Some(version) = repo.newest().map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        let commit = ObjectId::from_hex(version.rev.as_bytes()).map_err(|e| e.to_string())?;
        let manifest = repo.manifest(commit).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for item in manifest.print_items() {
            if let Some(data) = repo.read(commit, &item.path).map_err(|e| e.to_string())?
                && data.len() <= MAX_ITEM
            {
                out.push((item.path.clone(), data));
            }
        }
        Ok(out)
    }

    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let (repo, commit) = self
            .cache
            .find(&request.library, &request.url, &request.rev)
            .map_err(|e| e.to_string())?;
        repo.source(commit, &request.component, &request.url)
            .map_err(|e| e.to_string())
    }

    fn library_texture(&self, request: &LibraryRequest, path: &str) -> Result<Vec<u8>, String> {
        use mitcad_model::appearance::{MAX_EMBEDDED_IMAGE, image_format};

        if request.rev.len() != 40 || ObjectId::from_hex(request.rev.as_bytes()).is_err() {
            return Err("a library texture needs the component's exact commit".to_owned());
        }
        let (repo, commit) = self
            .cache
            .find(&request.library, &request.url, &request.rev)
            .map_err(|e| e.to_string())?;
        let manifest = repo.manifest(commit).map_err(|e| e.to_string())?;
        let problems = manifest.problems();
        if !problems.is_ok() {
            return Err(format!(
                "the library's manifest at {}: {}",
                commit.to_hex_with_len(7),
                problems.errors.join("; ")
            ));
        }
        let item = manifest.component(&request.component).ok_or_else(|| {
            format!(
                "{} {} has no component '{}'",
                manifest.id,
                commit.to_hex_with_len(7),
                request.component
            )
        })?;
        let relative = texture_path(&item.path, path)?;
        let bytes = repo
            .read(commit, &relative)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("texture '{relative}' is not in the library"))?;
        if bytes.len() > MAX_EMBEDDED_IMAGE {
            return Err(format!(
                "texture '{relative}' is larger than the {} MB a design file embeds",
                MAX_EMBEDDED_IMAGE >> 20
            ));
        }
        if image_format(&bytes).is_none() {
            return Err(format!("texture '{relative}' is no PNG or JPEG file"));
        }
        Ok(bytes)
    }
}

/// A texture's path in the commit tree. Parent directories may reach
/// shared images elsewhere in the library, but never leave its root.
fn texture_path(design: &str, texture: &str) -> Result<String, String> {
    let invalid = || format!("texture '{texture}' is not a path inside the library");
    if texture.is_empty()
        || texture.starts_with('/')
        || texture.contains('\\')
        || texture.contains(':')
        || !manifest::valid_path(design)
    {
        return Err(invalid());
    }
    let mut parts: Vec<_> = design.split('/').collect();
    parts.pop();
    for part in texture.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or_else(invalid)?;
            }
            part => parts.push(part),
        }
    }
    let path = parts.join("/");
    if !manifest::valid_path(&path) {
        return Err(invalid());
    }
    Ok(path)
}

/// Reads a manifest JSON from a folder (a library being made), at most
/// [`MAX_MANIFEST`] bytes.
fn read_file(path: &Path) -> Result<Vec<u8>, VcsError> {
    let meta = fs::metadata(path).map_err(|e| crate::io_error("read", path, e))?;
    if meta.len() > MAX_MANIFEST as u64 * 16 {
        return Err(VcsError::File(format!("{} is too large", path.display())));
    }
    fs::read(path).map_err(|e| crate::io_error("read", path, e))
}
