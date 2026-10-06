// SPDX-License-Identifier: MIT
//! The version history as JSON commands, for the application and
//! `mitcad-cli` through the C++ bridge (`core/model/src/api/commands.md`,
//! "Version history" and "Remote repositories"). [`ProjectRepo::command`]
//! answers in JSON, [`ProjectRepo::command_text`] in text for people;
//! [`ProjectRepo::command_with`] runs a remote command with a [`Control`]
//! for its progress and cancellation.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::remote::{Control, RemoteError, commands as remote};
use crate::{CommitOutcome, Identity, ProjectRepo, VcsError};

/// Makes folder `dir` a project with version history
/// ([`ProjectRepo::init`]); the first version's author is `author` (`Name
/// <email>`), or git's configuration when it is empty. Returns the
/// project and `{"root", "branch", "commit", "written", ...}`.
pub fn init(dir: &Path, author: &str) -> Result<(ProjectRepo, String), VcsError> {
    let author = (!author.trim().is_empty())
        .then(|| Identity::parse(author))
        .transpose()?;
    let (repo, outcome) = ProjectRepo::init(dir, author.as_ref())?;
    let mut answer = serde_json::to_value(&outcome).expect("serializable");
    answer["root"] = json!(repo.root().to_string_lossy());
    answer["branch"] = json!(repo.branch()?);
    Ok((repo, answer.to_string()))
}

// Remote repositories (P12 remote) without a project.

/// The git program, its version and git-lfs's (the remote command
/// `git_info`), as JSON or as text for people.
pub fn git_info(text: bool) -> String {
    let info = json!(crate::remote::git_info(None));
    if text {
        remote::describe_git(&info)
    } else {
        info.to_string()
    }
}

/// The git program at `program` (Preferences, before it is chosen), its
/// version and git-lfs's, as `git_info` answers; an empty `program` is the
/// one remote work finds.
pub fn git_info_at(program: &str) -> String {
    let git = (!program.trim().is_empty()).then(|| crate::remote::GitCli::at(program.trim()));
    json!(crate::remote::git_info(git.as_ref())).to_string()
}

/// Opens a project from the remote at `url` into folder `dir`
/// ([`crate::remote::clone_project`]). The answer in JSON has `root`,
/// `url`, `branch`, `head`, `files`, `error` and `log`, as the remote
/// commands; in text a failure is an error.
pub fn clone_project(
    url: &str,
    dir: &Path,
    control: Option<&Control>,
    text: bool,
) -> Result<String, VcsError> {
    let mut log = Vec::new();
    let result = crate::remote::clone_project(url, dir, None, control, &mut log);
    let (mut answer, error) = remote::answer(result, |_| {
        json!({"root": null, "url": crate::remote::redact(url), "branch": null, "head": null,
               "files": []})
    })?;
    answer["log"] = json!(log);
    match error {
        Some(error) if text => Err(error.into()),
        _ if text => Ok(remote::describe_clone(&answer)),
        _ => Ok(answer.to_string()),
    }
}

/// The JSON answer of remote command `name` (as [`ProjectRepo::command`]
/// gives it) as text for people, a failure too: what a `sync` did before it
/// stopped and why (its conflicts with their versions), else the failure's
/// message.
pub fn describe_answer(name: &str, answer: &str) -> Result<String, VcsError> {
    let answer: Value =
        serde_json::from_str(answer).map_err(|e| invalid(format!("not a JSON answer: {e}")))?;
    Ok(remote::describe_answer(name, &answer))
}

fn invalid(message: String) -> VcsError {
    VcsError::Command(message)
}

/// A text field of a command.
pub(crate) fn text<'a>(command: &'a Value, key: &str) -> Result<&'a str, VcsError> {
    command
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("the command needs \"{key}\" (text)")))
}

/// An optional text field; empty counts as missing.
pub(crate) fn optional<'a>(command: &'a Value, key: &str) -> Option<&'a str> {
    command
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
}

impl ProjectRepo {
    /// Runs a JSON command and answers in JSON (see the module's
    /// documentation).
    pub fn command(&self, json: &str) -> Result<String, VcsError> {
        Ok(self.run(json, None)?.1.to_string())
    }

    /// Runs a JSON command whose progress `control` shows and whose cancel
    /// request it carries (remote commands; the others ignore it).
    pub fn command_with(&self, json: &str, control: &Control) -> Result<String, VcsError> {
        Ok(self.run(json, Some(control))?.1.to_string())
    }

    /// Runs a JSON command and answers in text for people (`mitcad-cli`); a
    /// remote command's failure is an error.
    pub fn command_text(&self, json: &str) -> Result<String, VcsError> {
        let (name, answer, error) = self.run(json, None)?;
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(remote::describe(&name, &answer).unwrap_or_else(|| describe(&name, &answer)))
    }

    /// The command's name, its answer and a remote command's failure.
    fn run(
        &self,
        json: &str,
        control: Option<&Control>,
    ) -> Result<(String, Value, Option<RemoteError>), VcsError> {
        let command: Value =
            serde_json::from_str(json).map_err(|e| invalid(format!("not a JSON command: {e}")))?;
        let name = text(&command, "cmd")?.to_owned();
        // Remote repositories (P12 remote).
        if let Some((answer, error)) = self.remote_command(&name, &command, control)? {
            return Ok((name, answer, error));
        }
        let path = || text(&command, "path").map(PathBuf::from);
        let answer = match name.as_str() {
            "info" => json!({
                "root": self.root().to_string_lossy(),
                "branch": self.branch()?,
                "head": self.head_commit()?.map(|id| id.to_string()),
            }),
            "identity" => json!(self.author(&command)?),
            "commit" => {
                let paths = command
                    .get("paths")
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("commit needs \"paths\" (a list)".to_owned()))?;
                let files = paths
                    .iter()
                    .map(|p| p.as_str().map(PathBuf::from))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| invalid("\"paths\" are text".to_owned()))?;
                let message = optional(&command, "message").unwrap_or("");
                let outcome = self.commit(&files, message, &self.author(&command)?)?;
                outcome_json(&outcome, &files_text(self, &files))
            }
            "history" => {
                let versions = self.history(&path()?)?;
                let mut list = serde_json::to_value(&versions).expect("serializable");
                // P12e: what each version changed against the one before.
                if command.get("summaries").and_then(Value::as_bool) == Some(true) {
                    for (entry, summary) in list
                        .as_array_mut()
                        .expect("a list")
                        .iter_mut()
                        .zip(self.summaries(&versions))
                    {
                        match summary {
                            Some(Ok(text)) => entry["changes"] = json!(text),
                            Some(Err(e)) => {
                                entry["changes"] = Value::Null;
                                entry["changes_error"] = json!(e.to_string());
                            }
                            None => entry["changes"] = Value::Null,
                        }
                    }
                }
                json!({"path": self.relative(&path()?)?, "versions": list})
            }
            "read_version" => {
                let rev = text(&command, "rev")?;
                let data = self.read(rev, &path()?)?;
                let text = String::from_utf8(data)
                    .map_err(|_| VcsError::File("the file is not text in that version".into()))?;
                json!({"id": self.resolve(rev)?, "path": self.relative(&path()?)?, "text": text})
            }
            "restore" => {
                let rev = text(&command, "rev")?;
                let file = path()?;
                let message = optional(&command, "message");
                let outcome = self.restore(&file, rev, message, &self.author(&command)?)?;
                outcome_json(&outcome, &self.relative(&file)?)
            }
            "changes" => {
                let from = text(&command, "from")?;
                let to = optional(&command, "to");
                json!({
                    "from": self.resolve(from)?,
                    "to": self.resolve(to.unwrap_or("HEAD"))?,
                    "changes": self.changes(from, to)?,
                })
            }
            "status" => json!(self.status(&path()?)?),
            // Saving versions in the application (P12d).
            "follow_rename" => {
                let (from, moved) = self.follow_rename(&path()?)?;
                json!({"renamed_from": from, "display_moved": moved})
            }
            "resolve" => json!({"id": self.resolve(text(&command, "rev")?)?}),
            // Comparison of versions (P12c).
            "diff" => {
                let from = text(&command, "from")?;
                let to = optional(&command, "to");
                let file = path()?;
                let diff = self.diff(&file, from, to)?;
                let mut answer = diff.to_json();
                answer["path"] = json!(self.relative(&file)?);
                answer["from"] = json!(self.resolve(from)?);
                answer["to"] = match to {
                    Some(rev) => json!(self.resolve(rev)?),
                    None => Value::Null,
                };
                answer["text"] = json!(diff.to_text());
                answer
            }
            other => {
                return Err(invalid(format!(
                    "unknown version history command '{other}'"
                )));
            }
        };
        Ok((name, answer, None))
    }

    /// The author a command names (`author`: `Name <email>`), else git's
    /// configuration, else the command's `fallback_author` (Mitcad's
    /// settings).
    pub(crate) fn author(&self, command: &Value) -> Result<Identity, VcsError> {
        if let Some(author) = optional(command, "author") {
            return Identity::parse(author);
        }
        let fallback = optional(command, "fallback_author")
            .map(Identity::parse)
            .transpose()?;
        self.identity(fallback.as_ref())
    }
}

/// The saved files of a commit, relative to the project, for messages.
fn files_text(repo: &ProjectRepo, files: &[PathBuf]) -> String {
    let names: Vec<String> = files
        .iter()
        .map(|f| repo.relative(f).unwrap_or_else(|_| f.display().to_string()))
        .collect();
    names.join(", ")
}

fn outcome_json(outcome: &CommitOutcome, files: &str) -> Value {
    let mut answer = serde_json::to_value(outcome).expect("serializable");
    answer["files"] = json!(files);
    answer
}

/// A command's answer as text.
fn describe(name: &str, answer: &Value) -> String {
    let field = |key: &str| answer.get(key).and_then(Value::as_str).unwrap_or("");
    let mut out = String::new();
    match name {
        "commit" | "restore" => {
            let list = |key: &str| {
                answer
                    .get(key)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            };
            if let Some(commit) = answer.get("commit").and_then(Value::as_str) {
                let _ = writeln!(
                    out,
                    "Recorded version {} of {}: {} file(s) written, {} removed",
                    &commit[..7.min(commit.len())],
                    field("files"),
                    list("written"),
                    list("removed")
                );
            } else if let Some(reason) = answer.get("skipped").and_then(Value::as_str) {
                let _ = writeln!(out, "No version recorded: {reason}");
            } else {
                let _ = writeln!(
                    out,
                    "No changes since the latest version of {}",
                    field("files")
                );
            }
            for warning in answer
                .get("warnings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let _ = writeln!(out, "warning: {}", warning.as_str().unwrap_or(""));
            }
        }
        "history" => {
            let versions = answer["versions"].as_array().cloned().unwrap_or_default();
            let path = field("path");
            let _ = writeln!(
                out,
                "Versions of {path} ({}), newest first:",
                versions.len()
            );
            for (i, version) in versions.iter().enumerate() {
                let get = |key: &str| version[key].as_str().unwrap_or("");
                let mut line = format!(
                    "  v{:<3} {}  {}  {}  {}",
                    versions.len() - i,
                    get("short_id"),
                    get("date"),
                    version["author"]["name"].as_str().unwrap_or(""),
                    get("summary")
                );
                if version["blob"].is_null() {
                    line.push_str(" (deleted)");
                }
                if let Some(from) = version["renamed_from"].as_str() {
                    let _ = write!(line, " (renamed from {from})");
                }
                let _ = writeln!(out, "{}", line.trim_end());
            }
        }
        "changes" => {
            let changes = answer["changes"].as_array().cloned().unwrap_or_default();
            if changes.is_empty() {
                let _ = writeln!(out, "No changes");
            }
            for change in changes {
                let path = change["path"].as_str().unwrap_or("");
                let _ = match change["kind"].as_str().unwrap_or("") {
                    "added" => writeln!(out, "A {path}"),
                    "deleted" => writeln!(out, "D {path}"),
                    "renamed" => {
                        writeln!(out, "R {} -> {path}", change["from"].as_str().unwrap_or(""))
                    }
                    _ => writeln!(out, "M {path}"),
                };
            }
        }
        "status" => {
            let renamed = answer["renamed_from"]
                .as_str()
                .map(|from| format!("renamed from {from} since the latest version"));
            let state = if let Some(renamed) = &renamed {
                renamed.as_str()
            } else if answer["head"].is_null() && !answer["file"].is_null() {
                "not in the latest version"
            } else if answer["file"].is_null() && !answer["head"].is_null() {
                "deleted since the latest version"
            } else if answer["modified"].as_bool() == Some(true) {
                "changed since the latest version"
            } else {
                "as in the latest version"
            };
            let _ = writeln!(out, "{}: {state}", field("path"));
        }
        "follow_rename" => {
            let _ = match answer["renamed_from"].as_str() {
                Some(from) if answer["display_moved"].as_bool() == Some(true) => {
                    writeln!(out, "Renamed from {from}: its display state moved along")
                }
                Some(from) => writeln!(out, "Renamed from {from}"),
                None => writeln!(out, "Not renamed"),
            };
        }
        "info" => {
            let _ = writeln!(
                out,
                "Project {} on branch {}, latest version {}",
                field("root"),
                answer["branch"].as_str().unwrap_or("(detached HEAD)"),
                answer["head"]
                    .as_str()
                    .map_or("(none)", |h| &h[..7.min(h.len())])
            );
        }
        "read_version" => out.push_str(field("text")),
        "diff" => {
            let short = |key: &str| {
                answer[key]
                    .as_str()
                    .map(|id| id[..7.min(id.len())].to_owned())
            };
            let _ = writeln!(
                out,
                "Changes in {} from {} to {}:",
                field("path"),
                short("from").unwrap_or_default(),
                short("to").unwrap_or_else(|| "the saved file".to_owned())
            );
            out.push_str(field("text"));
        }
        "identity" => {
            let _ = writeln!(out, "{} <{}>", field("name"), field("email"));
        }
        _ => {
            let _ = writeln!(
                out,
                "{}",
                answer.get("id").and_then(Value::as_str).unwrap_or("")
            );
        }
    }
    out
}
