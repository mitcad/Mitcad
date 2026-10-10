// SPDX-License-Identifier: MIT
//! The commands of a project for Local and Cloud projects (mitcad#89): its
//! settings (shared ones in the marker, recorded as a version when they
//! change; this computer's in `.mitcad/local/`), the repository's own
//! author (`user.name`, `user.email` in `.git/config`) and the design
//! opened last.

use std::collections::BTreeMap;
use std::path::Path;

use gix::bstr::ByteSlice;
use mitcad_model::file::PROJECT_MARKER;
use mitcad_model::file::settings::{
    LocalSettings, ProjectSettings, SharedSettings, write_local, write_shared,
};
use serde_json::{Map, Value, json};

use super::inspect::{remember, remotes_of};
use super::{MAX_NAME, clean_text};
use crate::api::text;
use crate::remote::{COUNT_LIMIT, RemoteError};
use crate::{Identity, ProjectRepo, VcsError, io_error};

/// The longest name or email address written as the repository's author.
const MAX_IDENTITY: usize = 200;

/// `value` with `changes` merged in: objects field by field (a null
/// replaces), anything else replaced.
pub(crate) fn merged(value: &Value, changes: &Value) -> Value {
    match (value, changes) {
        (Value::Object(old), Value::Object(new)) => {
            let mut out: Map<String, Value> = old.clone();
            for (key, change) in new {
                let next = match out.get(key) {
                    Some(old) if change.is_object() && old.is_object() => merged(old, change),
                    _ => change.clone(),
                };
                out.insert(key.clone(), next);
            }
            Value::Object(out)
        }
        _ => changes.clone(),
    }
}

/// What changed between two shared settings, for the version's message:
/// `edit locks off, idle time 15 min`.
fn what_changed(old: &SharedSettings, new: &SharedSettings) -> String {
    let mut parts = Vec::new();
    let (a, b) = (&old.edit_locks, &new.edit_locks);
    if a.enabled != b.enabled {
        parts.push(format!(
            "edit locks {}",
            if b.enabled { "on" } else { "off" }
        ));
    }
    if a.idle_minutes != b.idle_minutes {
        parts.push(format!("idle time {} min", b.idle_minutes));
    }
    if a.poll_seconds != b.poll_seconds {
        parts.push(format!("poll interval {} s", b.poll_seconds));
    }
    match (&old.live_updates, &new.live_updates) {
        (Some(_), None) => parts.push("live updates off".to_owned()),
        (None, Some(live)) => parts.push(format!("live updates through {}", live.broker)),
        (Some(a), Some(b)) => {
            if a.broker != b.broker {
                parts.push(format!("live updates through {}", b.broker));
            }
            if a.prefix != b.prefix {
                parts.push(format!("topic prefix {}", b.prefix));
            }
        }
        (None, None) => {}
    }
    parts.join(", ")
}

impl ProjectRepo {
    /// Runs a command of Local and Cloud projects: its answer, None when
    /// `name` is not one.
    pub(crate) fn project_command(
        &self,
        name: &str,
        command: &Value,
    ) -> Result<Option<(Value, Option<RemoteError>)>, VcsError> {
        let answer = match name {
            "project_settings" => self.project_settings()?,
            "set_project_settings" => self.set_project_settings(command)?,
            "set_identity" => {
                let field = |key: &str| command.get(key).and_then(Value::as_str).unwrap_or("");
                match self.set_identity(field("name"), field("email"))? {
                    Some(identity) => json!(identity),
                    None => json!({"name": null, "email": null}),
                }
            }
            "remember_design" => {
                let path = self.relative(Path::new(text(command, "path")?))?;
                remember(self.root(), &path)?;
                json!({ "path": path })
            }
            _ => return Ok(None),
        };
        Ok(Some((answer, None)))
    }

    /// Local or Cloud: whether the repository has a remote.
    fn kind(&self) -> Result<&'static str, VcsError> {
        let (_, remotes) = remotes_of(&self.fresh()?);
        Ok(if remotes.is_empty() { "local" } else { "cloud" })
    }

    /// `project_settings`: the kind, the shared settings, this computer's,
    /// the live updates this computer uses, what was not used, and who
    /// made the versions (what sharing the project publishes).
    fn project_settings(&self) -> Result<Value, VcsError> {
        let (settings, problems) = ProjectSettings::read(self.root());
        let live = settings
            .live_updates()
            .map(|live| json!({"broker": live.broker.to_string(), "prefix": live.prefix}));
        Ok(json!({
            "kind": self.kind()?,
            "shared": settings.shared.to_json(),
            "local": settings.local.to_json(),
            "live_updates": live,
            "problems": problems,
            "authors": self.authors()?,
        }))
    }

    /// The authors of the versions on HEAD's history (up to 10 000
    /// versions), by email: {`name` (the latest, cleaned), `email`,
    /// `versions`}, most versions first.
    fn authors(&self) -> Result<Vec<Value>, VcsError> {
        let mut found: BTreeMap<String, (String, String, usize)> = BTreeMap::new();
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        for info in self.repo.rev_walk([head]).all()?.take(COUNT_LIMIT) {
            let commit = self.repo.find_commit(info?.id)?;
            let author = commit.author()?;
            let email = clean_text(&author.email.to_str_lossy(), MAX_NAME);
            let entry = found.entry(email.to_lowercase()).or_insert_with(|| {
                // Newest first: the name the author has now.
                (clean_text(&author.name.to_str_lossy(), MAX_NAME), email, 0)
            });
            entry.2 += 1;
        }
        let mut authors: Vec<(String, String, usize)> = found.into_values().collect();
        authors.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        Ok(authors
            .into_iter()
            .map(|(name, email, versions)| json!({"name": name, "email": email, "versions": versions}))
            .collect())
    }

    /// `set_project_settings`: `shared` and `local` merged into the
    /// settings as they are and checked strictly (an error, nothing
    /// written); a change of the shared ones written into the marker and
    /// recorded as the version `Change project settings: <what changed>`
    /// (by `author`, else git's configured one, else `fallback_author`).
    fn set_project_settings(&self, command: &Value) -> Result<Value, VcsError> {
        let (current, _) = ProjectSettings::read(self.root());
        let invalid = |problem: String| VcsError::Command(problem);
        let shared = match command.get("shared").filter(|v| !v.is_null()) {
            Some(changes) => {
                let value = merged(&current.shared.to_json(), changes);
                SharedSettings::from_json(&value, true).map_err(invalid)?.0
            }
            None => current.shared.clone(),
        };
        let local = match command.get("local").filter(|v| !v.is_null()) {
            Some(changes) => {
                let mut value = current.local.to_json();
                // Live updates here are one choice, replaced as a whole.
                if let Some(live) = changes.get("live") {
                    value["live"] = live.clone();
                }
                if let Some(sync) = changes.get("sync") {
                    value["sync"] = merged(&value["sync"], sync);
                }
                LocalSettings::from_json(&value, true).map_err(invalid)?.0
            }
            None => current.local.clone(),
        };
        // The author is needed before anything is written.
        let changed = shared != current.shared;
        let author = if changed {
            Some(self.author(command)?)
        } else {
            None
        };
        let written_local = local != current.local;
        if written_local {
            write_local(self.root(), &local).map_err(|e| io_error("write", self.root(), e))?;
        }
        let mut commit = None;
        let mut skipped = None;
        if let Some(author) = author {
            let marker = self.root().join(PROJECT_MARKER);
            write_shared(self.root(), &shared).map_err(|e| io_error("write", &marker, e))?;
            let message = format!(
                "Change project settings: {}",
                what_changed(&current.shared, &shared)
            );
            let outcome = self.commit(&[marker], &message, &author)?;
            commit = outcome.commit;
            skipped = outcome.skipped;
        }
        Ok(json!({
            "shared": shared.to_json(),
            "local": local.to_json(),
            "written_local": written_local,
            "commit": commit,
            "skipped": skipped,
        }))
    }

    /// Writes the repository's own author (`user.name`, `user.email` in
    /// `.git/config`), which [`ProjectRepo::identity`] then gives first;
    /// both empty removes them. Returns the author written.
    pub fn set_identity(&self, name: &str, email: &str) -> Result<Option<Identity>, VcsError> {
        let (name, email) = (name.trim(), email.trim());
        let identity = if name.is_empty() && email.is_empty() {
            None
        } else {
            let identity = Identity::new(name, email)?;
            if identity.name.chars().count() > MAX_IDENTITY
                || identity.email.chars().count() > MAX_IDENTITY
                || format!("{name}{email}").chars().any(char::is_control)
            {
                return Err(VcsError::InvalidIdentity(format!("{name} <{email}>")));
            }
            Some(identity)
        };
        let path = self.repo.git_dir().join("config");
        let git_error = |e: &dyn std::fmt::Display| {
            VcsError::Git(format!("cannot change {}: {e}", path.display()))
        };
        let mut config =
            gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
                .map_err(|e| git_error(&e))?;
        match &identity {
            Some(identity) => {
                config
                    .set_raw_value_by("user", None, "name", identity.name.as_str())
                    .map_err(|e| git_error(&e))?;
                config
                    .set_raw_value_by("user", None, "email", identity.email.as_str())
                    .map_err(|e| git_error(&e))?;
            }
            None => {
                if let Ok(mut section) = config.section_mut("user", None) {
                    section.remove("name");
                    section.remove("email");
                }
            }
        }
        let mut text = Vec::new();
        config
            .write_to(&mut text)
            .map_err(|e| io_error("write", &path, e))?;
        mitcad_model::file::write_atomically(&path, &text)
            .map_err(|e| io_error("write", &path, e))?;
        Ok(identity)
    }

    /// The repository's own author, from `.git/config` as it is now.
    pub(crate) fn own_identity(&self) -> Option<Identity> {
        let path = self.repo.git_dir().join("config");
        let config =
            gix::config::File::from_path_no_includes(path, gix::config::Source::Local).ok()?;
        let name = config.string("user.name")?;
        let email = config.string("user.email")?;
        Identity::new(&name.to_string(), &email.to_string()).ok()
    }
}
