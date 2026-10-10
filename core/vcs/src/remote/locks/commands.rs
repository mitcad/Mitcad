// SPDX-License-Identifier: MIT
//! The edit locks' JSON commands (`core/model/src/api/commands.md`, "Edit
//! locks"), for the application's lock controller and `mitcad-cli lock`.
//! Like the remote commands every answer has `error` and `log`: a failure
//! of git or the network is an answer, not a failed command.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::{
    Answer, AnswerKind, Asked, Contents, LockRefName, LockRemote, LockState, MAX_KEEP_MINUTES,
    MAX_MESSAGE, Me, Person, Previous, REQUEST_FILE, Refresh, Request, TakenBecause, Update,
    Written, clean_limited, command_line_session, follow_requests, is_commit_id, lock_id,
    lock_json, my_request_json, now_utc, parse_time, previous_json, refused, request_json,
    session_of, staleness,
};
use crate::api::{optional, text};
use crate::remote::commands::answer;
use crate::remote::{Control, RemoteError};
use crate::{MITCAD_VERSION, ProjectRepo, VcsError};
use mitcad_model::file::settings::{IDLE_MINUTES, POLL_SECONDS};

fn invalid(message: impl Into<String>) -> VcsError {
    VcsError::Command(message.into())
}

/// A flag of a command.
fn flag(command: &Value, key: &str, default: bool) -> bool {
    command.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// A whole number of a command within `bounds`; None when it is missing.
fn number(command: &Value, key: &str, bounds: (u32, u32)) -> Result<Option<u32>, VcsError> {
    match command.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let n = value
                .as_u64()
                .filter(|n| (u64::from(bounds.0)..=u64::from(bounds.1)).contains(n))
                .ok_or_else(|| {
                    invalid(format!(
                        "\"{key}\" is a whole number from {} to {}",
                        bounds.0, bounds.1
                    ))
                })?;
            Ok(Some(u32::try_from(n).expect("within bounds")))
        }
    }
}

/// A commit id of a command (`base`), or None.
fn commit_field(command: &Value, key: &str) -> Result<Option<String>, VcsError> {
    match optional(command, key) {
        None => Ok(None),
        Some(id) if is_commit_id(id.trim()) => Ok(Some(id.trim().to_owned())),
        Some(id) => Err(invalid(format!("\"{key}\" is not a commit id: {id}"))),
    }
}

/// A message of a command: cleaned, at most [`MAX_MESSAGE`] characters.
fn message_field(command: &Value) -> Result<Option<String>, VcsError> {
    let Some(text) = optional(command, "message") else {
        return Ok(None);
    };
    if text.chars().count() > MAX_MESSAGE {
        return Err(invalid(format!(
            "the message is longer than {MAX_MESSAGE} characters"
        )));
    }
    let text = clean_limited(text, MAX_MESSAGE);
    Ok((!text.is_empty()).then_some(text))
}

/// The text of a time for people: `2026-10-08 13:40 UTC`.
fn shown_time(value: &Value) -> String {
    let text = value.as_str().unwrap_or("");
    match text.get(..16) {
        Some(minutes) => format!("{} UTC", minutes.replace('T', " ")),
        None => text.to_owned(),
    }
}

impl ProjectRepo {
    /// The session of a command (`session`, a UUID; without it the command
    /// line's, made from the author's email) and its person (the author,
    /// as for versions: `author`, git's configuration, `fallback_author`).
    /// `person`: the command needs one.
    fn me(&self, command: &Value, person: bool) -> Result<Me, VcsError> {
        let author = match self.author(command) {
            Ok(author) => Some(Person::new(&author.name, &author.email)),
            Err(VcsError::NoIdentity) if !person => None,
            Err(error) => return Err(error),
        };
        let session = match optional(command, "session") {
            Some(text) => session_of(text)
                .ok_or_else(|| invalid(format!("\"session\" is not a UUID: {text}")))?,
            None => match &author {
                Some(author) => command_line_session(&author.email),
                None => String::new(),
            },
        };
        Ok(Me {
            session,
            person: author,
        })
    }

    /// The file of a command's `path`, relative to the project, and its
    /// lock's id.
    fn lock_file(&self, command: &Value) -> Result<(String, String), VcsError> {
        let path = self.lock_path(&PathBuf::from(text(command, "path")?))?;
        let id = lock_id(&path);
        Ok((path, id))
    }

    /// The version a lock names as the one its holder edits: `base`, else
    /// HEAD.
    fn lock_base(&self, command: &Value) -> Result<Option<String>, VcsError> {
        match commit_field(command, "base")? {
            Some(base) => Ok(Some(base)),
            None => Ok(self.head_commit()?.map(|id| id.to_string())),
        }
    }

    /// Runs the edit-lock command `name`: its answer and the failure it
    /// reports. None when `name` is not one.
    pub(crate) fn lock_command(
        &self,
        name: &str,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<Option<(Value, Option<RemoteError>)>, VcsError> {
        let result = match name {
            "lock_poll" => self.lock_poll(command, control),
            "lock_status" => self.lock_status(command, control),
            "lock_take" => self.lock_take(command, control),
            "lock_refresh" => self.lock_refresh(command, control),
            "lock_release" => self.lock_release(command, control),
            "lock_hand_over" => self.lock_hand_over(command, control),
            "lock_request" => self.lock_request(command, control),
            "lock_withdraw" => self.lock_withdraw(command, control),
            "lock_answer" => self.lock_answer(command, control),
            "lock_probe" => self.lock_probe(command, control),
            _ => return Ok(None),
        };
        let (mut value, error) = answer(result, |_| json!({"outcome": null}))?;
        value["log"] = json!(self.take_remote_log());
        Ok(Some((value, error)))
    }

    /// What this process knows of the project's locks, for `me`: `locks`,
    /// `requests_without_lock`, `my_requests`, `dropped`, `head`, `newer`.
    /// `only`: one lock's id.
    fn lock_view(
        &self,
        me: &Me,
        remote: Option<&LockRemote>,
        mqtt: bool,
        only: Option<&str>,
    ) -> Result<Value, VcsError> {
        let project = self.lock_settings();
        let tracking = match remote {
            Some(remote) => super::reference_id(&self.fresh()?, &remote.tracking)?,
            None => None,
        };
        Ok(self.tracked(|t| {
            let now = t.now();
            let wanted = |id: &str| only.is_none_or(|only| only == id);
            let locks: Vec<Value> = t
                .locks
                .iter()
                .filter(|(id, _)| wanted(id))
                .map(|(id, seen)| lock_json(t, id, seen, me, &project, mqtt))
                .collect();
            let mut without = Vec::new();
            let mut position = BTreeMapCounter::default();
            for ((id, session), seen) in &t.requests {
                if t.locks.contains_key(id) || !wanted(id) {
                    continue;
                }
                let order = position.next(id);
                without.push(request_json(
                    id, session, seen, None, me, order, &project, now,
                ));
            }
            let mine: Vec<Value> = t
                .requests
                .keys()
                .filter(|(id, session)| *session == me.session && wanted(id))
                .map(|(id, _)| my_request_json(t, id, me, &project, mqtt))
                .collect();
            json!({
                "remote": remote.map(|r| r.name.clone()),
                "session": me.session,
                "polled": t.polled,
                "head": t.head.map(|id| id.to_string()),
                "tracking": tracking.map(|id| id.to_string()),
                "newer": t.head.is_some() && t.head != tracking,
                "locks": locks,
                "requests_without_lock": without,
                "my_requests": mine,
                "dropped": t.malformed.len(),
                "settings": {
                    "enabled": project.enabled,
                    "idle_minutes": project.idle_minutes,
                    "poll_seconds": project.poll_seconds,
                },
            })
        }))
    }

    /// `lock_poll`: one listing, the receipts of the requests to this
    /// session's locks, and what changed.
    fn lock_poll(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        if me.session.is_empty() {
            return Err(invalid("lock_poll needs \"session\""));
        }
        let mqtt = flag(command, "mqtt", false);
        let poll_seconds = number(command, "poll_seconds", POLL_SECONDS)?;
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        // This session's waiting requests kept alive, stale ones removed.
        let (refreshed, removed) = if flag(command, "tidy", true) {
            self.tidy_requests(&remote, &me, poll_seconds, control)?
        } else {
            (Vec::new(), Vec::new())
        };
        let mut lost = self.lost_locks(&me);
        let mut receipts = Vec::new();
        if flag(command, "receipts", true) {
            let project = self.lock_settings();
            let due: Vec<String> = self.tracked(|t| {
                t.locks
                    .iter()
                    .filter_map(|(id, seen)| {
                        let lock = seen.contents.get()?;
                        if lock.session != me.session {
                            return None;
                        }
                        let unseen = t
                            .receipts_for(id, &me.session, &project)
                            .iter()
                            .any(|request| !lock.requests_seen.contains(request));
                        unseen.then(|| id.clone())
                    })
                    .collect()
            });
            for id in due {
                if let Some(lock) =
                    self.refresh_lock(&remote, &me, &id, &Refresh::default(), control)?
                {
                    receipts.push(lock.path);
                }
            }
            lost.extend(self.lost_locks(&me));
        }
        let mut value = self.lock_view(&me, Some(&remote), mqtt, None)?;
        let project = self.lock_settings();
        value["lost"] = self.tracked(|t| {
            lost.iter()
                .map(|(id, path, now)| {
                    json!({
                        "id": id,
                        "path": path,
                        "lock": now.as_ref().map(|seen| lock_json(t, id, seen, &me, &project, mqtt)),
                    })
                })
                .collect::<Vec<_>>()
                .into()
        });
        value["receipts"] = json!(receipts);
        value["refreshed_requests"] = json!(refreshed);
        value["removed_requests"] = json!(removed);
        Ok(value)
    }

    /// `lock_status`: the locks (of one file with `path`), from a new
    /// listing unless `network` is false; nothing is written.
    fn lock_status(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        let file = optional(command, "path")
            .map(|_| self.lock_file(command))
            .transpose()?;
        let remote = if flag(command, "network", true) {
            let remote = self.lock_remote()?;
            self.look(&remote, control)?;
            Some(remote)
        } else {
            // What the last poll saw; the remote only names the tracking ref.
            self.lock_remote().ok()
        };
        let only = file.as_ref().map(|(_, id)| id.as_str());
        let mut value = self.lock_view(&me, remote.as_ref(), flag(command, "mqtt", false), only)?;
        if let Some((path, id)) = &file {
            value["path"] = json!(path);
            value["file_id"] = json!(id);
            value["lock"] = value["locks"]
                .as_array()
                .and_then(|locks| locks.first().cloned())
                .unwrap_or(Value::Null);
        }
        Ok(value)
    }

    /// `lock_take`: the file's lock taken when it is free, stale, this
    /// session's already (refreshed with the given values, its request
    /// withdrawn), or `take_over` names it as it is; otherwise `held`.
    fn lock_take(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, true)?;
        let person = me.person.clone().expect("a person");
        let (path, id) = self.lock_file(command)?;
        let settings = self.lock_settings();
        let idle_minutes = number(command, "idle_minutes", IDLE_MINUTES)?;
        let poll_seconds = number(command, "poll_seconds", POLL_SECONDS)?;
        let mqtt = flag(command, "mqtt", false);
        let take_over = commit_field(command, "take_over")?;
        let base = self.lock_base(command)?;
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        let project = self.lock_settings();
        for _ in 0..super::WRITE_ATTEMPTS {
            let now = now_utc();
            let current = self.seen_lock(&id);
            let mut taken_from = None;
            // The lock as it was, for the answer when it is taken from
            // someone ("Alex's edit lock had expired (last active 14:02)").
            let previous = current.as_ref().map_or(Value::Null, |seen| {
                self.tracked(|t| lock_json(t, &id, seen, &me, &project, mqtt))
            });
            let (expected, outcome, old) = match &current {
                None => (None, "taken", None),
                Some(seen) => match &seen.contents {
                    Contents::Read(lock) if lock.session == me.session => {
                        (Some(seen.oid), "refreshed", Some(lock.clone()))
                    }
                    contents => {
                        let because = if take_over.as_deref() == Some(&seen.oid.to_string()) {
                            Some(TakenBecause::TakeOver)
                        } else {
                            self.tracked(|t| {
                                staleness(t, &id, seen, &me, &project, mqtt, t.now()).0
                            })
                        };
                        let Some(because) = because else {
                            return self.take_answer("held", &path, &id, &me, mqtt, None);
                        };
                        if let Contents::Read(lock) = contents {
                            taken_from = Some(Previous {
                                person: lock.owner.clone(),
                                session: lock.session.clone(),
                                because: Some(because),
                            });
                        }
                        (Some(seen.oid), "taken", None)
                    }
                },
            };
            let requests = self.tracked(|t| t.receipts_for(&id, &me.session, &project));
            let mut lock = super::Lock {
                path: path.clone(),
                owner: person.clone(),
                session: me.session.clone(),
                application_version: MITCAD_VERSION.to_owned(),
                taken_at: now,
                refreshed_at: now,
                active_at: now,
                idle_minutes: idle_minutes.unwrap_or(settings.idle_minutes),
                poll_seconds: poll_seconds.unwrap_or(settings.poll_seconds),
                mqtt,
                base: base.clone(),
                state: LockState::Active,
                idle_since: None,
                requests_seen: requests,
                answers: Default::default(),
                handed_over_from: None,
                taken_from: taken_from.clone(),
            };
            if let Some(old) = &old {
                lock.taken_at = old.taken_at;
                lock.handed_over_from = old.handed_over_from.clone();
                lock.taken_from = old.taken_from.clone();
                lock.answers = old.answers.clone();
                lock.answers
                    .retain(|request, _| lock.requests_seen.contains(request));
            }
            match self.write_lock(&remote, &id, &lock, &person, expected, true, control)? {
                Written::Done => {
                    // A request of this session for the file is answered.
                    let _ = self.withdraw_request(&remote, &me, &id, control);
                    let mut value = self.take_answer(outcome, &path, &id, &me, mqtt, taken_from)?;
                    if outcome == "taken" {
                        value["previous"] = previous;
                    }
                    return Ok(value);
                }
                _ => {
                    self.look(&remote, control)?;
                    if old.is_none() {
                        return self.take_answer("changed", &path, &id, &me, mqtt, None);
                    }
                }
            }
        }
        self.take_answer("changed", &path, &id, &me, mqtt, None)
    }

    /// The answer of a take: `outcome`, `path`, `file_id`, `lock` (as it is
    /// now), `taken_from`.
    fn take_answer(
        &self,
        outcome: &str,
        path: &str,
        id: &str,
        me: &Me,
        mqtt: bool,
        taken_from: Option<Previous>,
    ) -> Result<Value, VcsError> {
        let project = self.lock_settings();
        let lock = self.tracked(|t| {
            t.locks
                .get(id)
                .map(|seen| lock_json(t, id, seen, me, &project, mqtt))
                .unwrap_or(Value::Null)
        });
        Ok(json!({
            "outcome": outcome,
            "path": path,
            "file_id": id,
            "session": me.session,
            "lock": lock,
            "taken_from": previous_json(&taken_from),
        }))
    }

    /// `lock_refresh`: this session's lock written again with the receipts
    /// of the requests seen and the given changes; `lost` when it is not
    /// this session's any more.
    fn lock_refresh(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        if me.session.is_empty() {
            return Err(invalid("lock_refresh needs \"session\""));
        }
        let (path, id) = self.lock_file(command)?;
        let state = match optional(command, "state") {
            None => None,
            Some("active") => Some(LockState::Active),
            Some("idle") => Some(LockState::Idle),
            Some(other) => {
                return Err(invalid(format!(
                    "\"state\" is active or idle, not '{other}'"
                )));
            }
        };
        let active_at = match optional(command, "active_at") {
            None => None,
            Some("now") => Some(now_utc()),
            Some(text) => Some(parse_time(text).ok_or_else(|| {
                invalid(format!("\"active_at\" is not a time (RFC 3339): {text}"))
            })?),
        };
        let refresh = Refresh {
            active_at,
            state,
            idle_minutes: number(command, "idle_minutes", IDLE_MINUTES)?,
            poll_seconds: number(command, "poll_seconds", POLL_SECONDS)?,
            mqtt: command.get("mqtt").and_then(Value::as_bool),
            base: commit_field(command, "base")?,
            answer: None,
        };
        let remote = self.lock_remote()?;
        let written = self.refresh_lock(&remote, &me, &id, &refresh, control)?;
        let outcome = if written.is_some() {
            "refreshed"
        } else {
            "lost"
        };
        self.take_answer(outcome, &path, &id, &me, flag(command, "mqtt", false), None)
    }

    /// Withdraws this session's request for lock `id`, if it has one.
    fn withdraw_request(
        &self,
        remote: &LockRemote,
        me: &Me,
        id: &str,
        control: Option<&Control>,
    ) -> Result<bool, VcsError> {
        let key = (id.to_owned(), me.session.clone());
        let Some(expected) = self.tracked(|t| t.requests.get(&key).map(|seen| seen.oid)) else {
            return Ok(false);
        };
        let name = LockRefName::Request(id.to_owned(), me.session.clone());
        let update = Update {
            remote_ref: name.remote(),
            expected: Some(expected),
            new: None,
        };
        match self
            .push_updates(&remote.name, &[update], control)?
            .remove(0)
        {
            Written::Done => {
                self.written(&name, None)?;
                Ok(true)
            }
            Written::Changed(None) => {
                self.written(&name, None)?;
                Ok(false)
            }
            Written::Changed(Some(_)) => Ok(false),
            Written::Refused(reason) => Err(refused(&reason)),
        }
    }

    /// `lock_release`: the file's lock removed when it is this session's
    /// (any with `force`, which also removes the file's requests); without
    /// `path`, `all` of this session's locks and requests (with `force`,
    /// every lock and request ref of the project).
    fn lock_release(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        let force = flag(command, "force", false);
        let file = optional(command, "path")
            .map(|_| self.lock_file(command))
            .transpose()?;
        if file.is_none() && !flag(command, "all", false) {
            return Err(invalid("lock_release needs \"path\", or \"all\": true"));
        }
        if !force && me.session.is_empty() {
            return Err(invalid("lock_release needs \"session\""));
        }
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        let only = file.as_ref().map(|(_, id)| id.clone());
        // The refs to remove, with the lock's path for the answer.
        let (removals, held_by_other) = self.tracked(|t| {
            let mut removals: Vec<(LockRefName, Option<String>, gix::ObjectId)> = Vec::new();
            let mut other = None;
            for (id, seen) in &t.locks {
                if only.as_ref().is_some_and(|only| only != id) {
                    continue;
                }
                let lock = seen.contents.get();
                let mine = lock.is_some_and(|lock| lock.session == me.session);
                if mine || force {
                    removals.push((
                        LockRefName::Lock(id.clone()),
                        lock.map(|lock| lock.path.clone()),
                        seen.oid,
                    ));
                } else if only.is_some() {
                    other = Some(id.clone());
                }
            }
            for ((id, session), seen) in &t.requests {
                if only.as_ref().is_some_and(|only| only != id) {
                    continue;
                }
                if force || (only.is_none() && *session == me.session) {
                    removals.push((
                        LockRefName::Request(id.clone(), session.clone()),
                        seen.contents.get().map(|r| r.path.clone()),
                        seen.oid,
                    ));
                }
            }
            (removals, other)
        });
        let updates: Vec<Update> = removals
            .iter()
            .map(|(name, _, oid)| Update {
                remote_ref: name.remote(),
                expected: Some(*oid),
                new: None,
            })
            .collect();
        let results = self.push_updates(&remote.name, &updates, control)?;
        let (mut released, mut withdrawn, mut changed) = (Vec::new(), Vec::new(), Vec::new());
        for ((name, path, _), written) in removals.iter().zip(results) {
            let label = json!(path);
            match written {
                Written::Done | Written::Changed(None) => {
                    self.written(name, None)?;
                    if let LockRefName::Lock(id) = name {
                        // Released, not lost (another session's removed
                        // with force is lost to it).
                        self.tracked(|t| t.held.remove(&(me.session.clone(), id.clone())));
                        released.push(label);
                    } else {
                        withdrawn.push(label);
                    }
                }
                Written::Changed(Some(_)) => changed.push(label),
                Written::Refused(reason) => return Err(refused(&reason)),
            }
        }
        let mut value = json!({
            "released": released,
            "withdrawn": withdrawn,
            "changed": changed,
            "force": force,
        });
        if let Some((path, id)) = &file {
            let lock_released = removals
                .iter()
                .any(|(name, _, _)| matches!(name, LockRefName::Lock(_)));
            let outcome = if !changed.is_empty() && lock_released && released.is_empty() {
                "changed"
            } else if lock_released {
                "released"
            } else if held_by_other.is_some() {
                "not_held"
            } else {
                "free"
            };
            let project = self.lock_settings();
            value["outcome"] = json!(outcome);
            value["path"] = json!(path);
            value["file_id"] = json!(id);
            value["lock"] = self.tracked(|t| {
                t.locks
                    .get(id)
                    .map(|seen| lock_json(t, id, seen, &me, &project, false))
                    .unwrap_or(Value::Null)
            });
        } else {
            value["outcome"] = json!("released");
        }
        Ok(value)
    }

    /// `lock_hand_over`: this session's lock written with a requester as
    /// its owner (the first request served, or the session `to`), in one
    /// compare-and-swap, so nobody else can take it in between.
    fn lock_hand_over(
        &self,
        command: &Value,
        control: Option<&Control>,
    ) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        let (path, id) = self.lock_file(command)?;
        let to = match optional(command, "to") {
            Some(text) => Some(
                session_of(text).ok_or_else(|| invalid(format!("\"to\" is not a UUID: {text}")))?,
            ),
            None => None,
        };
        let base = self.lock_base(command)?;
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        let current = self.seen_lock(&id);
        let mine = current.as_ref().and_then(|seen| match &seen.contents {
            Contents::Read(lock) if lock.session == me.session => Some((seen.oid, lock.clone())),
            _ => None,
        });
        let Some((expected, old)) = mine else {
            let outcome = if current.is_some() { "lost" } else { "free" };
            return self.take_answer(outcome, &path, &id, &me, false, None);
        };
        // The first request served: of another session, not stale, and by
        // default not one declined already.
        let project = self.lock_settings();
        let request: Option<Request> = self.tracked(|t| {
            let now = t.now();
            t.requests_of(&id)
                .into_iter()
                .filter(|(session, _)| **session != me.session)
                .filter(|(session, _)| to.as_ref().is_none_or(|to| to == *session))
                .filter(|(session, seen)| !seen.stale_for(session, &me, &project, now))
                .filter(|(_, seen)| {
                    to.is_some()
                        || old.answers.get(&seen.id.to_string()).map(|a| a.answer)
                            != Some(AnswerKind::Declined)
                })
                .find_map(|(_, seen)| seen.contents.get().cloned())
        });
        let Some(request) = request else {
            return self.take_answer("no_request", &path, &id, &me, false, None);
        };
        let now = now_utc();
        let lock = super::Lock {
            path: path.clone(),
            owner: request.requester.clone(),
            session: request.session.clone(),
            application_version: MITCAD_VERSION.to_owned(),
            taken_at: now,
            refreshed_at: now,
            active_at: now,
            idle_minutes: old.idle_minutes,
            poll_seconds: old.poll_seconds,
            mqtt: false,
            base,
            state: LockState::Active,
            idle_since: None,
            requests_seen: Vec::new(),
            answers: Default::default(),
            handed_over_from: Some(Previous {
                person: old.owner.clone(),
                session: me.session.clone(),
                because: None,
            }),
            taken_from: None,
        };
        let committer = me.person.clone().unwrap_or_else(|| old.owner.clone());
        let outcome = match self.write_lock(
            &remote,
            &id,
            &lock,
            &committer,
            Some(expected),
            false,
            control,
        )? {
            Written::Done => {
                // Handed over, not lost.
                self.tracked(|t| t.held.remove(&(me.session.clone(), id.clone())));
                "handed_over"
            }
            _ => {
                self.look(&remote, control)?;
                "changed"
            }
        };
        let mut value = self.take_answer(outcome, &path, &id, &me, false, None)?;
        value["to"] = json!({
            "name": request.requester.name,
            "email": request.requester.email,
            "session": request.session,
        });
        Ok(value)
    }

    /// `lock_request`: this session's request for a lock someone else
    /// holds, written (again: a new request) to its own ref; the receipt is
    /// then due within `receipt_due_seconds`.
    fn lock_request(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, true)?;
        let person = me.person.clone().expect("a person");
        let (path, id) = self.lock_file(command)?;
        let message = message_field(command)?;
        let mqtt = flag(command, "mqtt", false);
        let poll_seconds = number(command, "poll_seconds", POLL_SECONDS)?
            .unwrap_or(self.lock_settings().poll_seconds);
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        let key = (id.clone(), me.session.clone());
        let (lock, expected) = self.tracked(|t| {
            (
                t.locks.get(&id).cloned(),
                t.requests.get(&key).map(|seen| seen.oid),
            )
        });
        match &lock {
            None => return self.take_answer("free", &path, &id, &me, mqtt, None),
            Some(seen) if seen.holder.as_deref() == Some(me.session.as_str()) => {
                return self.take_answer("mine", &path, &id, &me, mqtt, None);
            }
            Some(_) => {}
        }
        let now = now_utc();
        let request = Request {
            path: path.clone(),
            requester: person.clone(),
            session: me.session.clone(),
            asked_at: now,
            message,
            refresh_of: None,
            refreshed_at: None,
            poll_seconds: Some(poll_seconds),
        };
        let commit = self.write_ref_commit(
            REQUEST_FILE,
            &request.to_json(),
            &person,
            &person,
            "Mitcad edit lock request",
            now,
        )?;
        let name = LockRefName::Request(id.clone(), me.session.clone());
        let update = Update {
            remote_ref: name.remote(),
            expected,
            new: Some(commit),
        };
        let outcome = match self
            .push_updates(&remote.name, &[update], control)?
            .remove(0)
        {
            Written::Done => {
                self.written(&name, Some(commit))?;
                self.tracked(|t| {
                    let at = t.now();
                    t.asked.insert(
                        key.clone(),
                        Asked {
                            request: commit,
                            at,
                            holder: None,
                            receipt_at: None,
                            answer_at: None,
                            answer: None,
                            until: None,
                            keep: Default::default(),
                        },
                    );
                    follow_requests(t, at);
                });
                "requested"
            }
            Written::Changed(_) => {
                self.look(&remote, control)?;
                "changed"
            }
            Written::Refused(reason) => return Err(refused(&reason)),
        };
        let mut value = self.take_answer(outcome, &path, &id, &me, mqtt, None)?;
        let project = self.lock_settings();
        value["request"] = json!(commit.to_string());
        value["my_request"] = self.tracked(|t| my_request_json(t, &id, &me, &project, mqtt));
        value["receipt_due_seconds"] = value["my_request"]["stale_in_seconds"].clone();
        Ok(value)
    }

    /// `lock_withdraw`: this session's request for the file removed.
    fn lock_withdraw(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        if me.session.is_empty() {
            return Err(invalid("lock_withdraw needs \"session\""));
        }
        let (path, id) = self.lock_file(command)?;
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        let outcome = if self.withdraw_request(&remote, &me, &id, control)? {
            "withdrawn"
        } else {
            "none"
        };
        self.take_answer(outcome, &path, &id, &me, false, None)
    }

    /// `lock_answer`: the holder's answer to a request (`declined`, or
    /// `keep` for `minutes`), written into this session's lock.
    fn lock_answer(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        if me.session.is_empty() {
            return Err(invalid("lock_answer needs \"session\""));
        }
        let (path, id) = self.lock_file(command)?;
        let request = text(command, "request")?.trim().to_owned();
        if !is_commit_id(&request) {
            return Err(invalid(format!(
                "\"request\" is not a request's id: {request}"
            )));
        }
        let kind = match text(command, "answer")? {
            "declined" => AnswerKind::Declined,
            "keep" => AnswerKind::Keep,
            other => {
                return Err(invalid(format!(
                    "\"answer\" is declined or keep, not '{other}'"
                )));
            }
        };
        let minutes = number(command, "minutes", (1, MAX_KEEP_MINUTES))?.unwrap_or(15);
        let message = message_field(command)?;
        let remote = self.lock_remote()?;
        self.look(&remote, control)?;
        let request_oid = gix::ObjectId::from_hex(request.as_bytes())
            .map_err(|_| invalid(format!("\"request\" is not a request's id: {request}")))?;
        let known = self.tracked(|t| {
            t.requests_of(&id)
                .into_iter()
                .any(|(_, seen)| seen.id == request_oid)
        });
        if !known {
            return self.take_answer("no_request", &path, &id, &me, false, None);
        }
        let answer = Answer {
            answer: kind,
            until: (kind == AnswerKind::Keep).then(|| now_utc() + i64::from(minutes) * 60),
            message,
        };
        let refresh = Refresh {
            answer: Some((request, answer)),
            ..Refresh::default()
        };
        let outcome = match self.refresh_lock(&remote, &me, &id, &refresh, control)? {
            Some(_) => "answered",
            None => "lost",
        };
        self.take_answer(outcome, &path, &id, &me, false, None)
    }

    /// `lock_probe`: whether the remote accepts Mitcad's lock refs: a probe
    /// ref under `refs/mitcad/locks/` created and deleted.
    fn lock_probe(&self, command: &Value, control: Option<&Control>) -> Result<Value, VcsError> {
        let me = self.me(command, false)?;
        let remote = self.lock_remote()?;
        let seed = format!(
            "{}\0{}\0{:?}",
            me.session,
            std::process::id(),
            std::time::SystemTime::now()
        );
        let suffix = mitcad_model::Sha256::of(seed.as_bytes()).to_string();
        let name = format!("{}probe-{}", super::LOCKS_PREFIX, &suffix[..16]);
        let person = me
            .person
            .clone()
            .unwrap_or_else(|| Person::new("Mitcad", "probe@mitcad.invalid"));
        let commit = self.write_ref_commit(
            "probe.json",
            b"{\"format\": \"mitcad-lock-probe\", \"version\": 1}\n",
            &person,
            &person,
            "Mitcad lock probe",
            now_utc(),
        )?;
        let create = Update {
            remote_ref: name.clone(),
            expected: None,
            new: Some(commit),
        };
        let (accepted, reason, left) = match self
            .push_updates(&remote.name, &[create], control)?
            .remove(0)
        {
            Written::Done => {
                // A remote that hides the refs it took (from its listing)
                // cannot serve locks either.
                let listed = self.git_ok(
                    &format!("Reading the edit locks of {}", remote.name),
                    &["ls-remote", &remote.name, &name],
                    control,
                )?;
                let visible = listed
                    .stdout
                    .lines()
                    .any(|line| line.split_once('\t').is_some_and(|(_, n)| n.trim() == name));
                let delete = Update {
                    remote_ref: name.clone(),
                    expected: Some(commit),
                    new: None,
                };
                match self
                    .push_updates(&remote.name, &[delete], control)?
                    .remove(0)
                {
                    _ if !visible => (
                        false,
                        Some("the remote does not list the lock references it takes".to_owned()),
                        None,
                    ),
                    Written::Done | Written::Changed(None) => (true, None, None),
                    Written::Changed(Some(_)) => (true, None, Some(name.clone())),
                    Written::Refused(reason) => (
                        false,
                        Some(format!(
                            "the remote does not let lock references be deleted ({})",
                            reason.trim()
                        )),
                        Some(name.clone()),
                    ),
                }
            }
            Written::Refused(reason) => {
                let reason = match reason.trim() {
                    "" => "the remote refused the reference".to_owned(),
                    text => text.to_owned(),
                };
                (false, Some(reason), None)
            }
            Written::Changed(_) => (
                false,
                Some("the probe reference was there already".to_owned()),
                None,
            ),
        };
        let message = if accepted {
            None
        } else {
            Some("This remote does not accept Mitcad's lock references".to_owned())
        };
        Ok(json!({
            "remote": remote.name,
            "accepted": accepted,
            "message": message,
            "reason": reason,
            "left": left,
        }))
    }
}

// Sync: what it would send that someone else holds.

impl ProjectRepo {
    /// Adds `locked` to a sync plan: the files its push would send (those
    /// the project's versions the remote lacks change) whose lock another
    /// session holds (without `session` in the command, the command line's
    /// session), as {`path`, `lock`}; from a new listing, or with `locks`
    /// "last" from what this process saw last. Empty when edit locks are
    /// off or nothing is sent; null with `locked_error` when the locks
    /// could not be read.
    pub(crate) fn plan_locks(&self, plan: &mut Value, command: &Value, control: Option<&Control>) {
        plan["locked"] = json!([]);
        plan["locked_error"] = Value::Null;
        if !self.lock_settings().enabled
            || !matches!(plan["case"].as_str(), Some("push" | "replay"))
        {
            return;
        }
        let result = (|| -> Result<Vec<Value>, VcsError> {
            let me = self.me(command, false)?;
            let paths = self.paths_to_send(plan)?;
            if paths.is_empty() {
                return Ok(Vec::new());
            }
            if optional(command, "locks") != Some("last") {
                let remote = self.lock_remote()?;
                self.look(&remote, control)?;
            }
            let project = self.lock_settings();
            Ok(self.tracked(|t| {
                paths
                    .iter()
                    .filter_map(|path| {
                        let id = lock_id(path);
                        let seen = t.locks.get(&id)?;
                        let mine = !me.session.is_empty()
                            && seen.holder.as_deref() == Some(me.session.as_str());
                        (!mine).then(|| {
                            json!({"path": path,
                                   "lock": lock_json(t, &id, seen, &me, &project, false)})
                        })
                    })
                    .collect()
            }))
        })();
        match result {
            Ok(list) => plan["locked"] = json!(list),
            Err(error) => {
                plan["locked"] = Value::Null;
                plan["locked_error"] = json!(error.to_string());
            }
        }
    }

    /// The paths a sync plan's push would send: changed between its `base`
    /// (none: everything) and its `head`, the B-rep store left out.
    fn paths_to_send(&self, plan: &Value) -> Result<Vec<String>, VcsError> {
        let id = |key: &str| {
            plan[key]
                .as_str()
                .and_then(|text| gix::ObjectId::from_hex(text.as_bytes()).ok())
        };
        let Some(head) = id("head") else {
            return Ok(Vec::new());
        };
        let repo = self.fresh()?;
        let tree = |commit: gix::ObjectId| -> Result<gix::ObjectId, VcsError> {
            Ok(repo.find_commit(commit)?.tree_id()?.detach())
        };
        let old = id("base").map(tree).transpose()?;
        Ok(self
            .differences(old, Some(tree(head)?))?
            .into_iter()
            .map(|difference| difference.path)
            .filter(|path| crate::tree::brep_sha256(path).is_none())
            .collect())
    }
}

/// Positions of requests per lock id, counted in order.
#[derive(Default)]
struct BTreeMapCounter(std::collections::BTreeMap<String, usize>);

impl BTreeMapCounter {
    fn next(&mut self, id: &str) -> usize {
        let count = self.0.entry(id.to_owned()).or_default();
        *count += 1;
        *count
    }
}

// The answers as text (mitcad-cli lock).

fn describe_lock(out: &mut String, lock: &Value) {
    let path = lock["path"].as_str().unwrap_or("(unreadable lock)");
    if lock["readable"].as_bool() != Some(true) {
        let _ = writeln!(
            out,
            "{} {}: a lock that cannot be read ({})",
            lock["id"].as_str().map_or("", |id| &id[..12.min(id.len())]),
            path,
            lock["problem"].as_str().unwrap_or("")
        );
        return;
    }
    let who = if lock["mine"].as_bool() == Some(true) {
        "this session".to_owned()
    } else {
        format!(
            "{} <{}>",
            lock["owner"]["name"].as_str().unwrap_or(""),
            lock["owner"]["email"].as_str().unwrap_or("")
        )
    };
    let mut line = format!(
        "{path}: edited by {who} since {}, last active {}",
        shown_time(&lock["taken_at"]),
        shown_time(&lock["active_at"])
    );
    if lock["state"].as_str() == Some("idle") {
        let _ = write!(line, " (idle since {})", shown_time(&lock["idle_since"]));
    }
    if let Some(stale) = lock["stale"].as_str() {
        let _ = write!(line, " (stale: {stale})");
    }
    let _ = writeln!(out, "{line}");
    let _ = writeln!(
        out,
        "  session {}, Mitcad {}",
        lock["session"].as_str().unwrap_or(""),
        lock["application_version"].as_str().unwrap_or("")
    );
    for request in lock["requests"].as_array().into_iter().flatten() {
        describe_request(out, request);
    }
}

fn describe_request(out: &mut String, request: &Value) {
    if request["readable"].as_bool() != Some(true) {
        let _ = writeln!(
            out,
            "  a request that cannot be read ({})",
            request["problem"].as_str().unwrap_or("")
        );
        return;
    }
    let mut line = format!(
        "  requested by {} <{}> at {}",
        request["requester"]["name"].as_str().unwrap_or(""),
        request["requester"]["email"].as_str().unwrap_or(""),
        shown_time(&request["asked_at"])
    );
    if let Some(message) = request["message"].as_str() {
        let _ = write!(line, ": \"{message}\"");
    }
    if let Some(answer) = request["answer"]["answer"].as_str() {
        let _ = write!(line, " ({answer})");
    } else if request["seen"].as_bool() == Some(true) {
        line.push_str(" (seen)");
    }
    if request["stale"].is_string() {
        line.push_str(" (stale: its Mitcad no longer refreshes it)");
    }
    let _ = writeln!(out, "{line}");
}

/// An edit-lock command's answer as text; None for another command.
pub(crate) fn describe(name: &str, answer: &Value) -> Option<String> {
    let mut out = String::new();
    let path = answer["path"].as_str().unwrap_or("");
    match name {
        "lock_status" | "lock_poll" => {
            let locks = answer["locks"].as_array().cloned().unwrap_or_default();
            if answer["path"].is_string() && locks.is_empty() {
                let _ = writeln!(out, "{path}: no edit lock");
            } else if locks.is_empty() {
                let _ = writeln!(out, "No edit locks");
            }
            for lock in &locks {
                describe_lock(&mut out, lock);
            }
            for request in answer["requests_without_lock"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let _ = write!(
                    out,
                    "{} (no lock):",
                    request["path"].as_str().unwrap_or("(unreadable request)")
                );
                let mut line = String::new();
                describe_request(&mut line, request);
                out.push_str(line.trim_start_matches(' '));
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            if let Some(dropped) = answer["dropped"].as_u64().filter(|n| *n > 0) {
                let _ = writeln!(out, "Malformed lock or request refs dropped: {dropped}");
            }
            for removed in answer["removed_requests"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "Removed a stale request for {} by {}",
                    removed["path"].as_str().unwrap_or("(unreadable request)"),
                    removed["requester"]["name"].as_str().unwrap_or("?")
                );
            }
        }
        "lock_take" | "lock_refresh" | "lock_hand_over" | "lock_request" | "lock_withdraw"
        | "lock_answer" => {
            let lock = &answer["lock"];
            let holder = format!(
                "{} <{}>",
                lock["owner"]["name"].as_str().unwrap_or("?"),
                lock["owner"]["email"].as_str().unwrap_or("?")
            );
            let _ = match answer["outcome"].as_str().unwrap_or("") {
                "taken" => match answer["taken_from"]["name"].as_str() {
                    Some(from) => writeln!(
                        out,
                        "Edit lock of {path} taken from {from} ({})",
                        answer["taken_from"]["reason"].as_str().unwrap_or("")
                    ),
                    None => writeln!(out, "Edit lock of {path} taken"),
                },
                "refreshed" => writeln!(out, "Edit lock of {path} held by this session"),
                "held" => {
                    let mut line = format!(
                        "{path} is being edited by {holder} (since {}, last active {})",
                        shown_time(&lock["taken_at"]),
                        shown_time(&lock["active_at"])
                    );
                    if lock["same_owner"].as_bool() == Some(true) {
                        line.push_str(": you have it open elsewhere");
                    }
                    writeln!(out, "{line}")
                }
                "changed" => writeln!(
                    out,
                    "The edit lock of {path} changed meanwhile: look again (lock status)"
                ),
                "lost" => writeln!(out, "The edit lock of {path} is not this session's"),
                "free" => writeln!(out, "{path} has no edit lock"),
                "mine" => writeln!(out, "This session holds the edit lock of {path}"),
                "handed_over" => writeln!(
                    out,
                    "Edit lock of {path} handed over to {} <{}>",
                    answer["to"]["name"].as_str().unwrap_or(""),
                    answer["to"]["email"].as_str().unwrap_or("")
                ),
                "no_request" => writeln!(out, "No request for the edit lock of {path}"),
                "requested" => writeln!(out, "Asked {holder} for the edit lock of {path}"),
                "withdrawn" => writeln!(out, "Request for the edit lock of {path} withdrawn"),
                "none" => writeln!(out, "No request of this session for {path}"),
                "answered" => writeln!(out, "Answered the request for {path}"),
                other => writeln!(out, "{path}: {other}"),
            };
        }
        "lock_release" => {
            let list = |key: &str| -> Vec<String> {
                answer[key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| p.as_str().unwrap_or("(unreadable)").to_owned())
                    .collect()
            };
            match answer["outcome"].as_str().unwrap_or("") {
                "not_held" => {
                    let lock = &answer["lock"];
                    let _ = writeln!(
                        out,
                        "The edit lock of {path} is held by {} <{}>, not this session: \
                         --force removes it",
                        lock["owner"]["name"].as_str().unwrap_or("?"),
                        lock["owner"]["email"].as_str().unwrap_or("?")
                    );
                }
                "free" if answer["path"].is_string() => {
                    let _ = writeln!(out, "{path} has no edit lock");
                }
                _ => {
                    for released in list("released") {
                        let _ = writeln!(out, "Released the edit lock of {released}");
                    }
                    for withdrawn in list("withdrawn") {
                        let _ = writeln!(out, "Removed a request for {withdrawn}");
                    }
                    for changed in list("changed") {
                        let _ = writeln!(out, "Not removed (changed meanwhile): {changed}");
                    }
                    if list("released").is_empty() && list("withdrawn").is_empty() {
                        let _ = writeln!(out, "Nothing to release");
                    }
                }
            }
        }
        "lock_probe" => {
            let _ = if answer["accepted"].as_bool() == Some(true) {
                writeln!(out, "The remote accepts Mitcad's lock references")
            } else {
                writeln!(
                    out,
                    "This remote does not accept Mitcad's lock references: {}",
                    answer["reason"].as_str().unwrap_or("")
                )
            };
        }
        _ => return None,
    }
    Some(out)
}
