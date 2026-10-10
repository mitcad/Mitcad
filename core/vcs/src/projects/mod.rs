// SPDX-License-Identifier: MIT
//! Local and Cloud projects (mitcad#89). A project is a folder with
//! `.mitcad/project.json`; it is **Local** when the folder is the root of a
//! git repository without a remote and **Cloud** when the repository has
//! one. The kind is never stored: [`inspect_folder`] reads it from the
//! folder, so a project cloned with any git tool is Cloud when opened.
//!
//! - [`inspect_folder`]: what a folder is, without the network and without
//!   writing (`inspect.rs`).
//! - [`check_remote`]: what a remote holds, without a project, its objects
//!   read in a temporary repository that is removed (`check.rs`).
//! - [`create_project`]: a project made in a folder with its first version,
//!   and for a Cloud project its remote, the remote's files taken in and a
//!   push; [`clone_project`] with `adopt`; [`init_bare`] for a shared
//!   folder (`create.rs`).
//! - The commands of a project ([`ProjectRepo::command`]): its settings,
//!   the repository's own author, the design opened last (`settings.rs`),
//!   and `connect` onto a remote's files (`onto.rs`).
//! - SSH servers: [`host_keys`], [`trust_host_key`] and [`ssh_public_key`]
//!   (`hostkeys.rs`).
//!
//! What a remote or a server sends is untrusted: names shown to people are
//! cleaned ([`clean_text`]) and host keys are parsed and checked here; the
//! application gets typed, checked values.
//!
//! The JSON commands without a project are [`run`] (bridge:
//! `projects_command`; `core/model/src/api/commands.md`, "Projects").

mod check;
mod create;
mod hostkeys;
mod inspect;
mod onto;
mod settings;

use std::fmt::Write;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use mitcad_model::file::PROJECT_MARKER;
use mitcad_model::file::settings::SharedSettings;
use serde::Serialize;
use serde_json::{Value, json};

use crate::api::{optional, text};
use crate::remote::commands::answer;
use crate::remote::{COUNT_LIMIT, Control, GitCli, RemoteError, redact};
use crate::{Identity, ProjectRepo, VcsError};

pub use check::check_remote;
pub use create::{
    ADOPT_MESSAGE, AdoptOutcome, BareOutcome, CreateOptions, Created, clone_project,
    create_project, init_bare,
};
#[cfg(test)]
pub(crate) use hostkeys::hashed_host;
pub use hostkeys::{
    HostKey, HostKeys, PublicKey, Published, Trusted, fingerprint, host_keys, parse_keyscan,
    ssh_public_key, trust_host_key, trust_host_keys,
};
pub(crate) use inspect::remotes_of;
pub use inspect::{Design, FolderInfo, FolderKind, ProjectRemote, inspect_folder};

/// Names at the root of a remote's latest version that a check lists.
const MAX_FILES: usize = 100;
/// The longest name shown of what a remote holds (characters).
pub(crate) const MAX_NAME: usize = 100;

/// Where the commands find the programs and files they use; the defaults
/// find them (tests: a fake git, a fake `ssh-keyscan`, a home of their
/// own).
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The git program (else [`GitCli::find`]).
    pub git: Option<GitCli>,
    /// `ssh-keyscan` (else `MITCAD_SSH_KEYSCAN`, the one of the git
    /// installation, `PATH`, and Windows' OpenSSH).
    pub keyscan: Option<PathBuf>,
    /// The user's home folder, whose `.ssh` holds `known_hosts` and the
    /// public keys (else `HOME`, and on Windows `USERPROFILE`).
    pub home: Option<PathBuf>,
}

impl Context {
    pub(crate) fn git(&self) -> Result<GitCli, RemoteError> {
        self.git.clone().map_or_else(GitCli::find, Ok)
    }
}

/// The latest version of a remote's branch, for Open from Cloud's summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Latest {
    /// The author's time: seconds since 1970 (UTC) and as RFC 3339 (UTC).
    pub time: i64,
    pub date: String,
    /// The author's name, cleaned for display.
    pub author: String,
}

/// What the latest version of a remote's branch holds.
pub(crate) struct Summary {
    pub has_project: bool,
    pub files: Vec<String>,
    pub latest: Option<Latest>,
    pub versions: usize,
}

impl ProjectRepo {
    /// What commit `tip` of `repo` (this repository, or one read for a
    /// check) holds: a project at its root, the names there, its author
    /// and time, and the commits up to it (counted up to 10 000).
    pub(crate) fn summary(
        &self,
        repo: &gix::Repository,
        tip: ObjectId,
    ) -> Result<Summary, VcsError> {
        let commit = repo.find_commit(tip)?;
        let tree = commit.tree_id()?.detach();
        let has_project = self.blob_entry(tree, PROJECT_MARKER)?.is_some();
        let files = self
            .entries(tree)?
            .keys()
            .take(MAX_FILES)
            .map(|name| clean_text(&name.to_str_lossy(), MAX_NAME))
            .collect();
        let author = commit.author()?;
        let time = author.time()?;
        let latest = Some(Latest {
            time: time.seconds,
            date: rfc3339(time.seconds),
            author: clean_text(&author.name.to_str_lossy(), MAX_NAME),
        });
        let mut versions = 0;
        for info in repo.rev_walk([tip]).all()? {
            info?;
            versions += 1;
            if versions >= COUNT_LIMIT {
                break;
            }
        }
        Ok(Summary {
            has_project,
            files,
            latest,
            versions,
        })
    }
}

/// Text from elsewhere (a remote's names, a server's output) as it may be
/// shown: control characters and invisible format characters (such as
/// bidirectional overrides and zero-width spaces) removed, whitespace
/// collapsed to single spaces, trimmed, at most `limit` characters.
pub fn clean_text(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = count > 0;
            continue;
        }
        if c.is_control() || invisible(c) {
            continue;
        }
        if count + usize::from(space) + 1 > limit {
            break;
        }
        if space {
            out.push(' ');
            count += 1;
            space = false;
        }
        out.push(c);
        count += 1;
    }
    out
}

/// Characters that change how text around them looks without being seen
/// (Unicode's format characters that matter in names).
fn invisible(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}'
        | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
        | '\u{FFA0}' | '\u{FFF0}'..='\u{FFFB}' | '\u{E0000}'..='\u{E007F}')
}

/// Seconds since 1970 as RFC 3339 in UTC (`2026-10-08T12:34:56Z`).
pub fn rfc3339(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let second = seconds.rem_euclid(86_400);
    // Days to a civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        second / 3600,
        second % 3600 / 60,
        second % 60
    )
}

/// The time a file was last changed, as RFC 3339 (UTC).
pub(crate) fn modified(metadata: &std::fs::Metadata) -> Option<String> {
    let time = metadata.modified().ok()?;
    let seconds = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).ok()?,
        Err(before) => -i64::try_from(before.duration().as_secs()).ok()?,
    };
    Some(rfc3339(seconds))
}

/// Files the operating system leaves in folders, which do not make a
/// folder hold anything.
pub(crate) const SYSTEM_FILES: [&str; 3] = [".DS_Store", "desktop.ini", "Thumbs.db"];

pub(crate) fn is_system_file(name: &str) -> bool {
    SYSTEM_FILES
        .iter()
        .any(|file| file.eq_ignore_ascii_case(name))
}

/// The entries of folder `dir` other than [`SYSTEM_FILES`]; none when it
/// is missing.
pub(crate) fn folder_entries(dir: &Path) -> Result<Vec<String>, VcsError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(crate::io_error("read", dir, e)),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| crate::io_error("read", dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_system_file(&name) {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// The fields of a `create_project` command.
fn create_options(command: &Value) -> Result<CreateOptions, VcsError> {
    let flag = |key: &str| command.get(key).and_then(Value::as_bool).unwrap_or(true);
    let design = match command.get("design").filter(|value| !value.is_null()) {
        Some(design) => {
            let path = design.get("path").and_then(Value::as_str);
            let text = design.get("text").and_then(Value::as_str);
            match (path, text) {
                (Some(path), Some(text)) => Some((path.to_owned(), text.to_owned())),
                _ => {
                    return Err(VcsError::Command(
                        "\"design\" is {\"path\": <relative path>, \"text\": <the design>}"
                            .to_owned(),
                    ));
                }
            }
        }
        None => None,
    };
    // Merged into the defaults and checked strictly, as set_project_settings.
    let shared = match command.get("shared").filter(|value| !value.is_null()) {
        Some(changes) => {
            let value = settings::merged(&SharedSettings::default().to_json(), changes);
            let (shared, _) = SharedSettings::from_json(&value, true).map_err(VcsError::Command)?;
            Some(shared)
        }
        None => None,
    };
    Ok(CreateOptions {
        author: Identity::parse(text(command, "author")?)?,
        design,
        include_designs: flag("include_designs"),
        url: optional(command, "url").map(str::to_owned),
        push: flag("push"),
        shared,
    })
}

/// Runs a command without a project (`core/model/src/api/commands.md`,
/// "Projects"): its name, its answer and the failure the answer reports.
/// Every answer has `error` (null, or `class`, `message` and `detail`) and
/// `log` (the programs run, credentials hidden); other failures (a command
/// that is not understood, a file that cannot be read) are errors.
pub fn run(
    json: &str,
    context: &Context,
    control: Option<&Control>,
) -> Result<(String, Value, Option<RemoteError>), VcsError> {
    let command: Value = serde_json::from_str(json)
        .map_err(|e| VcsError::Command(format!("not a JSON command: {e}")))?;
    let name = text(&command, "cmd")?.to_owned();
    let mut log = Vec::new();
    let (mut value, error) = match name.as_str() {
        "inspect_folder" => {
            let dir = text(&command, "dir")?;
            answer(inspect_folder(Path::new(dir)), |_| json!({}))?
        }
        "check_remote" => {
            let url = text(&command, "url")?;
            answer(check_remote(url, context, control, &mut log), |_| {
                json!({"url": redact(url), "reachable": false, "empty": null,
                       "default_branch": null, "head": null, "has_project": null,
                       "files": [], "latest": null, "versions": null})
            })?
        }
        "create_project" => {
            let dir = text(&command, "dir")?;
            let options = create_options(&command)?;
            match create_project(Path::new(dir), &options, context, control, &mut log) {
                Ok(created) => {
                    let error = created.error.clone();
                    let mut value = json!(created);
                    value["error"] = json!(error);
                    (value, error)
                }
                Err(error) => answer::<Value>(Err(error), |_| {
                    json!({"root": null, "branch": null, "commit": null, "files": [],
                           "remote": null, "adopted": false, "push": null, "ahead": null,
                           "behind": null})
                })?,
            }
        }
        "clone_project" => {
            let url = text(&command, "url")?;
            let dir = text(&command, "dir")?;
            let adopt = command
                .get("adopt")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let author = optional(&command, "author")
                .map(Identity::parse)
                .transpose()?;
            match clone_project(
                url,
                Path::new(dir),
                adopt,
                author.as_ref(),
                context,
                control,
                &mut log,
            ) {
                Ok(outcome) => {
                    let error = outcome.error.clone();
                    let mut value = json!(outcome);
                    value["error"] = json!(error);
                    (value, error)
                }
                Err(error) => answer::<Value>(Err(error), |_| {
                    json!({"root": null, "url": redact(url), "branch": null, "head": null,
                           "files": [], "adopted": false, "commit": null, "push": null})
                })?,
            }
        }
        "init_bare" => {
            let dir = text(&command, "dir")?;
            answer(
                init_bare(Path::new(dir)),
                |_| json!({"dir": dir, "created": false}),
            )?
        }
        "host_keys" => {
            let host = text(&command, "host")?;
            let port = port_of(&command)?;
            answer(host_keys(host, port, context, &mut log), |_| {
                json!({"host": host, "port": port, "keys": [], "service": null,
                       "published": null, "known": false, "changed": false})
            })?
        }
        "trust_host_key" => {
            let host = text(&command, "host")?;
            let port = port_of(&command)?;
            // Without a key: every key the server gives.
            let trusted = match (optional(&command, "type"), optional(&command, "key")) {
                (None, None) => hostkeys::trust_host_keys(host, port, context, &mut log),
                _ => {
                    let kind = text(&command, "type")?;
                    let key = text(&command, "key")?;
                    trust_host_key(host, port, kind, key, context, &mut log)
                }
            };
            answer(trusted, |_| json!({"path": null, "written": false}))?
        }
        "ssh_public_key" => {
            let key = ssh_public_key(context);
            let mut value = json!({"path": null, "text": null});
            if let Some(key) = key {
                value = json!(key);
            }
            value["error"] = Value::Null;
            (value, None)
        }
        "git_info" => {
            let git = context.git.as_ref();
            let info = crate::remote::git_info(git);
            let error = info.error.clone();
            let mut value = json!(info);
            value["credential_helper"] = credential_helper(context, &mut log);
            value["author"] = configured_author(context, &mut log);
            (value, error)
        }
        other => {
            return Err(VcsError::Command(format!(
                "unknown projects command '{other}'"
            )));
        }
    };
    value["log"] = json!(log);
    Ok((name, value, error))
}

/// A command's `port` (default 22).
fn port_of(command: &Value) -> Result<u16, VcsError> {
    match command.get("port").filter(|value| !value.is_null()) {
        None => Ok(22),
        Some(port) => port
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port > 0)
            .ok_or_else(|| VcsError::Command(format!("{port} is not a port (1 to 65535)"))),
    }
}

/// The values of `key` in git's system and user configuration (`git
/// config --get-all`, run outside any repository), in order; none without
/// git.
fn git_config(context: &Context, key: &str, log: &mut Vec<String>) -> Vec<String> {
    let Ok(git) = context.git() else {
        return Vec::new();
    };
    let dir = std::env::temp_dir();
    let args = ["config", "--get-all", key];
    let result = git.run(Some(&dir), &args, None);
    log.push(redact(&format!(
        "git {}: {}",
        args.join(" "),
        match &result {
            Ok(output) if output.success => "ok".to_owned(),
            // 1: not set.
            Ok(output) => format!("exit {}", output.code.unwrap_or(-1)),
            Err(error) => error.message.clone(),
        }
    )));
    match result {
        Ok(output) if output.success => output
            .stdout
            .lines()
            .map(|line| line.trim().to_owned())
            .collect(),
        _ => Vec::new(),
    }
}

/// git's `credential.helper`: {`configured`, `name`} (the helper that
/// applies: the last one after the last empty value, credentials hidden).
fn credential_helper(context: &Context, log: &mut Vec<String>) -> Value {
    let mut helper = None;
    for value in git_config(context, "credential.helper", log) {
        helper = (!value.is_empty()).then_some(value);
    }
    match helper {
        Some(name) => json!({"configured": true, "name": clean_text(&redact(&name), 200)}),
        None => json!({"configured": false, "name": null}),
    }
}

/// git's configured author (`user.name`, `user.email`), which New Project
/// shows first; null when git has none (or none that is an author).
fn configured_author(context: &Context, log: &mut Vec<String>) -> Value {
    let name = git_config(context, "user.name", log).pop();
    let email = git_config(context, "user.email", log).pop();
    match (name, email) {
        (Some(name), Some(email)) => Identity::new(&name, &email)
            .map(|identity| json!(identity))
            .unwrap_or(Value::Null),
        _ => Value::Null,
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

/// The summary of a remote's latest version (`check_remote`,
/// `remote_check`) as text: `14 versions, latest 2026-10-07 by Name` and
/// the names at its root.
pub(crate) fn describe_summary(out: &mut String, answer: &Value) {
    if let Some(count) = answer["versions"].as_u64() {
        let latest = &answer["latest"];
        let date = latest["date"].as_str().unwrap_or("");
        let _ = writeln!(
            out,
            "{}{}, latest {} by {}",
            versions(count),
            if count >= COUNT_LIMIT as u64 {
                " or more"
            } else {
                ""
            },
            date.get(..10).unwrap_or(date),
            latest["author"].as_str().unwrap_or("")
        );
    }
    let files: Vec<&str> = answer["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !files.is_empty() {
        let _ = writeln!(out, "Files: {}", files.join(", "));
    }
}

/// The answer of command `name` of [`run`] as text for people.
pub fn describe(name: &str, answer: &Value) -> String {
    let mut out = String::new();
    let field = |key: &str| answer[key].as_str().unwrap_or("");
    let short = |id: &Value| {
        let id = id.as_str().unwrap_or("");
        id[..7.min(id.len())].to_owned()
    };
    match name {
        "inspect_folder" => inspect::describe(&mut out, answer),
        "check_remote" => {
            let _ = writeln!(out, "Reachable: {}", field("url"));
            if answer["empty"].as_bool() == Some(true) {
                let _ = writeln!(out, "The repository is empty: a project can be made there");
            } else {
                let project = if answer["has_project"].as_bool() == Some(true) {
                    "a Mitcad project"
                } else {
                    "no Mitcad project"
                };
                let _ = writeln!(
                    out,
                    "Branch {} ({}): {project}",
                    field("default_branch"),
                    short(&answer["head"])
                );
                describe_summary(&mut out, answer);
            }
        }
        "create_project" | "clone_project" => {
            let what = if name == "create_project" {
                "Created the project"
            } else {
                "Opened"
            };
            let _ = writeln!(out, "{what} {}", field("root"));
            if answer["adopted"].as_bool() == Some(true) {
                let _ = writeln!(
                    out,
                    "The remote's files were taken in: the project is beside them"
                );
            }
            if !answer["commit"].is_null() {
                let _ = writeln!(out, "Recorded version {}", short(&answer["commit"]));
            }
            if let Some(url) = answer["remote"]["url"].as_str() {
                let _ = writeln!(
                    out,
                    "Remote {}: {url}",
                    answer["remote"]["name"].as_str().unwrap_or("")
                );
            }
            let files: Vec<&str> = answer["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if !files.is_empty() {
                let label = if name == "create_project" {
                    "Recorded"
                } else {
                    "Project files"
                };
                let _ = writeln!(out, "{label}: {}", files.join(", "));
            }
            if answer["push"]["pushed"].as_bool() == Some(true) {
                let _ = writeln!(out, "Pushed to the remote");
            }
            if let Some(message) = answer["error"]["message"].as_str() {
                let _ = writeln!(out, "warning: {message}");
            }
        }
        "init_bare" => {
            let _ = if answer["created"].as_bool() == Some(true) {
                writeln!(out, "Made {} a shared repository", field("dir"))
            } else {
                writeln!(out, "{} is a shared repository already", field("dir"))
            };
        }
        "host_keys" => hostkeys::describe(&mut out, answer),
        "trust_host_key" => {
            let _ = if answer["written"].as_bool() == Some(true) {
                writeln!(out, "Trusted: the key was added to {}", field("path"))
            } else {
                writeln!(out, "The key is in {} already", field("path"))
            };
        }
        "ssh_public_key" => {
            let _ = match answer["text"].as_str() {
                Some(text) => writeln!(out, "{text}"),
                None => writeln!(
                    out,
                    "No public key in ~/.ssh (id_ed25519.pub, id_ecdsa.pub or id_rsa.pub): \
                     make one with ssh-keygen -t ed25519"
                ),
            };
        }
        "git_info" => {
            out.push_str(&crate::remote::commands::describe_git(answer));
            let _ = match (
                answer["author"]["name"].as_str(),
                answer["author"]["email"].as_str(),
            ) {
                (Some(name), Some(email)) => writeln!(out, "Author: {name} <{email}>"),
                _ => writeln!(
                    out,
                    "No author in git's configuration (user.name, user.email)"
                ),
            };
            let helper = &answer["credential_helper"];
            let _ = match helper["name"].as_str() {
                Some(name) => writeln!(out, "Credential helper: {name}"),
                None => writeln!(
                    out,
                    "No credential helper: HTTPS remotes need one (Git Credential Manager), \
                     or use SSH"
                ),
            };
        }
        _ => {}
    }
    out
}

/// The answer of a project's command of Local and Cloud projects as text;
/// None for another command.
pub(crate) fn describe_project(name: &str, answer: &Value) -> Option<String> {
    let mut out = String::new();
    let settings = |out: &mut String, shared: &Value, local: &Value| {
        let locks = &shared["edit_locks"];
        let _ = if locks["enabled"].as_bool() == Some(true) {
            writeln!(
                out,
                "Edit locks: on (idle time {} min, poll {} s)",
                locks["idle_minutes"], locks["poll_seconds"]
            )
        } else {
            writeln!(out, "Edit locks: off")
        };
        let _ = match shared["live_updates"]["broker"].as_str() {
            Some(broker) => writeln!(
                out,
                "Live updates: {broker} (prefix {})",
                shared["live_updates"]["prefix"].as_str().unwrap_or("")
            ),
            None => writeln!(out, "Live updates: none"),
        };
        let _ = match local["live"]["mode"].as_str() {
            Some("off") => writeln!(out, "Live updates on this computer: off"),
            Some("broker") => writeln!(
                out,
                "Live updates on this computer: {}",
                local["live"]["broker"].as_str().unwrap_or("")
            ),
            _ => writeln!(out, "Live updates on this computer: the project's"),
        };
        let sync = &local["sync"];
        let send = match sync["send_at_once"].as_bool() {
            Some(true) => "yes",
            Some(false) => "no",
            None => "the application's default",
        };
        let check = match sync["check_minutes"].as_u64() {
            Some(0) => "never".to_owned(),
            Some(n) => format!("every {n} min"),
            None => "the application's default".to_owned(),
        };
        let _ = writeln!(
            out,
            "Send each version at once: {send}; check for newer versions: {check}"
        );
    };
    match name {
        "project_settings" => {
            let _ = writeln!(
                out,
                "{} project",
                if answer["kind"] == "cloud" {
                    "Cloud"
                } else {
                    "Local"
                }
            );
            settings(&mut out, &answer["shared"], &answer["local"]);
            let authors: Vec<String> = answer["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|author| {
                    format!(
                        "{} <{}> ({})",
                        author["name"].as_str().unwrap_or(""),
                        author["email"].as_str().unwrap_or(""),
                        versions(author["versions"].as_u64().unwrap_or(0))
                    )
                })
                .collect();
            if !authors.is_empty() {
                let _ = writeln!(out, "Versions by: {}", authors.join(", "));
            }
            for problem in answer["problems"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "warning: {}", problem.as_str().unwrap_or(""));
            }
        }
        "set_project_settings" => {
            settings(&mut out, &answer["shared"], &answer["local"]);
            if let Some(commit) = answer["commit"].as_str() {
                let _ = writeln!(
                    out,
                    "Recorded version {} (send it with mitcad-cli sync)",
                    &commit[..7.min(commit.len())]
                );
            }
            if let Some(reason) = answer["skipped"].as_str() {
                let _ = writeln!(out, "No version recorded: {reason}");
            }
        }
        "set_identity" => {
            let _ = match (answer["name"].as_str(), answer["email"].as_str()) {
                (Some(name), Some(email)) => {
                    writeln!(out, "Author of this project: {name} <{email}>")
                }
                _ => writeln!(out, "The project's own author was removed"),
            };
        }
        "remember_design" => {
            let _ = writeln!(
                out,
                "Opened last: {}",
                answer["path"].as_str().unwrap_or("")
            );
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_rfc3339_in_utc() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_791_460_496), "2026-10-08T11:54:56Z");
        assert_eq!(rfc3339(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn text_from_elsewhere_is_cleaned() {
        assert_eq!(clean_text("  Ada \n\t Lovelace ", 100), "Ada Lovelace");
        assert_eq!(
            clean_text("evil\u{202E}txt.exe\u{200B}\u{1b}[31m", 100),
            "eviltxt.exe[31m"
        );
        assert_eq!(clean_text("<img src=x>", 100), "<img src=x>");
        assert_eq!(clean_text("abcdef", 3), "abc");
        assert_eq!(clean_text("ab cd", 3), "ab");
        assert_eq!(clean_text("\u{feff}", 10), "");
    }
}
