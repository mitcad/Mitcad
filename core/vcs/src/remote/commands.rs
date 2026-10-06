// SPDX-License-Identifier: MIT
//! The remote repositories' JSON commands (`core/model/src/api/commands.md`,
//! "Remote repositories"). Every answer has `error` (null, or `class`,
//! `message` and `detail`) and `log` (the git commands run, credentials
//! hidden): a failure of the network or of signing in is an answer the
//! application shows, not a failed command. As text (`mitcad-cli`) such a
//! failure is an error.

use std::fmt::Write;

use serde::Serialize;
use serde_json::{Value, json};

use super::{
    Control, DEFAULT_REMOTE, ErrorClass, RemoteError, Resolution, SyncOptions, git_info, redact,
};
use crate::api::{optional, text};
use crate::{ProjectRepo, VcsError};

/// The options of the `sync` command: `resolutions` ({path: "mine" |
/// "theirs" | "copy"}), `resolve_all`, `fetch` and `push` (default true).
fn sync_options(command: &Value) -> Result<SyncOptions, VcsError> {
    let choice = |text: &str| {
        Resolution::parse(text).ok_or_else(|| {
            VcsError::Command(format!(
                "'{text}' is not a choice for a file changed on both sides: mine, theirs or copy"
            ))
        })
    };
    let flag = |key: &str| command.get(key).and_then(Value::as_bool).unwrap_or(true);
    let mut options = SyncOptions {
        fetch: flag("fetch"),
        push: flag("push"),
        ..SyncOptions::default()
    };
    if let Some(resolutions) = command.get("resolutions").filter(|value| !value.is_null()) {
        let resolutions = resolutions.as_object().ok_or_else(|| {
            VcsError::Command(
                "\"resolutions\" is an object: {\"<path>\": \"mine\" | \"theirs\" | \"copy\"}"
                    .to_owned(),
            )
        })?;
        for (path, value) in resolutions {
            let text = value.as_str().unwrap_or_default();
            options.resolutions.insert(path.clone(), choice(text)?);
        }
    }
    if let Some(all) = optional(command, "resolve_all") {
        options.resolve_all = Some(choice(all)?);
    }
    Ok(options)
}

/// An outcome as JSON with `error` null; a remote failure as `failed`'s
/// JSON with the error; another error stays one.
pub(crate) fn answer<T: Serialize>(
    result: Result<T, VcsError>,
    failed: impl FnOnce(&RemoteError) -> Value,
) -> Result<(Value, Option<RemoteError>), VcsError> {
    match result {
        Ok(outcome) => {
            let mut value = serde_json::to_value(outcome).expect("serializable");
            value["error"] = Value::Null;
            Ok((value, None))
        }
        Err(VcsError::Remote(error)) => {
            let mut value = failed(&error);
            value["error"] = json!(error);
            Ok((value, Some(error)))
        }
        Err(other) => Err(other),
    }
}

impl ProjectRepo {
    /// Runs the remote command `name`: its answer and the failure it
    /// reports. None when `name` is not a remote command.
    pub(crate) fn remote_command(
        &self,
        name: &str,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<Option<(Value, Option<RemoteError>)>, VcsError> {
        let flag =
            |key: &str, default: bool| command.get(key).and_then(Value::as_bool).unwrap_or(default);
        let remote = optional(command, "name").unwrap_or(DEFAULT_REMOTE);
        let (mut value, error) = match name {
            "git_info" => {
                let info = git_info(self.git.borrow().as_ref());
                let error = info.error.clone();
                (json!(info), error)
            }
            "remote_info" => answer(self.remote_info(flag("links", false)), |_| json!({}))?,
            "remote_check" => {
                let url = text(command, "url")?;
                answer(self.remote_check(url, control), |_| {
                    json!({"url": redact(url), "reachable": false, "empty": null,
                           "default_branch": null, "head": null, "has_project": null,
                           "related": null})
                })?
            }
            "remote_set" => {
                let url = text(command, "url")?;
                let author = self.author(command)?;
                answer(
                    self.remote_set(url, remote, &author),
                    |_| json!({"name": remote, "url": redact(url)}),
                )?
            }
            "remote_remove" => answer(
                self.remote_remove(remote)
                    .map(|removed| json!({"name": remote, "removed": removed})),
                |_| json!({"name": remote, "removed": false}),
            )?,
            "connect" => {
                let url = text(command, "url")?;
                let author = self.author(command)?;
                match self.connect(url, remote, &author, flag("push", true), control) {
                    Ok(connected) => {
                        let error = connected.error.clone();
                        let mut value = json!(connected);
                        value["error"] = json!(error);
                        (value, error)
                    }
                    Err(error) => answer::<Value>(
                        Err(error),
                        |_| json!({"check": null, "set": null, "fetch": null, "push": null}),
                    )?,
                }
            }
            "fetch" => answer(self.fetch(control), |_| self.local_counts())?,
            "push" => {
                let (mut value, error) = answer(self.push(control), |_| json!({"pushed": false}))?;
                value["rejected"] = json!(
                    error
                        .as_ref()
                        .is_some_and(|e| e.class == ErrorClass::Rejected)
                );
                (value, error)
            }
            // Sync and conflicts.
            "sync_plan" => answer(
                self.sync_plan(
                    command
                        .get("fetch")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    control,
                ),
                |_| json!({"case": null, "local": [], "remote": [], "conflicts": [], "uncommitted": []}),
            )?,
            "sync" => {
                let options = sync_options(command)?;
                // Only a replay records versions, so only it needs a committer.
                let committer = match self.author(command) {
                    Ok(committer) => Some(committer),
                    Err(VcsError::NoIdentity) => None,
                    Err(error) => return Err(error),
                };
                match self.sync(&options, committer.as_ref(), control) {
                    Ok(outcome) => {
                        let error = outcome.error.clone();
                        let mut value = json!(outcome);
                        value["error"] = json!(error);
                        (value, error)
                    }
                    Err(error) => answer::<Value>(Err(error), |_| {
                        json!({"case": null, "replayed": [], "copies": [], "conflicts": [],
                               "pushed": false})
                    })?,
                }
            }
            // The remote's newer versions (the application's notice).
            "incoming" => {
                let path = optional(command, "path").map(std::path::Path::new);
                answer(
                    self.incoming(path),
                    |_| json!({"versions": [], "file_versions": [], "differs": false}),
                )?
            }
            _ => return Ok(None),
        };
        value["log"] = json!(self.take_remote_log());
        Ok(Some((value, error)))
    }

    /// The versions ahead and behind by the local references, for the
    /// answer of a fetch that failed.
    fn local_counts(&self) -> Value {
        match self.remote_info(false) {
            Ok(info) => json!({"remote": info.name, "updated": [], "ahead": info.ahead,
                               "behind": info.behind}),
            Err(_) => json!({"updated": []}),
        }
    }
}

/// `n version(s)`.
fn versions(n: u64) -> String {
    if n == 1 {
        "1 version".to_owned()
    } else {
        format!("{n} versions")
    }
}

/// The first 7 digits of an id.
fn short(id: &Value) -> &str {
    let id = id.as_str().unwrap_or("");
    &id[..7.min(id.len())]
}

/// Where the branch stands against its upstream (`ahead`, `behind`).
fn state(answer: &Value) -> String {
    match (answer["ahead"].as_u64(), answer["behind"].as_u64()) {
        (Some(0), Some(0)) => "up to date".to_owned(),
        (Some(ahead), Some(0)) => format!("{} to push", versions(ahead)),
        (Some(0), Some(behind)) => format!("{} newer on the remote", versions(behind)),
        (Some(ahead), Some(behind)) => format!(
            "{} to push and {} newer on the remote: sync to combine them",
            versions(ahead),
            versions(behind)
        ),
        _ => "not fetched or pushed yet".to_owned(),
    }
}

/// When a fetch or push was last tried, and how it went.
fn attempt(out: &mut String, label: &str, attempt: &Value) {
    if attempt.is_null() {
        return;
    }
    let outcome = attempt["error"]["message"].as_str().unwrap_or("ok");
    let _ = writeln!(
        out,
        "{label}: {} ({outcome})",
        attempt["date"].as_str().unwrap_or("")
    );
}

fn describe_set(out: &mut String, set: &Value) {
    let _ = writeln!(
        out,
        "Remote {} set to {}; branch {} follows {}",
        set["name"].as_str().unwrap_or(""),
        set["url"].as_str().unwrap_or(""),
        set["branch"].as_str().unwrap_or(""),
        set["upstream"].as_str().unwrap_or("")
    );
    if !set["commit"].is_null() {
        let written: Vec<&str> = set["written"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let _ = writeln!(
            out,
            "Recorded version {}: {} ({})",
            short(&set["commit"]),
            super::CONFIGURE_MESSAGE,
            written.join(", ")
        );
    }
}

fn describe_fetch(out: &mut String, fetch: &Value) {
    let updated = fetch["updated"].as_array().map_or(0, Vec::len);
    let what = match updated {
        0 => "nothing new".to_owned(),
        1 => "1 branch updated".to_owned(),
        n => format!("{n} branches updated"),
    };
    let _ = writeln!(
        out,
        "Fetched from {}: {what}; {}",
        fetch["remote"].as_str().unwrap_or(""),
        state(fetch)
    );
}

fn describe_push(out: &mut String, push: &Value) {
    let upstream = format!(
        "{}/{}",
        push["remote"].as_str().unwrap_or(""),
        push["branch"].as_str().unwrap_or("")
    );
    let _ = if push["pushed"].as_bool() == Some(true) {
        match push["versions"].as_u64() {
            Some(n) => writeln!(out, "Pushed {} to {upstream}", versions(n)),
            None => writeln!(out, "Pushed to {upstream}"),
        }
    } else {
        writeln!(out, "Nothing to push: {upstream} has every version")
    };
    warnings(out, &push["warnings"]);
}

fn warnings(out: &mut String, warnings: &Value) {
    for warning in warnings.as_array().into_iter().flatten() {
        let _ = writeln!(out, "warning: {}", warning.as_str().unwrap_or(""));
    }
}

/// The texts of a list.
fn texts(list: &Value) -> Vec<&str> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// A version of a sync: `abc1234 2026-10-05 14:03 Name: summary`.
fn describe_version(version: &Value) -> String {
    let date = version["date"].as_str().unwrap_or("");
    format!(
        "{} {} {}: {}",
        short(&version["id"]),
        date.get(..16).unwrap_or(date),
        version["author"]["name"].as_str().unwrap_or(""),
        version["summary"].as_str().unwrap_or("")
    )
}

/// The files changed on both sides, with their versions and the copy's
/// name.
fn describe_conflicts(out: &mut String, conflicts: &Value) {
    for conflict in conflicts.as_array().into_iter().flatten() {
        let how = match conflict["kind"].as_str().unwrap_or("") {
            "deleted_theirs" => "deleted on the remote, changed here",
            "deleted_mine" => "deleted here, changed on the remote",
            "added_both" => "added on both sides",
            _ => "changed on both sides",
        };
        let _ = writeln!(
            out,
            "Conflict: {}: {how}",
            conflict["path"].as_str().unwrap_or("")
        );
        for (label, side) in [
            ("mine:  ", &conflict["mine"]),
            ("theirs:", &conflict["theirs"]),
        ] {
            let mut line = format!("  {label} ");
            if side["version"].is_null() {
                line.push_str("(no version)");
            } else {
                line.push_str(&describe_version(&side["version"]));
            }
            if side["blob"].is_null() {
                line.push_str(" (deleted)");
            }
            let count = side["versions"].as_u64().unwrap_or(0);
            if count > 1 {
                let _ = write!(line, " (and {} before)", versions(count - 1));
            }
            let _ = writeln!(out, "{line}");
        }
        if let Some(copy) = conflict["copy"].as_str() {
            let _ = writeln!(out, "  copy:   {copy}");
        }
    }
}

/// The paths a sync changed in the folder, B-rep files counted.
fn describe_paths(out: &mut String, label: &str, paths: &Value) {
    let paths = texts(paths);
    let (store, files): (Vec<&str>, Vec<&str>) = paths
        .iter()
        .partition(|path| crate::tree::brep_sha256(path).is_some());
    if paths.is_empty() {
        return;
    }
    let mut line = format!("{label}: {}", files.join(", "));
    if !store.is_empty() {
        let n = store.len();
        let what = if n == 1 { "B-rep file" } else { "B-rep files" };
        if files.is_empty() {
            let _ = write!(line, "{n} {what}");
        } else {
            let _ = write!(line, " and {n} {what}");
        }
    }
    let _ = writeln!(out, "{line}");
}

/// What a sync would do.
fn describe_plan(out: &mut String, plan: &Value) {
    if !plan["fetch"].is_null() {
        describe_fetch(out, &plan["fetch"]);
    }
    let upstream = plan["upstream"].as_str().unwrap_or("");
    let count = |key: &str| plan[key].as_u64().unwrap_or(0);
    let _ = match plan["case"].as_str().unwrap_or("") {
        "up_to_date" => writeln!(out, "Up to date with {upstream}"),
        "push" => writeln!(
            out,
            "Sync will push {} to {upstream}",
            versions(count("ahead"))
        ),
        "fast_forward" => writeln!(
            out,
            "Sync will take {} newer on {upstream}",
            versions(count("behind"))
        ),
        "replay" => writeln!(
            out,
            "Sync will replay {} of this project after {} newer on {upstream}",
            versions(count("ahead")),
            versions(count("behind"))
        ),
        _ => writeln!(
            out,
            "Sync cannot run: {}",
            plan["reason"].as_str().unwrap_or("")
        ),
    };
    describe_conflicts(out, &plan["conflicts"]);
    let uncommitted = texts(&plan["uncommitted"]);
    if !uncommitted.is_empty() {
        let _ = writeln!(
            out,
            "Changed in the folder, not recorded as versions: {}",
            uncommitted.join(", ")
        );
    }
}

/// What a sync did, and why it stopped when it did.
fn describe_sync(out: &mut String, sync: &Value) {
    if !sync["fetch"].is_null() {
        describe_fetch(out, &sync["fetch"]);
    }
    let upstream = sync["upstream"].as_str().unwrap_or("");
    let replayed = sync["replayed"].as_array().cloned().unwrap_or_default();
    if !replayed.is_empty() {
        let left_out = replayed.iter().filter(|r| r["to"].is_null()).count();
        let mut line = format!(
            "Replayed {} after the newer ones on {upstream}",
            versions((replayed.len() - left_out) as u64)
        );
        if left_out > 0 {
            let _ = write!(
                line,
                " ({} left out: all of its changes were taken from the remote)",
                versions(left_out as u64)
            );
        }
        let _ = writeln!(out, "{line}");
    }
    let copies = sync["copies"].as_array().cloned().unwrap_or_default();
    for (path, choice) in sync["resolved"].as_object().into_iter().flatten() {
        let _ = match choice.as_str().unwrap_or("") {
            "mine" => writeln!(out, "Kept mine: {path}"),
            "theirs" => writeln!(out, "Took theirs: {path}"),
            _ => match copies.iter().find(|c| c["path"].as_str() == Some(path)) {
                Some(copy) => writeln!(
                    out,
                    "Saved mine as a copy: {} (theirs: {path})",
                    copy["copy"].as_str().unwrap_or("")
                ),
                None => writeln!(out, "Took theirs: {path} (mine was deleted)"),
            },
        };
    }
    if let Some(backup) = sync["backup"].as_str() {
        let _ = writeln!(out, "Versions before the sync kept in {backup}");
    }
    describe_paths(out, "Files updated", &sync["changed_paths"]);
    if !sync["push"].is_null() {
        describe_push(out, &sync["push"]);
    }
    warnings(out, &sync["warnings"]);
    describe_conflicts(out, &sync["conflicts"]);
    if sync["conflicts"]
        .as_array()
        .is_some_and(|conflicts| !conflicts.is_empty())
    {
        let _ = writeln!(
            out,
            "Choose for each file: --resolve <path>=mine|theirs|copy (copy keeps both)"
        );
    }
    if let Some(message) = sync["error"]["message"].as_str() {
        let _ = writeln!(out, "Sync stopped: {message}");
    }
    if !sync["branch"].is_null() {
        let _ = writeln!(
            out,
            "Branch {} follows {upstream}: {}",
            sync["branch"].as_str().unwrap_or(""),
            state(sync)
        );
    }
}

/// git's program and version as text.
pub(crate) fn describe_git(info: &Value) -> String {
    let mut out = String::new();
    if let Some(message) = info["error"]["message"].as_str() {
        let _ = writeln!(out, "No git: {message}");
        return out;
    }
    let lfs = match info["lfs"].as_str() {
        Some(version) => format!("git-lfs {version}"),
        None => "no git-lfs".to_owned(),
    };
    let _ = writeln!(
        out,
        "git {} ({}), {lfs}",
        info["version"].as_str().unwrap_or(""),
        info["path"].as_str().unwrap_or("")
    );
    if info["supported"].as_bool() != Some(true) {
        let _ = writeln!(
            out,
            "warning: Mitcad's remote repositories are tested with git {} or newer",
            info["minimum"].as_str().unwrap_or("")
        );
    }
    out
}

/// A project opened from a remote as text.
pub(crate) fn describe_clone(answer: &Value) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Opened {} into {}: branch {}, latest version {}",
        answer["url"].as_str().unwrap_or(""),
        answer["root"].as_str().unwrap_or(""),
        answer["branch"].as_str().unwrap_or("(detached HEAD)"),
        short(&answer["head"])
    );
    let files: Vec<&str> = answer["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let _ = writeln!(out, "Project files: {}", files.join(", "));
    out
}

/// A remote command's answer as text; None for another command.
pub(crate) fn describe(name: &str, answer: &Value) -> Option<String> {
    let mut out = String::new();
    let field = |key: &str| answer[key].as_str().unwrap_or("");
    match name {
        "git_info" => return Some(describe_git(answer)),
        "remote_info" => {
            if answer["name"].is_null() {
                let _ = writeln!(
                    out,
                    "No remote repository: connect one with mitcad-cli remote add <folder> <url>"
                );
                return Some(out);
            }
            let _ = writeln!(out, "Remote {}: {}", field("name"), field("url"));
            let _ = match (answer["branch"].as_str(), answer["upstream"].as_str()) {
                (Some(branch), Some(upstream)) => {
                    writeln!(out, "Branch {branch} follows {upstream}: {}", state(answer))
                }
                (Some(branch), None) => writeln!(out, "Branch {branch} follows no remote branch"),
                (None, _) => writeln!(out, "HEAD is detached (not on a branch)"),
            };
            attempt(&mut out, "Last fetch", &answer["last_fetch"]);
            attempt(&mut out, "Last push", &answer["last_push"]);
            attempt(&mut out, "Last sync", &answer["last_sync"]);
            for link in answer["external_links"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "warning: {} links {} (component {}), outside the project: other copies \
                     of the project do not have it",
                    link["file"].as_str().unwrap_or(""),
                    link["path"].as_str().unwrap_or(""),
                    link["component"].as_str().unwrap_or("")
                );
            }
        }
        "remote_check" => {
            let _ = writeln!(out, "Reachable: {}", field("url"));
            if answer["empty"].as_bool() == Some(true) {
                let _ = writeln!(
                    out,
                    "The repository is empty: the project can be connected to it"
                );
            } else {
                let project = if answer["has_project"].as_bool() == Some(true) {
                    "a Mitcad project"
                } else {
                    "no Mitcad project"
                };
                let history = if answer["related"].as_bool() == Some(true) {
                    "this project's history"
                } else {
                    "another history"
                };
                let _ = writeln!(
                    out,
                    "Branch {} ({}): {project} with {history}",
                    field("default_branch"),
                    short(&answer["head"])
                );
            }
        }
        "remote_set" => describe_set(&mut out, answer),
        "remote_remove" => {
            let _ = if answer["removed"].as_bool() == Some(true) {
                writeln!(out, "Remote {} removed", field("name"))
            } else {
                writeln!(out, "No remote {}", field("name"))
            };
        }
        "connect" => {
            describe_set(&mut out, &answer["set"]);
            if !answer["fetch"].is_null() {
                describe_fetch(&mut out, &answer["fetch"]);
            }
            if !answer["push"].is_null() {
                describe_push(&mut out, &answer["push"]);
            }
            let _ = writeln!(
                out,
                "Branch {} follows {}: {}",
                answer["set"]["branch"].as_str().unwrap_or(""),
                answer["set"]["upstream"].as_str().unwrap_or(""),
                state(answer)
            );
        }
        "fetch" => describe_fetch(&mut out, answer),
        "push" => describe_push(&mut out, answer),
        // Sync and conflicts.
        "sync_plan" => describe_plan(&mut out, answer),
        "sync" => describe_sync(&mut out, answer),
        // The remote's newer versions.
        "incoming" => {
            let _ = writeln!(out, "{}: {}", field("upstream"), state(answer));
            let (list, what) = if answer["path"].is_string() {
                (&answer["file_versions"], "Newer on the remote")
            } else {
                (&answer["versions"], "On the remote")
            };
            for version in list.as_array().into_iter().flatten() {
                let _ = writeln!(out, "{what}: {}", describe_version(version));
            }
            if answer["path"].is_string() {
                let theirs = answer["file_versions"]
                    .as_array()
                    .is_some_and(|list| !list.is_empty());
                let mine = answer["changed_here"].as_bool() == Some(true);
                let how = match (answer["differs"].as_bool() == Some(true), theirs, mine) {
                    (false, _, _) => "the same on the remote",
                    (true, true, true) => {
                        "changed here and on the remote: a sync asks what to keep"
                    }
                    (true, true, false) => "a newer version is on the remote",
                    (true, false, _) => "changed here, not on the remote yet",
                };
                let _ = writeln!(out, "{}: {how}", field("path"));
            }
        }
        _ => return None,
    }
    Some(out)
}

/// A remote command's answer (JSON) as text, failures too: what a sync did
/// before it stopped and why, else the failure's message.
pub(crate) fn describe_answer(name: &str, answer: &Value) -> String {
    let failed = !answer["error"].is_null();
    if failed && name != "sync" {
        return format!(
            "error: {}\n",
            answer["error"]["message"].as_str().unwrap_or("")
        );
    }
    describe(name, answer).unwrap_or_default()
}
