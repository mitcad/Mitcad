// SPDX-License-Identifier: MIT
//! Projects made (mitcad#89): New Project's `create_project` (a Local
//! project, or a Cloud one with its remote, the remote's files taken in and
//! a push), Open from Cloud's `clone_project` with `adopt` (a repository
//! with files but no project made a project), and `init_bare` (a shared
//! folder made a remote).
//!
//! Nothing changes before what the remote holds is known, and a cancel or
//! a failure before the first version leaves the folder as it was (a
//! folder made for the project is removed, the files Mitcad wrote into one
//! that was there are taken out again). After the first version the
//! project stays: a failed push leaves its versions waiting (`ahead`).

use std::fs;
use std::path::{Path, PathBuf};

use mitcad_model::Project;
use mitcad_model::file::PROJECT_MARKER;
use mitcad_model::file::settings::{SharedSettings, write_shared};
use serde::Serialize;

use super::inspect::{ProjectRemote, remotes_of, valid_relative};
use super::{Context, folder_entries};
use crate::commit::project_files_in;
use crate::remote::{
    CloneOutcome, Control, DEFAULT_REMOTE, ErrorClass, PushOutcome, RemoteError, SyncOptions,
    SyncOutcome, check_url, clone_outcome, clone_repository, failure, parse_ls_remote, redact,
    remove_clone, run_logged,
};
use crate::{Identity, ProjectRepo, VcsError, absolute, io_error, same_folder};

/// The message of the first version of a repository made a project.
pub const ADOPT_MESSAGE: &str = "Make this repository a Mitcad project";

/// What New Project makes ([`create_project`]).
#[derive(Debug, Clone)]
pub struct CreateOptions {
    /// The author, written to the repository's own configuration.
    pub author: Identity,
    /// The first design: a path relative to the folder and its text.
    pub design: Option<(String, String)>,
    /// The `.mitcad` files already in the folder go into the first version.
    pub include_designs: bool,
    /// A Cloud project's remote.
    pub url: Option<String>,
    /// Push after the first version (with `url`).
    pub push: bool,
    /// The shared settings (edit locks, live updates) the first version's
    /// marker holds; None: the defaults (none written).
    pub shared: Option<SharedSettings>,
}

/// A project made ([`create_project`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Created {
    /// The project's folder (absolute).
    pub root: String,
    pub branch: Option<String>,
    /// The first version.
    pub commit: Option<String>,
    /// The paths it recorded, relative to the project.
    pub files: Vec<String>,
    pub warnings: Vec<String>,
    /// The remote, as `inspect_folder` gives it.
    pub remote: Option<ProjectRemote>,
    /// The remote's files (and versions) were taken in.
    pub adopted: bool,
    pub push: Option<PushOutcome>,
    /// The repository's versions and the remote's brought together (a
    /// folder that was a clone of the remote with newer versions there).
    pub sync: Option<SyncOutcome>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    /// A failure after the first version (the push): the project stays.
    #[serde(skip)]
    pub error: Option<RemoteError>,
}

/// A project opened from a remote ([`clone_project`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdoptOutcome {
    #[serde(flatten)]
    pub clone: CloneOutcome,
    /// The repository was made a project.
    pub adopted: bool,
    /// Its first version as a project.
    pub commit: Option<String>,
    pub push: Option<PushOutcome>,
    /// A failure after that version (the push): the project stays.
    #[serde(skip)]
    pub error: Option<RemoteError>,
}

/// A shared folder made a remote ([`init_bare`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BareOutcome {
    pub dir: String,
    /// False: it was a bare repository already.
    pub created: bool,
}

fn cancelled(control: Option<&Control>) -> Result<(), VcsError> {
    if control.is_some_and(Control::is_cancelled) {
        Err(RemoteError::new(ErrorClass::Cancelled, "cancelled").into())
    } else {
        Ok(())
    }
}

/// Refuses a folder that is a project or inside one.
fn refuse_projects(dir: &Path) -> Result<(), VcsError> {
    if dir.is_dir() && Project::open(dir).is_some() {
        return Err(failure(
            ErrorClass::InsideProject,
            format!(
                "{} is a Mitcad project already: open it (Open Project)",
                dir.display()
            ),
        ));
    }
    if let Some(project) = Project::find(dir) {
        return Err(failure(
            ErrorClass::InsideProject,
            format!(
                "{} is inside the Mitcad project {}: make the new project in a folder outside it",
                dir.display(),
                project.root().display()
            ),
        ));
    }
    if dir.exists() && !dir.is_dir() {
        return Err(VcsError::InvalidPath(format!(
            "{} is a file, not a folder",
            dir.display()
        )));
    }
    Ok(())
}

/// Whether folder `dir` is the root of a git repository's work tree.
fn repository_root_at(dir: &Path) -> bool {
    dir.is_dir()
        && gix::open(dir)
            .ok()
            .and_then(|repo| repo.workdir().map(|w| same_folder(w, dir)))
            .unwrap_or(false)
}

/// What a folder held before a project was made there, put back when
/// making it fails before its first version.
struct Before {
    dir: PathBuf,
    existed: bool,
    /// Files Mitcad writes or extends, with what they held.
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
    mitcad: bool,
    git: bool,
    /// The first design, when Mitcad wrote it.
    design: Option<PathBuf>,
}

impl Before {
    fn take(dir: &Path) -> Self {
        let mut files: Vec<PathBuf> = vec![dir.join(".gitattributes"), dir.join(".gitignore")];
        let git = dir.join(".git").exists();
        if git {
            files.push(dir.join(".git").join("config"));
        }
        Self {
            dir: dir.to_path_buf(),
            existed: dir.exists(),
            files: files
                .into_iter()
                .map(|path| {
                    let content = fs::read(&path).ok();
                    (path, content)
                })
                .collect(),
            mitcad: dir.join(".mitcad").exists(),
            git,
            design: None,
        }
    }

    /// The folder as it was (what can be put back).
    fn restore(&self) {
        if !self.existed {
            remove_clone(&self.dir, false);
            return;
        }
        if let Some(design) = &self.design {
            let _ = fs::remove_file(design);
        }
        if self.mitcad {
            let _ = fs::remove_file(self.dir.join(PROJECT_MARKER));
        } else {
            remove_clone(&self.dir.join(".mitcad"), false);
        }
        if !self.git {
            remove_clone(&self.dir.join(".git"), false);
        }
        for (path, content) in &self.files {
            if !self.git && path.starts_with(self.dir.join(".git")) {
                continue;
            }
            let _ = match content {
                Some(content) => mitcad_model::file::write_atomically(path, content),
                None => fs::remove_file(path),
            };
        }
    }
}

/// Writes the first design, which must not be there yet.
fn check_design(
    dir: &Path,
    design: Option<&(String, String)>,
) -> Result<Option<PathBuf>, VcsError> {
    let Some((path, _)) = design else {
        return Ok(None);
    };
    let first = path.split('/').next().unwrap_or("");
    if !valid_relative(path) || first == ".git" || first == ".mitcad" {
        return Err(VcsError::InvalidPath(format!(
            "'{path}' is not a path for a design in the project (relative, with /)"
        )));
    }
    let full = dir.join(path);
    if full.exists() {
        return Err(VcsError::InvalidPath(format!(
            "{} is there already: the new design needs another name",
            full.display()
        )));
    }
    Ok(Some(full))
}

/// The project made in `dir` with its first version (`message`): the
/// marker, `.gitattributes`, `.gitignore`, the repository (unless the
/// folder is the root of one), the author in its configuration, the
/// design, and with `include_designs` the `.mitcad` files there.
fn make_project(
    dir: &Path,
    options: &CreateOptions,
    message: &str,
    before: &mut Before,
    control: Option<&Control>,
) -> Result<(ProjectRepo, Vec<String>, Vec<String>, String), VcsError> {
    let design = check_design(dir, options.design.as_ref())?;
    cancelled(control)?;
    let repo = ProjectRepo::create(dir)?;
    repo.set_identity(&options.author.name, &options.author.email)?;
    if let Some(shared) = &options.shared {
        let marker = repo.root().join(PROJECT_MARKER);
        write_shared(repo.root(), shared).map_err(|e| io_error("write", &marker, e))?;
    }
    let mut files: Vec<PathBuf> = [PROJECT_MARKER, ".gitattributes", ".gitignore"]
        .iter()
        .map(|name| repo.root().join(name))
        .collect();
    if let (Some(full), Some((_, text))) = (design, &options.design) {
        before.design = Some(full.clone());
        mitcad_model::file::write_atomically(&full, text.as_bytes())
            .map_err(|e| io_error("write", &full, e))?;
        files.push(full);
    }
    if options.include_designs {
        for file in project_files_in(repo.root()) {
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    cancelled(control)?;
    let outcome = repo.commit(&files, message, &options.author)?;
    let Some(commit) = outcome.commit.clone() else {
        return Err(failure(
            ErrorClass::Unsupported,
            outcome
                .skipped
                .unwrap_or_else(|| "the first version could not be recorded".to_owned()),
        ));
    };
    let mut recorded: Vec<String> = files
        .iter()
        .filter_map(|file| repo.relative(file).ok())
        .collect();
    recorded.sort();
    recorded.dedup();
    Ok((repo, recorded, outcome.warnings, commit))
}

/// The name of a project's folder.
fn folder_name(dir: &Path) -> String {
    dir.file_name().map_or_else(
        || dir.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Makes a project in folder `dir` (missing, empty, or a folder of designs
/// or other files, or a git repository's root; not a project and not inside
/// one: `inside_project`) with its first version `Create project <folder
/// name>`, by `options.author`, who is written to the repository's own
/// configuration.
///
/// With `options.url` the remote is checked first and nothing changes on a
/// failure: a remote with a project is `has_project`; an empty one is set
/// as the project's remote; one with files but no project is cloned into
/// `dir` (which must then be missing or empty: `not_empty`) and the
/// project made beside its files, or, when `dir` is the root of a
/// repository that shares the remote's history, the project is made there
/// and brought together with the remote's newer versions. Then a push
/// (unless `options.push` is false); its failure is [`Created::error`],
/// with the project made and its versions `ahead`.
pub fn create_project(
    dir: &Path,
    options: &CreateOptions,
    context: &Context,
    control: Option<&Control>,
    log: &mut Vec<String>,
) -> Result<Created, VcsError> {
    let dir = absolute(dir)?;
    refuse_projects(&dir)?;
    check_design(&dir, options.design.as_ref())?;
    let message = format!("Create project {}", folder_name(&dir));
    let Some(url) = &options.url else {
        let (repo, files, warnings, commit) = local(&dir, options, &message, control)?;
        return finish(&repo, files, warnings, commit, false);
    };
    let url = check_url(url)?;
    let shown = redact(&url);
    let git = context.git()?;
    let existing = dir
        .ancestors()
        .find(|folder| folder.is_dir())
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    let listed = run_logged(
        &git,
        &existing,
        &["ls-remote", "--symref", &url],
        control,
        &format!("Reaching {shown}"),
        log,
    )?;
    let (_, tip, _) = parse_ls_remote(&listed.stdout);
    if tip.is_none() {
        // An empty remote: the project's versions go there.
        let (repo, files, warnings, commit) = local(&dir, options, &message, control)?;
        repo.set_git(git);
        let mut created = finish(&repo, files, warnings, commit, false)?;
        publish(&repo, &url, options, false, control, &mut created);
        log.extend(repo.take_remote_log());
        return refresh(&repo, created);
    }
    if repository_root_at(&dir) {
        // A repository of its own: the project goes onto its history when
        // that is the remote's.
        let probe = ProjectRepo::new(gix::open(&dir)?, dir.clone());
        probe.set_git(git.clone());
        let check = probe.remote_check(&url, control);
        log.extend(probe.take_remote_log());
        drop(probe);
        let check = check?;
        if check.has_project == Some(true) {
            return Err(has_project(&shown));
        }
        if check.related != Some(true) {
            return Err(failure(
                ErrorClass::NotEmpty,
                format!(
                    "{shown} holds files and the repository {} holds another history: make the \
                     project in a new or empty folder to take the remote's files in",
                    dir.display()
                ),
            ));
        }
        let (repo, files, warnings, commit) = local(&dir, options, &message, control)?;
        repo.set_git(git);
        let mut created = finish(&repo, files, warnings, commit, true)?;
        publish(&repo, &url, options, true, control, &mut created);
        log.extend(repo.take_remote_log());
        return refresh(&repo, created);
    }
    let literally_empty = !dir.exists()
        || fs::read_dir(&dir)
            .map_err(|e| io_error("read", &dir, e))?
            .next()
            .is_none();
    if !literally_empty {
        // Files here and on the remote: the class tells which.
        let check = super::check_remote(&url, context, control, log)?;
        if check.has_project == Some(true) {
            return Err(has_project(&shown));
        }
        return Err(failure(
            ErrorClass::NotEmpty,
            format!(
                "{} is not empty: a project is made beside the files of {shown} only in a new \
                 or empty folder (or make it Local here and share it onto the repository's \
                 files in Project Settings)",
                dir.display()
            ),
        ));
    }
    // The remote's files, then the project beside them.
    let cloned = clone_repository(&url, &dir, Some(&git), control, log)?;
    if cloned.dir.join(PROJECT_MARKER).is_file() {
        remove_clone(&cloned.dir, cloned.existed);
        return Err(has_project(&shown));
    }
    let mut before = Before::take(&dir);
    before.existed = cloned.existed;
    // A failure removes the clone as a whole.
    before.mitcad = false;
    let made = make_project(&dir, options, &message, &mut before, control);
    let (repo, files, warnings, commit) = match made {
        Ok(made) => made,
        Err(error) => {
            remove_clone(&dir, cloned.existed);
            return Err(error);
        }
    };
    repo.set_git(git);
    let mut created = finish(&repo, files, warnings, commit, true)?;
    publish(&repo, &url, options, false, control, &mut created);
    log.extend(repo.take_remote_log());
    refresh(&repo, created)
}

fn has_project(shown: &str) -> VcsError {
    failure(
        ErrorClass::HasProject,
        format!(
            "{shown} holds a Mitcad project: open it (Open from Cloud, mitcad-cli clone) instead \
             of making a new one"
        ),
    )
}

/// [`make_project`] in a folder that was there or not; the folder put back
/// on a failure.
fn local(
    dir: &Path,
    options: &CreateOptions,
    message: &str,
    control: Option<&Control>,
) -> Result<(ProjectRepo, Vec<String>, Vec<String>, String), VcsError> {
    let mut before = Before::take(dir);
    match make_project(dir, options, message, &mut before, control) {
        Ok(made) => Ok(made),
        Err(error) => {
            before.restore();
            Err(error)
        }
    }
}

fn finish(
    repo: &ProjectRepo,
    files: Vec<String>,
    warnings: Vec<String>,
    commit: String,
    adopted: bool,
) -> Result<Created, VcsError> {
    Ok(Created {
        root: repo.root().to_string_lossy().into_owned(),
        branch: repo.branch()?,
        commit: Some(commit),
        files,
        warnings,
        remote: None,
        adopted,
        push: None,
        sync: None,
        ahead: None,
        behind: None,
        error: None,
    })
}

/// The remote set (`origin`), fetched when it has versions, and the
/// project's versions sent: pushed, or synced when the remote has newer
/// ones. A failure is the answer's error; the project stays.
fn publish(
    repo: &ProjectRepo,
    url: &str,
    options: &CreateOptions,
    fetch: bool,
    control: Option<&Control>,
    created: &mut Created,
) {
    let result = (|| -> Result<(), VcsError> {
        repo.remote_set(url, DEFAULT_REMOTE, &options.author)?;
        if fetch {
            repo.fetch(control)?;
        }
        if !options.push {
            return Ok(());
        }
        let behind = repo.remote_info(false)?.behind.unwrap_or(0);
        if behind > 0 {
            let sync_options = SyncOptions {
                fetch: false,
                ..SyncOptions::default()
            };
            let outcome = repo.sync(&sync_options, Some(&options.author), control)?;
            let error = outcome.error.clone();
            created.sync = Some(outcome);
            if let Some(error) = error {
                return Err(error.into());
            }
        } else {
            created.push = Some(repo.push(control)?);
        }
        Ok(())
    })();
    match result {
        Ok(()) => {}
        Err(VcsError::Remote(error)) => created.error = Some(error),
        Err(other) => created.error = Some(RemoteError::new(ErrorClass::Other, other.to_string())),
    }
}

/// The remote, the branch and the versions ahead and behind as they are
/// now.
fn refresh(repo: &ProjectRepo, mut created: Created) -> Result<Created, VcsError> {
    let fresh = repo.fresh()?;
    created.remote = remotes_of(&fresh).0;
    created.branch = repo.branch()?;
    if created.remote.is_some() {
        let info = repo.remote_info(false)?;
        (created.ahead, created.behind) = (info.ahead, info.behind);
    }
    Ok(created)
}

/// Opens the remote at `url` into folder `dir` (new or empty:
/// `not_empty`; not in a project: `inside_project`) as the bridge's
/// `clone_project` does. With `adopt`, a remote with files but no project
/// is made a project (first version [`ADOPT_MESSAGE`] by `author`, else
/// git's configured author) and pushed; without, it stays
/// `not_a_project`, as an empty remote always does (`create_project` with
/// `url` makes a project there). `author`, when given, is written to the
/// repository's own configuration. A failure before the first version
/// leaves no folder (an empty one that was there stays, empty); a failed
/// push is [`AdoptOutcome::error`], the project made.
pub fn clone_project(
    url: &str,
    dir: &Path,
    adopt: bool,
    author: Option<&Identity>,
    context: &Context,
    control: Option<&Control>,
    log: &mut Vec<String>,
) -> Result<AdoptOutcome, VcsError> {
    let dir = absolute(dir)?;
    refuse_projects(&dir)?;
    if dir.is_dir()
        && fs::read_dir(&dir)
            .map_err(|e| io_error("read", &dir, e))?
            .next()
            .is_some()
    {
        return Err(failure(
            ErrorClass::NotEmpty,
            format!(
                "{} is not empty: a project is opened from a remote into a new or empty folder",
                dir.display()
            ),
        ));
    }
    let git = context.git()?;
    let cloned = clone_repository(url, &dir, Some(&git), control, log)?;
    let is_project = cloned.dir.join(PROJECT_MARKER).is_file();
    if !is_project && !adopt {
        remove_clone(&cloned.dir, cloned.existed);
        return Err(failure(
            ErrorClass::NotAProject,
            format!(
                "{} holds no Mitcad project ({PROJECT_MARKER} at its root): open it with \
                 \"adopt\" (Make It a Project) to make it one",
                cloned.url
            ),
        ));
    }
    let made = (|| -> Result<(Option<String>, Option<ProjectRepo>), VcsError> {
        if is_project {
            if let Some(author) = author {
                ProjectRepo::open(&cloned.dir)?.set_identity(&author.name, &author.email)?;
            }
            return Ok((None, None));
        }
        cancelled(control)?;
        Project::init(&cloned.dir).map_err(|e| io_error("make a project of", &cloned.dir, e))?;
        let repo = ProjectRepo::open(&cloned.dir)?;
        repo.set_git(git.clone());
        let author = match author {
            Some(author) => {
                repo.set_identity(&author.name, &author.email)?;
                author.clone()
            }
            None => repo.identity(None)?,
        };
        let files: Vec<PathBuf> = [PROJECT_MARKER, ".gitattributes", ".gitignore"]
            .iter()
            .map(|name| repo.root().join(name))
            .collect();
        let outcome = repo.commit(&files, ADOPT_MESSAGE, &author)?;
        Ok((outcome.commit, Some(repo)))
    })();
    let (commit, repo) = match made {
        Ok(made) => made,
        Err(error) => {
            remove_clone(&cloned.dir, cloned.existed);
            return Err(error);
        }
    };
    let mut error = None;
    let mut push = None;
    if let Some(repo) = &repo {
        match repo.push(control) {
            Ok(outcome) => push = Some(outcome),
            Err(VcsError::Remote(failed)) => error = Some(failed),
            Err(other) => error = Some(RemoteError::new(ErrorClass::Other, other.to_string())),
        }
        log.extend(repo.take_remote_log());
    }
    drop(repo);
    Ok(AdoptOutcome {
        clone: clone_outcome(&cloned)?,
        adopted: !is_project,
        commit,
        push,
        error,
    })
}

/// Makes folder `dir` a bare git repository for a project's remote (a
/// shared folder; the branch `main`): a missing or empty one is made one,
/// a bare repository is used as it is (`created` false), a folder with
/// anything else is `not_empty`.
pub fn init_bare(dir: &Path) -> Result<BareOutcome, VcsError> {
    let dir = absolute(dir)?;
    let text = dir.to_string_lossy().into_owned();
    if dir.exists() {
        if !dir.is_dir() {
            return Err(VcsError::InvalidPath(format!(
                "{text} is a file, not a folder"
            )));
        }
        if let Ok(repo) = gix::open(&dir)
            && repo.is_bare()
            && same_folder(repo.git_dir(), &dir)
        {
            return Ok(BareOutcome {
                dir: text,
                created: false,
            });
        }
        if !folder_entries(&dir)?.is_empty() {
            return Err(failure(
                ErrorClass::NotEmpty,
                format!(
                    "{text} holds other files: a shared folder for a project is an empty folder \
                     or a bare git repository"
                ),
            ));
        }
    }
    fs::create_dir_all(&dir).map_err(|e| io_error("make the folder", &dir, e))?;
    // gix makes a bare repository only in an empty folder: one that holds
    // files the system leaves gets it made beside them and moved in.
    let holds_files = fs::read_dir(&dir)
        .map_err(|e| io_error("read", &dir, e))?
        .next()
        .is_some();
    let target = if holds_files {
        dir.join(format!(".mitcad-init-{}", std::process::id()))
    } else {
        dir.clone()
    };
    use gix::sec::trust::DefaultForLevel;
    let options = gix::open::Options::default_for_level(gix::sec::Trust::Full)
        .config_overrides(["init.defaultBranch=main"]);
    let made = gix::ThreadSafeRepository::init_opts(
        &target,
        gix::create::Kind::Bare,
        gix::create::Options::default(),
        options,
    );
    if holds_files {
        let moved = made.map_err(VcsError::from).and_then(|repo| {
            drop(repo);
            for entry in fs::read_dir(&target).map_err(|e| io_error("read", &target, e))? {
                let entry = entry.map_err(|e| io_error("read", &target, e))?;
                let to = dir.join(entry.file_name());
                mitcad_model::file::rename_retrying(&entry.path(), &to)
                    .map_err(|e| io_error("move into place", &to, e))?;
            }
            Ok(())
        });
        remove_clone(&target, false);
        moved?;
    } else {
        made?;
    }
    Ok(BareOutcome {
        dir: text,
        created: true,
    })
}
