// SPDX-License-Identifier: MIT
//! A project's folders and designs (mitcad#144, `project_tree`), for the
//! Projects panel: the folders and designs in the project's folder, each
//! design with its latest version, its edit lock and whether it changed
//! since that version, and the project's kind and remote. No network and
//! no writes: the edit locks are the local copies the last poll left
//! (`refs/mitcad/remote-locks/`), ahead and behind count from the
//! remote-tracking reference of the last fetch or push.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use mitcad_model::file::settings::ProjectSettings;
use serde_json::{Value, json};

use super::inspect::{MAX_DESIGNS, MAX_ENTRIES, remotes_of};
use super::{MAX_NAME, clean_text, modified, rfc3339};
use crate::commit::rename_trailers;
use crate::remote::COUNT_LIMIT;
use crate::remote::locks::{
    LOCAL_PREFIX, LOCK_FILE, Lock, MAX_FILE_SIZE, format_time, is_lock_id, lock_id, read_lock,
};
use crate::{ProjectRepo, VcsError, is_design_file};

/// The empty file that keeps a folder in the versions (git records files,
/// not folders); never listed.
pub const FOLDER_PLACEHOLDER: &str = ".gitkeep";

/// A lock commit or its tree larger than this is not read.
const MAX_LOCK_OBJECT: u64 = 64 * 1024;

/// The latest version of a design: how many versions it has (as its
/// history counts them, through renames) and the commit of the newest.
#[derive(Debug, Clone, Copy)]
struct Latest {
    number: usize,
    commit: ObjectId,
}

/// What a walk of the project's folder found.
#[derive(Default)]
pub(crate) struct Walked {
    /// Relative to the project, with `/`, sorted.
    pub folders: Vec<String>,
    /// Designs: relative path and the full path.
    pub designs: Vec<(String, PathBuf)>,
    /// More entries than [`MAX_ENTRIES`] or designs than [`MAX_DESIGNS`].
    pub truncated: bool,
}

/// The folders and designs under `root` (relative to it), leaving out
/// every name starting with a dot (`.git`, `.mitcad`, the placeholder) and
/// links, as the commit's walk.
pub(crate) fn walk(root: &Path) -> Walked {
    let mut walked = Walked::default();
    let mut seen = 0;
    let mut folders = vec![(root.to_path_buf(), String::new())];
    'walk: while let Some((folder, prefix)) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                walked.truncated = true;
                break 'walk;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = format!("{prefix}{name}");
            if kind.is_dir() {
                walked.folders.push(path.clone());
                folders.push((entry.path(), format!("{path}/")));
            } else if kind.is_file() && is_design_file(name) {
                walked.designs.push((path, entry.path()));
            }
        }
    }
    walked.folders.sort();
    walked.designs.sort();
    if walked.designs.len() > MAX_DESIGNS {
        walked.designs.truncate(MAX_DESIGNS);
        walked.truncated = true;
    }
    walked
}

impl ProjectRepo {
    /// `project_tree` (commands.md, "Folders and designs").
    pub(crate) fn project_tree(&self, command: &Value) -> Result<Value, VcsError> {
        let repo = self.fresh()?;
        let (remote, remotes) = remotes_of(&repo);
        let cloud = !remotes.is_empty();
        let (ahead, behind) = if cloud {
            let info = self.remote_info(false)?;
            (info.ahead, info.behind)
        } else {
            (None, None)
        };
        let head = self.head_commit()?;
        let head_tree = match head {
            Some(head) => Some(self.repo.find_commit(head)?.tree_id()?.detach()),
            None => None,
        };
        let recorded: HashMap<String, ObjectId> = match head_tree {
            Some(tree) => self.design_files(tree)?.into_iter().collect(),
            None => HashMap::new(),
        };
        let (latest, history_truncated) = self.latest_versions(head)?;
        let all_settings = ProjectSettings::read(self.root()).0;
        let settings = all_settings.shared.edit_locks;
        let display_name = all_settings.local.name;
        let locks = if settings.enabled {
            self.known_locks()?
        } else {
            BTreeMap::new()
        };
        let session = command.get("session").and_then(Value::as_str);
        let me = self.author(command).ok();
        let walked = walk(self.root());
        let mut commits: HashMap<ObjectId, Value> = HashMap::new();
        let mut designs = Vec::new();
        for (path, full) in &walked.designs {
            let metadata = fs::metadata(full).ok();
            let in_head = recorded.get(path).copied();
            let in_folder = fs::read(full).ok().and_then(|data| {
                gix::objs::compute_hash(self.repo.object_hash(), gix::objs::Kind::Blob, &data).ok()
            });
            let version = match latest.get(path) {
                Some(found) if in_head.is_some() => {
                    let mut version = match commits.get(&found.commit) {
                        Some(known) => known.clone(),
                        None => {
                            let described = self.describe_commit(found.commit)?;
                            commits.insert(found.commit, described.clone());
                            described
                        }
                    };
                    version["number"] = json!(found.number);
                    version
                }
                _ => Value::Null,
            };
            let lock = locks.get(&lock_id(path)).map(|lock| {
                let mine = session.is_some_and(|session| session == lock.session);
                let same_owner = me
                    .as_ref()
                    .is_some_and(|me| me.email.eq_ignore_ascii_case(&lock.owner.email));
                json!({
                    "owner": {"name": lock.owner.name, "email": lock.owner.email},
                    "session": lock.session,
                    "state": lock.state,
                    "taken_at": format_time(lock.taken_at),
                    "active_at": format_time(lock.active_at),
                    "mine": mine,
                    "same_owner": same_owner,
                })
            });
            designs.push(json!({
                "path": path,
                "modified": metadata.as_ref().and_then(modified),
                "recorded": in_head.is_some(),
                "changed": in_head.is_none() || in_head != in_folder,
                "version": version,
                "lock": lock,
            }));
        }
        let name = self
            .root()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let kind = if cloud { "cloud" } else { "local" };
        Ok(json!({
            "root": self.root().to_string_lossy(),
            "name": name,
            "display_name": display_name,
            "kind": kind,
            "branch": self.branch()?,
            "head": head.map(|id| id.to_string()),
            "remote": remote,
            "remotes": remotes,
            "ahead": ahead,
            "behind": behind,
            "edit_locks": settings.enabled,
            "folders": walked.folders,
            "designs": designs,
            "truncated": walked.truncated,
            "history_truncated": history_truncated,
            "error": null,
        }))
    }

    /// The id, time and author of a version.
    fn describe_commit(&self, id: ObjectId) -> Result<Value, VcsError> {
        let commit = self.repo.find_commit(id)?;
        let author = commit.author()?;
        let time = author.time()?;
        Ok(json!({
            "id": id.to_string(),
            "short_id": id.to_hex_with_len(7).to_string(),
            "time": time.seconds,
            "date": rfc3339(time.seconds),
            "author": {
                "name": clean_text(&author.name.to_str_lossy(), MAX_NAME),
                "email": clean_text(&author.email.to_str_lossy(), MAX_NAME),
            },
        }))
    }

    /// The latest version of every design path on HEAD's first-parent
    /// chain (the last [`COUNT_LIMIT`] commits; true when there are
    /// more), counted as `history` counts them: each commit that changed
    /// the path, and a recorded move or equal-content rename carries the
    /// count of the path it came from.
    fn latest_versions(
        &self,
        head: Option<ObjectId>,
    ) -> Result<(HashMap<String, Latest>, bool), VcsError> {
        let mut latest: HashMap<String, Latest> = HashMap::new();
        let Some(head) = head else {
            return Ok((latest, false));
        };
        let mut chain = Vec::new();
        let mut truncated = false;
        for info in self.repo.rev_walk([head]).first_parent_only().all()? {
            let info = info?;
            if chain.len() >= COUNT_LIMIT {
                truncated = true;
                break;
            }
            chain.push((info.id, info.parent_ids().next().map(|p| p.detach())));
        }
        for (id, parent) in chain.into_iter().rev() {
            let commit = self.repo.find_commit(id)?;
            let tree = commit.tree_id()?.detach();
            let message = commit.message_raw()?.to_str_lossy();
            let renames = rename_trailers(&message);
            let old = match parent {
                Some(parent) => Some(self.repo.find_commit(parent)?.tree_id()?.detach()),
                None => None,
            };
            let mut differences = self.differences(old, Some(tree))?;
            // Moving onto a name deleted only in the working folder can
            // even leave that name's recorded bytes unchanged. Its design
            // identity still changes, so count the move as history does.
            for (from, to) in &renames {
                if is_design_file(to)
                    && !differences.iter().any(|d| &d.path == to)
                    && differences
                        .iter()
                        .any(|d| &d.path == from && d.old.is_some() && d.new.is_none())
                    && self.tree_entry(tree, from)?.is_none()
                    && let Some(blob) = self.blob_entry(tree, to)?
                {
                    differences.push(crate::tree::Difference {
                        path: to.clone(),
                        old: Some(blob),
                        new: Some(blob),
                        mode: None,
                    });
                }
            }
            let mut updates = Vec::new();
            for difference in differences.iter().filter(|d| is_design_file(&d.path)) {
                // A recorded move takes its source's history even when
                // the destination used to name a different design.
                let mut source = None;
                if difference.new.is_some() {
                    for (from, to) in &renames {
                        if to == &difference.path
                            && differences
                                .iter()
                                .any(|d| &d.path == from && d.old.is_some() && d.new.is_none())
                            && self.tree_entry(tree, from)?.is_none()
                        {
                            source = Some(from.as_str());
                            break;
                        }
                    }
                }
                let before = if let Some(source) = source {
                    latest.get(source)
                } else {
                    match (difference.old, difference.new) {
                        // Without metadata, only an added design can
                        // follow an equal-content rename, as in `history`.
                        (None, Some(blob)) => differences
                            .iter()
                            .find(|d| d.old == Some(blob) && d.new.is_none())
                            .map_or_else(
                                || latest.get(&difference.path),
                                |from| latest.get(&from.path),
                            ),
                        _ => latest.get(&difference.path),
                    }
                };
                let number = before.map_or(0, |known| known.number) + 1;
                updates.push((difference.path.clone(), number));
            }
            for (path, number) in updates {
                latest.insert(path, Latest { number, commit: id });
            }
        }
        Ok((latest, truncated))
    }

    /// The edit locks as the last poll saw them (the local copies of the
    /// remote's lock refs), by lock id; unreadable ones left out. No
    /// network.
    pub(crate) fn known_locks(&self) -> Result<BTreeMap<String, Lock>, VcsError> {
        let repo = self.fresh()?;
        let prefix = format!("{LOCAL_PREFIX}locks/");
        let mut out = BTreeMap::new();
        for reference in repo.references()?.all()? {
            let reference = reference?;
            let name = reference.name().as_bstr().to_str_lossy().into_owned();
            let Some(id) = name.strip_prefix(&prefix) else {
                continue;
            };
            if !is_lock_id(id) {
                continue;
            }
            let Some(oid) = reference.try_id().map(|oid| oid.detach()) else {
                continue;
            };
            if let Some(lock) = read_known_lock(&repo, oid).filter(|lock| lock_id(&lock.path) == id)
            {
                out.insert(id.to_owned(), lock);
            }
        }
        Ok(out)
    }
}

/// A lock commit's `lock.json`, read with the limits of the lock reader:
/// a small parentless commit and tree, the file at most
/// [`MAX_FILE_SIZE`] bytes.
fn read_known_lock(repo: &gix::Repository, oid: ObjectId) -> Option<Lock> {
    let header = |oid: ObjectId| {
        repo.try_find_header(oid)
            .ok()
            .flatten()
            .map(|header| (header.kind(), header.size()))
    };
    let (kind, size) = header(oid)?;
    if kind != gix::object::Kind::Commit || size > MAX_LOCK_OBJECT {
        return None;
    }
    let commit = repo.find_commit(oid).ok()?;
    if commit.parent_ids().next().is_some() {
        return None;
    }
    let tree = commit.tree_id().ok()?.detach();
    let (kind, size) = header(tree)?;
    if kind != gix::object::Kind::Tree || size > MAX_LOCK_OBJECT {
        return None;
    }
    let tree = repo.find_tree(tree).ok()?;
    let decoded = tree.decode().ok()?;
    let entry = decoded
        .entries
        .iter()
        .find(|entry| entry.filename == LOCK_FILE)?;
    if !entry.mode.is_blob() {
        return None;
    }
    let blob = entry.oid.to_owned();
    let (kind, size) = header(blob)?;
    if kind != gix::object::Kind::Blob || size > MAX_FILE_SIZE as u64 {
        return None;
    }
    let data = repo.find_blob(blob).ok()?.data.clone();
    read_lock(&data).ok()
}

/// `project_tree`'s answer as text: the project, then its folders and
/// designs in path order.
pub(crate) fn describe(out: &mut String, answer: &Value) {
    use std::fmt::Write;
    let kind = if answer["kind"] == "cloud" {
        "Cloud"
    } else {
        "Local"
    };
    let _ = write!(
        out,
        "{kind} project {}",
        answer["root"].as_str().unwrap_or("")
    );
    if let Some(url) = answer["remote"]["url"].as_str() {
        let _ = write!(out, " ({url})");
    }
    if let Some(name) = answer["display_name"].as_str() {
        let _ = write!(out, ", shown as \"{name}\"");
    }
    let count = |key: &str| answer[key].as_u64().unwrap_or(0);
    // One whole text for one and for several: no "(s)" (mitcad#1759).
    match count("ahead") {
        0 => {}
        1 => out.push_str(", one version to send"),
        n => {
            let _ = write!(out, ", {n} versions to send");
        }
    }
    if count("behind") > 0 {
        let _ = write!(out, ", {} newer on the remote", count("behind"));
    }
    out.push('\n');
    let mut lines: Vec<(String, String)> = Vec::new();
    for folder in answer["folders"].as_array().into_iter().flatten() {
        let path = folder.as_str().unwrap_or("");
        lines.push((format!("{path}/"), format!("{path}/")));
    }
    for design in answer["designs"].as_array().into_iter().flatten() {
        let path = design["path"].as_str().unwrap_or("");
        let mut line = path.to_owned();
        let version = &design["version"];
        if let Some(number) = version["number"].as_u64() {
            let date = version["date"].as_str().unwrap_or("");
            let _ = write!(
                line,
                "  v{number} {} by {}",
                date.get(..10).unwrap_or(date),
                version["author"]["name"].as_str().unwrap_or("")
            );
        } else {
            line.push_str("  not recorded");
        }
        if design["recorded"] == true && design["changed"] == true {
            line.push_str(", changed");
        }
        if let Some(owner) = design["lock"]["owner"]["name"].as_str() {
            let _ = write!(line, ", locked by {owner}");
        }
        lines.push((path.to_owned(), line));
    }
    lines.sort();
    if lines.is_empty() {
        out.push_str("No folders or designs\n");
    }
    for (_, line) in lines {
        let _ = writeln!(out, "  {line}");
    }
    if answer["truncated"] == true {
        out.push_str("  ... (more than listed)\n");
    }
}
