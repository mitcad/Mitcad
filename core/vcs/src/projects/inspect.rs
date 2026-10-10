// SPDX-License-Identifier: MIT
//! What a folder is (mitcad#89, `inspect_folder`): a project (Local or
//! Cloud, read from its repository), a folder inside one, a git
//! repository, a folder of designs, empty or missing. No network and no
//! writes: Open Project and the New Project dialog's folder check run it
//! while the user types.

use std::fmt::Write;
use std::fs;
use std::path::Path;

use gix::bstr::ByteSlice;
use mitcad_model::Project;
use mitcad_model::file::settings::ProjectSettings;
use serde::Serialize;
use serde_json::Value;

use super::{clean_text, folder_entries, modified};
use crate::remote::{branch_of, redact, remote_url, upstream_for};
use crate::{VcsError, absolute, is_project_file, same_folder};

/// The designs listed at most.
const MAX_DESIGNS: usize = 1000;
/// The folder entries looked at for designs at most (a folder such as a
/// home folder is not walked whole while the user types).
const MAX_ENTRIES: usize = 50_000;
/// The file of the design opened last, relative to the project.
pub(crate) const RECENT: &str = ".mitcad/local/recent.json";
/// Settings and state files larger than this are not read.
const MAX_STATE_SIZE: u64 = 64 * 1024;

/// What a folder is, the first that applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderKind {
    /// The folder does not exist.
    Missing,
    /// It has `.mitcad/project.json`.
    Project,
    /// A folder above it has.
    InsideProject,
    /// It is the root of a git repository's work tree.
    Repository,
    /// Nothing in it but files the system leaves there.
    Empty,
    /// `.mitcad` files in it or below.
    Designs,
    Other,
}

/// The remote a project (or a repository) follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectRemote {
    pub name: String,
    /// Credentials hidden.
    pub url: Option<String>,
    /// The branch HEAD is on (None: detached).
    pub branch: Option<String>,
    /// `origin/main`: the branch's upstream, else the remote's branch of the
    /// same name.
    pub upstream: Option<String>,
}

/// A design in the folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Design {
    /// Relative to the project's folder (else the folder), with `/`.
    pub path: String,
    /// When it was last changed (RFC 3339, UTC).
    pub modified: Option<String>,
    #[serde(skip)]
    seconds: u64,
}

/// What [`inspect_folder`] finds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FolderInfo {
    /// The folder (absolute).
    pub dir: String,
    pub kind: FolderKind,
    /// The project the folder is or is in (also for a missing folder
    /// inside a project).
    pub project_root: Option<String>,
    /// The work tree of the git repository the folder (or its nearest
    /// existing folder upwards) is in.
    pub git_root: Option<String>,
    /// That work tree when it is not the project's or the folder's own.
    pub outer_repository: Option<String>,
    /// Why the project has no version history (it is inside another
    /// repository), when so.
    pub history_blocked: Option<String>,
    /// The project's folder is the root of its repository.
    pub has_history: bool,
    /// The remote the branch follows, else `origin`, else the only one;
    /// None with several and none of these (Project Settings asks which).
    pub remote: Option<ProjectRemote>,
    /// All the remotes' names (of the project's repository, or of the
    /// folder's own).
    pub remotes: Vec<String>,
    /// A Cloud project: version history and a remote.
    pub cloud: bool,
    /// The designs (`.mitcad` files) of the project (else of the folder),
    /// newest first, `.git` and `.mitcad` folders left out.
    pub designs: Vec<Design>,
    /// More designs than listed, or the folder too large to look at whole.
    pub designs_truncated: bool,
    /// The design opened last (`.mitcad/local/recent.json`), while it is
    /// there.
    pub last_design: Option<String>,
    /// The project's shared settings, checked (as `project_settings`'
    /// `shared`); None outside projects.
    pub settings: Option<Value>,
    /// What of the settings was not used.
    pub problems: Vec<String>,
}

/// The git repository whose work tree holds `dir` (or its nearest existing
/// folder upwards).
fn discover(dir: &Path) -> Option<gix::Repository> {
    let existing = dir.ancestors().find(|folder| folder.is_dir())?;
    let repo = gix::discover(existing).ok()?;
    repo.workdir().is_some().then_some(repo)
}

/// What folder `dir` is (see [`FolderInfo`]); a path that is a file is an
/// error.
pub fn inspect_folder(dir: &Path) -> Result<FolderInfo, VcsError> {
    let dir = absolute(dir)?;
    let exists = dir.is_dir();
    if !exists && dir.exists() {
        return Err(VcsError::InvalidPath(format!(
            "{} is a file, not a folder",
            dir.display()
        )));
    }
    let own_project = exists.then(|| Project::open(&dir)).flatten();
    let project = own_project.clone().or_else(|| Project::find(&dir));
    let root = project.as_ref().map(|project| project.root().to_path_buf());
    let repo = discover(&dir);
    let git_root = repo
        .as_ref()
        .and_then(|repo| repo.workdir().map(Path::to_path_buf));
    let is_root_of = |folder: &Path| git_root.as_deref().is_some_and(|g| same_folder(g, folder));
    let has_history = root.as_deref().is_some_and(is_root_of);
    let kind = if !exists {
        FolderKind::Missing
    } else if own_project.is_some() {
        FolderKind::Project
    } else if project.is_some() {
        FolderKind::InsideProject
    } else if is_root_of(&dir) {
        FolderKind::Repository
    } else if folder_entries(&dir)?.is_empty() {
        FolderKind::Empty
    } else {
        // Designs or other files: decided by the walk below.
        FolderKind::Other
    };
    // The repository is the project's own, or the folder's own.
    let own = has_history || kind == FolderKind::Repository;
    let outer_repository = git_root
        .clone()
        .filter(|git| match &root {
            Some(root) => !same_folder(git, root),
            None => !same_folder(git, &dir),
        })
        .filter(|_| !own);
    let history_blocked = match (&root, &outer_repository) {
        (Some(root), Some(outer)) if exists => Some(format!(
            "the project {} is inside the git repository {}, whose root is not the project's \
             folder; Mitcad records versions only in a repository of its own: move the \
             designs to a project of their own",
            root.display(),
            outer.display()
        )),
        _ => None,
    };
    let (remote, remotes) = match repo.as_ref().filter(|_| own) {
        Some(repo) => remotes_of(repo),
        None => (None, Vec::new()),
    };
    let listed = root.clone().unwrap_or_else(|| dir.clone());
    let (designs, truncated) = if listed.is_dir() {
        designs_in(&listed)
    } else {
        (Vec::new(), false)
    };
    let kind = if kind == FolderKind::Other && !designs.is_empty() {
        FolderKind::Designs
    } else {
        kind
    };
    let (last_design, settings, problems) = match root.as_deref().filter(|root| root.is_dir()) {
        Some(root) => {
            let (settings, problems) = ProjectSettings::read(root);
            (last_design(root), Some(settings.shared.to_json()), problems)
        }
        None => (None, None, Vec::new()),
    };
    let text = |path: &Path| path.to_string_lossy().into_owned();
    Ok(FolderInfo {
        dir: text(&dir),
        kind,
        project_root: root.as_deref().map(text),
        git_root: git_root.as_deref().map(text),
        outer_repository: outer_repository.as_deref().map(text),
        history_blocked,
        has_history,
        cloud: has_history && !remotes.is_empty(),
        remote,
        remotes,
        designs,
        designs_truncated: truncated,
        last_design,
        settings,
        problems,
    })
}

/// The remote a repository's branch follows (else `origin`, else the only
/// one), and all the remotes' names.
pub(crate) fn remotes_of(repo: &gix::Repository) -> (Option<ProjectRemote>, Vec<String>) {
    let mut names: Vec<String> = repo
        .remote_names()
        .iter()
        .map(|name| name.to_str_lossy().into_owned())
        .filter(|name| remote_url(repo, name).is_some())
        .collect();
    names.sort();
    let branch = branch_of(repo).ok().flatten();
    let upstream = branch
        .as_deref()
        .and_then(|branch| upstream_for(repo, branch));
    let remote = upstream.map(|upstream| ProjectRemote {
        url: remote_url(repo, &upstream.remote).map(|url| redact(&url)),
        upstream: Some(upstream.short()),
        name: upstream.remote,
        branch: branch.clone(),
    });
    (remote, names)
}

/// The `.mitcad` files under `root`, newest first (at most
/// [`MAX_DESIGNS`]), leaving out `.git` and `.mitcad` folders and links;
/// true when there are more, or the folder holds too much to look at whole.
fn designs_in(root: &Path) -> (Vec<Design>, bool) {
    let mut designs = Vec::new();
    let mut seen = 0;
    let mut truncated = false;
    let mut folders = vec![(root.to_path_buf(), String::new())];
    'walk: while let Some((folder, prefix)) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                truncated = true;
                break 'walk;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = format!("{prefix}{name}");
            if kind.is_dir() {
                if name != ".git" && name != ".mitcad" {
                    folders.push((entry.path(), format!("{path}/")));
                }
            } else if kind.is_file() && is_project_file(name) {
                let metadata = entry.metadata().ok();
                let seconds = metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                designs.push(Design {
                    path,
                    modified: metadata.as_ref().and_then(modified),
                    seconds,
                });
            }
        }
    }
    designs.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.path.cmp(&b.path)));
    if designs.len() > MAX_DESIGNS {
        designs.truncate(MAX_DESIGNS);
        truncated = true;
    }
    (designs, truncated)
}

/// A path relative to a project as Mitcad writes them: `/`-separated
/// names, none empty, `.` or `..`, not absolute.
pub(crate) fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The design opened last, from `.mitcad/local/recent.json`, while it is
/// in the project.
fn last_design(root: &Path) -> Option<String> {
    let file = root.join(RECENT);
    if fs::metadata(&file).ok()?.len() > MAX_STATE_SIZE {
        return None;
    }
    let value: Value = serde_json::from_slice(&fs::read(&file).ok()?).ok()?;
    let path = value.get("last_design")?.as_str()?;
    (valid_relative(path) && root.join(path).is_file()).then(|| path.to_owned())
}

/// Writes `.mitcad/local/recent.json` of the project in folder `root`.
pub(crate) fn remember(root: &Path, path: &str) -> Result<(), VcsError> {
    let file = root.join(RECENT);
    let text = serde_json::json!({"format": "mitcad-local", "version": 1, "last_design": path});
    mitcad_model::file::write_atomically(&file, format!("{text:#}\n").as_bytes())
        .map_err(|e| crate::io_error("write", &file, e))
}

/// `inspect_folder`'s answer as text.
pub(crate) fn describe(out: &mut String, answer: &Value) {
    let field = |key: &str| answer[key].as_str().unwrap_or("");
    let kind = match field("kind") {
        "missing" => "does not exist",
        "project" if answer["cloud"].as_bool() == Some(true) => "a Cloud project",
        "project" if answer["has_history"].as_bool() == Some(true) => "a Local project",
        "project" => "a project without version history",
        "inside_project" => "inside a project",
        "repository" => "a git repository (not a project)",
        "empty" => "an empty folder",
        "designs" => "a folder of designs (not a project)",
        _ => "a folder of other files",
    };
    let _ = writeln!(out, "{}: {kind}", field("dir"));
    if let Some(root) = answer["project_root"].as_str()
        && field("kind") != "project"
    {
        let _ = writeln!(out, "Project: {root}");
    }
    if let Some(outer) = answer["outer_repository"].as_str() {
        let _ = writeln!(out, "Inside the git repository {outer}");
    }
    if let Some(reason) = answer["history_blocked"].as_str() {
        let _ = writeln!(out, "No versions: {reason}");
    }
    if let Some(name) = answer["remote"]["name"].as_str() {
        let _ = writeln!(
            out,
            "Remote {name}: {} (branch {} follows {})",
            answer["remote"]["url"].as_str().unwrap_or(""),
            answer["remote"]["branch"]
                .as_str()
                .unwrap_or("(detached HEAD)"),
            answer["remote"]["upstream"].as_str().unwrap_or("")
        );
    } else if let Some(remotes) = answer["remotes"].as_array().filter(|r| !r.is_empty()) {
        let names: Vec<&str> = remotes.iter().filter_map(Value::as_str).collect();
        let _ = writeln!(
            out,
            "Remotes {}: none is followed; choose one in Project Settings (mitcad-cli remote \
             follow <folder> <name>)",
            names.join(", ")
        );
    }
    let designs = answer["designs"].as_array().cloned().unwrap_or_default();
    if !designs.is_empty() {
        let names: Vec<String> = designs
            .iter()
            .take(20)
            .map(|d| clean_text(d["path"].as_str().unwrap_or(""), 200))
            .collect();
        let more = if designs.len() > 20 || answer["designs_truncated"].as_bool() == Some(true) {
            ", ..."
        } else {
            ""
        };
        let _ = writeln!(out, "Designs: {}{more}", names.join(", "));
    }
    if let Some(last) = answer["last_design"].as_str() {
        let _ = writeln!(out, "Opened last: {last}");
    }
    if let Some(settings) = answer["settings"].as_object() {
        let locks = &settings["edit_locks"];
        let _ = if locks["enabled"].as_bool() == Some(true) {
            writeln!(
                out,
                "Edit locks: on (idle time {} min, poll {} s)",
                locks["idle_minutes"], locks["poll_seconds"]
            )
        } else {
            writeln!(out, "Edit locks: off")
        };
        let _ = match settings["live_updates"]["broker"].as_str() {
            Some(broker) => writeln!(
                out,
                "Live updates: {broker} (prefix {})",
                settings["live_updates"]["prefix"].as_str().unwrap_or("")
            ),
            None => writeln!(out, "Live updates: none"),
        };
    }
    for problem in answer["problems"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "warning: {}", problem.as_str().unwrap_or(""));
    }
}
