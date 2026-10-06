// SPDX-License-Identifier: MIT
//! Local version history of Mitcad projects (P12b): a project (a folder
//! with `.mitcad/project.json`, `mitcad_model::Project`) whose folder is
//! the root of a git repository records its versions as commits. Mitcad
//! commits only to such a repository, so a user's own repository does not
//! get Mitcad's commits unexpectedly.
//!
//! The git library is gitoxide (`gix`, MIT OR Apache-2.0), used only in this
//! crate; its types stay out of the public API so that another back end
//! (the system's git for pushing, later) can take its place. What gix lacks
//! (checkout, gc, hooks) P12 does not need: a commit is made straight from a
//! tree, older versions are read from their trees, and Mitcad writes the
//! files itself.
//!
//! - **Commit** ([`ProjectRepo::commit`]): the saved paths only, like `git
//!   commit --only`: HEAD's tree with those paths written or removed, the
//!   B-rep files their project files refer to added and the B-rep files no
//!   project file of the version refers to removed (from the folder too),
//!   then the commit and the branch moved from the HEAD it was built on
//!   (when another commit came first, it is built again). The index is
//!   locked (`index.lock`) for the whole round and only the committed
//!   paths' entries change, so the user's staged changes stay as they are
//!   and `git status` stays clean. Nothing changed: no commit. HEAD
//!   detached or a merge or rebase in progress: no commit, with the reason.
//! - **Author** ([`ProjectRepo::identity`]): git's configuration
//!   (`user.name`, `user.email`, `GIT_AUTHOR_*`), else Mitcad's.
//! - **History** ([`ProjectRepo::history`]): the commits on HEAD's
//!   first-parent chain that changed a file, following renames of the same
//!   content.
//! - **Older versions** ([`ProjectRepo::read`], [`ProjectRepo::load_version`]):
//!   read from the commit's tree without a checkout; their B-rep data from
//!   the same tree ([`GitTreeStore`]).
//! - **Changes** ([`ProjectRepo::changes`]): the paths that differ between
//!   two versions.
//! - **Comparison** ([`ProjectRepo::diff`], P12c): what differs in a
//!   design between two versions, or a version and the saved file
//!   (`mitcad_model::diff`), without computing them.
//! - **Restore** ([`ProjectRepo::restore`]): an older version written back
//!   and recorded as a new version; history is never rewritten.
//! - **Remote repositories** ([`remote`], P12 remote): the history shared
//!   through a git server or a folder, with the system's git program for
//!   the network (fetch, push, clone); local history works without it. A
//!   sync puts the project's unpublished versions after the remote's newer
//!   ones, file by file, with a choice for each file changed on both sides.
//!
//! The JSON commands for the application and `mitcad-cli` are in [`api`]
//! (`core/model/src/api/commands.md`, "Version history" and "Remote
//! repositories").

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use mitcad_model::file::PROJECT_MARKER;
use mitcad_model::{Document, Kernel, Project, Sha256};
use serde::Serialize;

pub mod api;
mod commit;
mod history;
// Remote repositories (P12 remote).
pub mod remote;
mod store;
mod tree;

pub use commit::CommitOutcome;
pub use history::{Change, ChangeKind, FileStatus, Version};
pub use store::GitTreeStore;

/// The version of Mitcad that records versions (a commit trailer).
const MITCAD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Who made a version: a commit's author and committer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

impl Identity {
    /// An identity git accepts: a name and an email address, neither empty
    /// nor with `<`, `>` or line breaks.
    pub fn new(name: &str, email: &str) -> Result<Self, VcsError> {
        let (name, email) = (name.trim(), email.trim());
        let bad = |text: &str| text.is_empty() || text.contains(['<', '>', '\n', '\r']);
        if bad(name) || bad(email) {
            return Err(VcsError::InvalidIdentity(format!("{name} <{email}>")));
        }
        Ok(Self {
            name: name.to_owned(),
            email: email.to_owned(),
        })
    }

    /// Reads `Name <email>`.
    pub fn parse(text: &str) -> Result<Self, VcsError> {
        let invalid = || VcsError::InvalidIdentity(text.to_owned());
        let (name, rest) = text.split_once('<').ok_or_else(invalid)?;
        let email = rest.trim_end().strip_suffix('>').ok_or_else(invalid)?;
        Self::new(name, email)
    }
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} <{}>", self.name, self.email)
    }
}

/// Why the version history could not do something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcsError {
    /// The path is not in a project with version history.
    NotVersioned(String),
    /// Neither git's configuration nor Mitcad's settings give an author.
    NoIdentity,
    InvalidIdentity(String),
    /// A path outside the project, or in its `.git`.
    InvalidPath(String),
    /// A version, or a file in a version, that is not there.
    NotFound(String),
    /// A project file of an older version that cannot be read.
    File(String),
    /// A JSON command that is not understood ([`api`]).
    Command(String),
    Io(String),
    /// The repository could not be read or written.
    Git(String),
    /// A remote repository's operation failed (P12 remote): its class, a
    /// message and git's words, credentials hidden.
    Remote(remote::RemoteError),
}

impl fmt::Display for VcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoIdentity => f.write_str(
                "no author for the version: set user.name and user.email in git's \
                 configuration, or the name and email in Mitcad's settings",
            ),
            Self::InvalidIdentity(text) => write!(
                f,
                "'{text}' is not an author: a name and an email address are needed, as \
                 'Name <email>'"
            ),
            Self::NotVersioned(message)
            | Self::InvalidPath(message)
            | Self::NotFound(message)
            | Self::File(message)
            | Self::Command(message)
            | Self::Io(message)
            | Self::Git(message) => f.write_str(message),
            Self::Remote(error) => f.write_str(&error.message),
        }
    }
}

impl std::error::Error for VcsError {}

/// The message of an error with its causes, as far as they add to it.
fn chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

impl From<gix::Error> for VcsError {
    fn from(error: gix::Error) -> Self {
        Self::Git(chain(&error))
    }
}

impl<E> From<gix::Exn<E>> for VcsError
where
    E: std::error::Error + Send + Sync + 'static,
    gix::Error: From<gix::Exn<E>>,
{
    fn from(error: gix::Exn<E>) -> Self {
        gix::Error::from(error).into()
    }
}

/// An I/O error with what was done to which path.
fn io_error(what: &str, path: &Path, error: io::Error) -> VcsError {
    VcsError::Io(format!("cannot {what} {}: {error}", path.display()))
}

/// The absolute form of a path with `.` and `..` folded away lexically (as
/// `mitcad_model::Project` finds projects).
fn absolute(path: &Path) -> Result<PathBuf, VcsError> {
    let mut out = PathBuf::new();
    let full = std::path::absolute(path).map_err(|e| io_error("resolve", path, e))?;
    for component in full.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// Whether two paths name the same folder.
fn same_folder(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The work tree of the git repository that folder `dir` (which need not
/// exist: its nearest existing folder upwards) is in, if any: whether a
/// project made there would be inside another repository (P12d).
pub fn repository_root(dir: &Path) -> Option<PathBuf> {
    let dir = absolute(dir).ok()?;
    let existing = dir.ancestors().find(|folder| folder.is_dir())?;
    let repo = gix::discover(existing).ok()?;
    repo.workdir().map(Path::to_path_buf)
}

/// Whether a path in a version is a project file.
fn is_project_file(path: &str) -> bool {
    path.ends_with(".mitcad")
}

/// A project whose folder is the root of a git repository: its version
/// history.
pub struct ProjectRepo {
    repo: gix::Repository,
    /// The project's folder (absolute, as given), the repository's work
    /// tree.
    root: PathBuf,
    /// The B-rep data each project file (by blob id) refers to; None for a
    /// file that could not be read. Also kept in `.mitcad/cache/refs`.
    references: RefCell<HashMap<ObjectId, Option<BTreeSet<Sha256>>>>,
    /// The git program for remote work, when set ([`ProjectRepo::set_git`]).
    git: RefCell<Option<remote::GitCli>>,
    /// The git commands run for remote work since the log was last taken.
    remote_log: RefCell<Vec<String>>,
    /// Called once between building a commit's tree and moving the branch
    /// (tests: another commit comes first).
    #[cfg(test)]
    race: RefCell<Option<Box<dyn FnOnce()>>>,
    /// Called at the stages of a sync (tests: cancelled during the replay,
    /// another push first).
    #[cfg(test)]
    sync_hook: RefCell<Option<SyncHook>>,
}

/// A test's hook at the stages of a sync.
#[cfg(test)]
type SyncHook = Box<dyn FnMut(&str)>;

impl fmt::Debug for ProjectRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectRepo")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl ProjectRepo {
    /// The project with version history that `path` (a file, which need not
    /// exist, or a project's folder) is in: the nearest project above it,
    /// whose folder must be the root of a git repository. The error says
    /// why there is none.
    pub fn open(path: &Path) -> Result<Self, VcsError> {
        Self::open_with(path, gix::open::Options::default())
    }

    fn open_with(path: &Path, options: gix::open::Options) -> Result<Self, VcsError> {
        let project = path
            .is_dir()
            .then(|| Project::open(path))
            .flatten()
            .or_else(|| Project::find(path))
            .ok_or_else(|| {
                VcsError::NotVersioned(format!(
                    "{} is not in a Mitcad project (a folder with {PROJECT_MARKER})",
                    path.display()
                ))
            })?;
        let root = project.root().to_path_buf();
        let no_history = || {
            VcsError::NotVersioned(format!(
                "the project {} has no version history (its folder is not the root of a git \
                 repository)",
                root.display()
            ))
        };
        let repo =
            gix::discover_opts(&root, Default::default(), options).map_err(|_| no_history())?;
        match repo.workdir() {
            Some(workdir) if same_folder(workdir, &root) => {}
            Some(workdir) => {
                return Err(VcsError::NotVersioned(format!(
                    "the project {} is inside the git repository {}, whose root is not the \
                     project's folder; Mitcad records versions only in a repository of its own",
                    root.display(),
                    workdir.display()
                )));
            }
            None => return Err(no_history()),
        }
        Ok(Self::new(repo, root))
    }

    fn new(repo: gix::Repository, root: PathBuf) -> Self {
        Self {
            repo,
            root,
            references: RefCell::default(),
            git: RefCell::default(),
            remote_log: RefCell::default(),
            #[cfg(test)]
            race: RefCell::default(),
            #[cfg(test)]
            sync_hook: RefCell::default(),
        }
    }

    /// Makes folder `dir` (created if needed) a project with version
    /// history: the project's marker, `.gitattributes` and `.gitignore`
    /// (`mitcad_model::Project::init`), a git repository unless the folder
    /// is the root of one already (a new one's branch is `main`, whatever
    /// git's `init.defaultBranch` says), and a first version of those
    /// files (none when they are recorded already) by `author`, or by
    /// git's configured author ([`ProjectRepo::identity`]; without one the
    /// error comes after the repository is made, and `init` can be run
    /// again).
    pub fn init(dir: &Path, author: Option<&Identity>) -> Result<(Self, CommitOutcome), VcsError> {
        let this = Self::create(dir)?;
        let author = match author {
            Some(author) => author.clone(),
            None => this.identity(None)?,
        };
        let author = &author;
        let files: Vec<PathBuf> = [PROJECT_MARKER, ".gitattributes", ".gitignore"]
            .iter()
            .map(|name| this.root.join(name))
            .collect();
        let name = this.root.file_name().map_or_else(
            || this.root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let outcome = this.commit(&files, &format!("Create project {name}"), author)?;
        Ok((this, outcome))
    }

    /// The first half of [`ProjectRepo::init`]: folder `dir` (created if
    /// needed) made a project with a git repository, but no version yet,
    /// so that the application can show the author git's configuration
    /// gives before the first version is recorded (P12d).
    pub fn create(dir: &Path) -> Result<Self, VcsError> {
        let (project, _) = Project::init(dir).map_err(|e| io_error("make a project of", dir, e))?;
        let root = project.root().to_path_buf();
        let repo = match gix::open(&root) {
            Ok(repo) if repo.workdir().is_some_and(|w| same_folder(w, &root)) => repo,
            _ => {
                // The branch is `main` whatever git's installation sets
                // (Git for Windows sets `master`).
                use gix::sec::trust::DefaultForLevel;
                let options = gix::open::Options::default_for_level(gix::sec::Trust::Full)
                    .config_overrides(["init.defaultBranch=main"]);
                gix::ThreadSafeRepository::init_opts(
                    &root,
                    gix::create::Kind::WithWorktree,
                    gix::create::Options::default(),
                    options,
                )?
                .to_thread_local()
            }
        };
        Ok(Self::new(repo, root))
    }

    /// The project's folder (absolute).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The branch HEAD is on (`main`), or None when HEAD is detached.
    pub fn branch(&self) -> Result<Option<String>, VcsError> {
        Ok(self
            .repo
            .head_name()?
            .map(|name| name.shorten().to_str_lossy().into_owned()))
    }

    /// The author of new versions: git's configuration (`user.name` and
    /// `user.email`, or `GIT_AUTHOR_NAME` and `GIT_AUTHOR_EMAIL`), else
    /// `fallback` (Mitcad's settings).
    pub fn identity(&self, fallback: Option<&Identity>) -> Result<Identity, VcsError> {
        if let Some(Ok(author)) = self.repo.author() {
            let configured =
                Identity::new(&author.name.to_str_lossy(), &author.email.to_str_lossy());
            if let Ok(identity) = configured {
                return Ok(identity);
            }
        }
        fallback.cloned().ok_or(VcsError::NoIdentity)
    }

    /// The path of `file` (absolute, or relative to the current folder) in
    /// the project's versions: relative to its folder, with `/`.
    pub fn relative(&self, file: &Path) -> Result<String, VcsError> {
        let full = absolute(file)?;
        let outside = || {
            VcsError::InvalidPath(format!(
                "{} is not in the project {}",
                file.display(),
                self.root.display()
            ))
        };
        let relative = full.strip_prefix(&self.root).map_err(|_| outside())?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_str().ok_or_else(|| {
                    VcsError::InvalidPath(format!("{} is not valid Unicode", file.display()))
                })?),
                _ => return Err(outside()),
            }
        }
        match parts.first() {
            None => Err(outside()),
            Some(first) if first.eq_ignore_ascii_case(".git") => Err(VcsError::InvalidPath(
                format!("{} is in the repository's .git folder", file.display()),
            )),
            Some(_) => Ok(parts.join("/")),
        }
    }

    /// The commit HEAD points to; None before the first version.
    fn head_commit(&self) -> Result<Option<ObjectId>, VcsError> {
        Ok(self.repo.head()?.id().map(|id| id.detach()))
    }

    /// Reads `file` as it was in version `rev` (an id, a unique prefix of
    /// one, `HEAD` or `HEAD~n`).
    pub fn read(&self, rev: &str, file: &Path) -> Result<Vec<u8>, VcsError> {
        let commit = self.resolve_id(rev)?;
        let path = self.relative(file)?;
        self.read_at(commit, &path)
    }

    fn read_at(&self, commit: ObjectId, path: &str) -> Result<Vec<u8>, VcsError> {
        let id = self.entry_at(commit, path)?.ok_or_else(|| {
            VcsError::NotFound(format!(
                "{path} is not in version {}",
                commit.to_hex_with_len(7)
            ))
        })?;
        Ok(self.repo.find_blob(id)?.take_data())
    }

    /// The B-rep store of version `rev`: its `.mitcad/brep` files, read from
    /// the commit's tree.
    pub fn store_at(&self, rev: &str) -> Result<GitTreeStore<'_>, VcsError> {
        let commit = self.resolve_id(rev)?;
        GitTreeStore::new(&self.repo, commit)
    }

    /// The project file `file` as it was in version `rev`, with its B-rep
    /// data from that version, as an untitled document: nothing is
    /// computed, and linked components are not followed (their bodies are
    /// those saved with the version). An older version is read under the
    /// path the file had there, through the renames its history follows.
    pub fn load_version<K: Kernel>(
        &self,
        rev: &str,
        file: &Path,
        kernel: K,
    ) -> Result<Document<K>, VcsError> {
        let commit = self.resolve_id(rev)?;
        let path = self.relative(file)?;
        let path = self.path_at(commit, file, &path)?;
        let data = self.read_at(commit, &path)?;
        let text = String::from_utf8(data)
            .map_err(|_| VcsError::File(format!("{path} is not text in that version")))?;
        let store = GitTreeStore::new(&self.repo, commit)?;
        Document::from_json_in(&text, &store, kernel)
            .map_err(|e| VcsError::File(format!("{path}: {e}")))
    }

    /// Writes `file` as it was in version `rev` (and the B-rep files it
    /// refers to that the project's store lacks) and records it as a new
    /// version: `message`, or `Restore <path> from <id>`. An older version
    /// is read under the path the file had there (through the renames its
    /// history follows) and written to the file's path now.
    pub fn restore(
        &self,
        file: &Path,
        rev: &str,
        message: Option<&str>,
        author: &Identity,
    ) -> Result<CommitOutcome, VcsError> {
        let commit = self.resolve_id(rev)?;
        let path = self.relative(file)?;
        let there = self.path_at(commit, file, &path)?;
        let data = self.read_at(commit, &there)?;
        if is_project_file(&path) {
            let references = std::str::from_utf8(&data)
                .ok()
                .and_then(|text| mitcad_model::file::brep_references(text).ok())
                .unwrap_or_default();
            let store = GitTreeStore::new(&self.repo, commit)?;
            let project = mitcad_model::FsStore::new(self.root.join(mitcad_model::file::BREP_DIR));
            for sha256 in references {
                use mitcad_model::BlobStore;
                if !project.contains(&sha256).unwrap_or(false)
                    && let Ok(Some(content)) = store.get(&sha256)
                {
                    project
                        .put(&sha256, &content)
                        .map_err(|e| io_error("write", &project.path(&sha256), e))?;
                }
            }
        }
        let target = self.root.join(&path);
        mitcad_model::file::write_atomically(&target, &data)
            .map_err(|e| io_error("write", &target, e))?;
        let default = format!("Restore {path} from {}", commit.to_hex_with_len(7));
        let message = message.filter(|m| !m.trim().is_empty()).unwrap_or(&default);
        self.commit(&[target], message, author)
    }
}

#[cfg(test)]
mod tests;
