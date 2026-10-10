// SPDX-License-Identifier: MIT
//! The topics and payloads of live updates (mitcad#89, "MQTT protocol"),
//! written and read.
//!
//! Topics, under the project's prefix:
//!
//! | Topic | Payload | |
//! |---|---|---|
//! | `<prefix>/<project>/locks/<file>` | [`LockSummary`] | retained; empty when the lock is released |
//! | `<prefix>/<project>/requests/<file>` | [`Request`] | QoS 1: requests, receipts, answers, withdrawals |
//! | `<prefix>/<project>/versions` | [`Version`] | QoS 1: a branch and commit after a push |
//! | `<prefix>/<project>/open/<file>/<session>` | [`OpenEntry`] | retained; empty when the window closes |
//! | `<prefix>/sessions/<session>` | [`Presence`] | retained `online`; the will `offline`; empty after a clean disconnect |
//!
//! `<project>` is the repository's first commit id, `<file>` the SHA-256 of
//! the file's path in the project, `<session>` the application's session
//! (a UUID). A session's presence is not under a project: one connection
//! has one will, and it serves every project on the broker.
//!
//! Every payload is JSON with `format` and `version` (1). A reader takes
//! only the known format of the topic and version 1; it checks every field
//! and gives typed values with the text for people cleaned
//! ([`super::check`]). Unknown fields are ignored, so that a later version
//! 1 writer may add some.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use mitcad_model::Sha256;
use mitcad_model::file::settings::{IDLE_MINUTES, POLL_SECONDS};

use super::check;

/// Payloads larger than this are not read.
pub const MAX_PAYLOAD: usize = 16 * 1024;
/// The version of every payload format.
pub const VERSION: u32 = 1;
pub const LOCK_FORMAT: &str = "mitcad-live-lock";
pub const REQUEST_FORMAT: &str = "mitcad-live-request";
pub const VERSION_FORMAT: &str = "mitcad-live-version";
pub const OPEN_FORMAT: &str = "mitcad-live-open";
pub const SESSION_FORMAT: &str = "mitcad-live-session";

/// A topic Mitcad uses, parsed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Topic {
    Lock {
        project: String,
        file: String,
    },
    Requests {
        project: String,
        file: String,
    },
    Versions {
        project: String,
    },
    Open {
        project: String,
        file: String,
        session: String,
    },
    Session {
        session: String,
    },
}

impl Topic {
    /// The topic's name under `prefix`.
    pub fn name(&self, prefix: &str) -> String {
        match self {
            Topic::Lock { project, file } => format!("{prefix}/{project}/locks/{file}"),
            Topic::Requests { project, file } => format!("{prefix}/{project}/requests/{file}"),
            Topic::Versions { project } => format!("{prefix}/{project}/versions"),
            Topic::Open {
                project,
                file,
                session,
            } => format!("{prefix}/{project}/open/{file}/{session}"),
            Topic::Session { session } => format!("{prefix}/sessions/{session}"),
        }
    }

    /// The project the topic is in (None for a session's presence).
    pub fn project(&self) -> Option<&str> {
        match self {
            Topic::Lock { project, .. }
            | Topic::Requests { project, .. }
            | Topic::Versions { project }
            | Topic::Open { project, .. } => Some(project),
            Topic::Session { .. } => None,
        }
    }

    /// Reads a topic name under `prefix`; None for anything that is not one
    /// of the known forms with valid ids.
    pub fn parse(prefix: &str, name: &str) -> Option<Topic> {
        let rest = name.strip_prefix(prefix)?.strip_prefix('/')?;
        let parts: Vec<&str> = rest.split('/').collect();
        let topic = match parts.as_slice() {
            ["sessions", session] if check::is_session(session) => Topic::Session {
                session: (*session).to_owned(),
            },
            [project, "versions"] if check::is_commit(project) => Topic::Versions {
                project: (*project).to_owned(),
            },
            [project, "locks", file] if check::is_commit(project) && check::is_file_id(file) => {
                Topic::Lock {
                    project: (*project).to_owned(),
                    file: (*file).to_owned(),
                }
            }
            [project, "requests", file] if check::is_commit(project) && check::is_file_id(file) => {
                Topic::Requests {
                    project: (*project).to_owned(),
                    file: (*file).to_owned(),
                }
            }
            [project, "open", file, session]
                if check::is_commit(project)
                    && check::is_file_id(file)
                    && check::is_session(session) =>
            {
                Topic::Open {
                    project: (*project).to_owned(),
                    file: (*file).to_owned(),
                    session: (*session).to_owned(),
                }
            }
            _ => return None,
        };
        Some(topic)
    }
}

/// The topic filter of everything in a project.
pub fn project_filter(prefix: &str, project: &str) -> String {
    format!("{prefix}/{project}/#")
}

/// A file's id: the SHA-256 of its path in the project.
pub fn file_id(path: &str) -> String {
    Sha256::of(path.as_bytes()).to_string()
}

/// A person as live messages show them: a name only (no email address:
/// the broker may be more public than the repository).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    Active,
    Idle,
}

/// What a lock ref says, for those who listen (`locks/<file>`): enough to
/// show the lock at once; the ref itself is read with git.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockSummary {
    pub path: String,
    pub owner: Person,
    /// The holder's session.
    pub session: String,
    pub state: LockState,
    pub taken_at: String,
    pub active_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_since: Option<String>,
    /// Clamped to the bounds of the project's settings.
    pub idle_minutes: u64,
    /// Clamped to the bounds of the project's settings.
    pub poll_seconds: u64,
    /// The commit the lock ref points to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// Someone asks for the lock.
    Request,
    /// The holder's application has seen the request.
    Receipt,
    /// The holder's answer.
    Answer,
    /// The requester no longer asks (its window closed).
    Withdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// The lock was handed over to the requester.
    Released,
    /// The holder keeps it until `until`.
    Keep,
    Declined,
}

/// A message about a request for a file's lock (`requests/<file>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub kind: RequestKind,
    /// Who sent it: the requester for `request` and `withdrawn`, the holder
    /// for `receipt` and `answer`.
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub at: String,
    /// The requester's session a receipt or an answer is for.
    #[serde(default, rename = "for", skip_serializing_if = "Option::is_none")]
    pub for_session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A version pushed (`versions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub branch: String,
    pub commit: String,
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenMode {
    #[serde(rename = "editing")]
    Editing,
    #[serde(rename = "read-only")]
    ReadOnly,
}

/// Who has a design open (`open/<file>/<session>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenEntry {
    pub name: String,
    pub mode: OpenMode,
    pub since: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Online,
    /// The session's connection was lost (its will).
    Offline,
}

/// A session's presence on the broker (`<prefix>/sessions/<session>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub state: PresenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

/// A message read from a topic, checked. An empty retained topic (a lock
/// released, a window closed, a clean disconnect) is None.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Lock(Option<LockSummary>),
    Request(Request),
    Version(Version),
    Open(Option<OpenEntry>),
    Session(Option<Presence>),
}

/// Why a message was dropped.
pub type Dropped = &'static str;

#[derive(Deserialize)]
struct Header {
    format: String,
    version: u64,
}

/// Wraps a payload with its format and version.
#[derive(Serialize)]
struct Envelope<'a, T: Serialize> {
    format: &'a str,
    version: u32,
    #[serde(flatten)]
    body: &'a T,
}

/// A payload's JSON with its format and version.
pub fn encode<T: Serialize>(format: &str, body: &T) -> Vec<u8> {
    serde_json::to_vec(&Envelope {
        format,
        version: VERSION,
        body,
    })
    .expect("live payloads are serializable")
}

/// The payload of a message (empty for a cleared retained topic).
pub fn encode_message(message: &Message) -> Vec<u8> {
    match message {
        Message::Lock(Some(summary)) => encode(LOCK_FORMAT, summary),
        Message::Request(request) => encode(REQUEST_FORMAT, request),
        Message::Version(version) => encode(VERSION_FORMAT, version),
        Message::Open(Some(entry)) => encode(OPEN_FORMAT, entry),
        Message::Session(Some(presence)) => encode(SESSION_FORMAT, presence),
        Message::Lock(None) | Message::Open(None) | Message::Session(None) => Vec::new(),
    }
}

/// Reads the payload of a message on `topic`, checked; the reason when it is
/// dropped.
pub fn read(topic: &Topic, payload: &[u8]) -> Result<Message, Dropped> {
    if payload.len() > MAX_PAYLOAD {
        return Err("a payload larger than 16 KiB");
    }
    let retained_empty = payload.is_empty();
    Ok(match topic {
        Topic::Lock { file, .. } => Message::Lock(if retained_empty {
            None
        } else {
            Some(lock(file, typed(LOCK_FORMAT, payload)?)?)
        }),
        Topic::Requests { .. } => Message::Request(request(typed(REQUEST_FORMAT, payload)?)?),
        Topic::Versions { .. } => Message::Version(version(typed(VERSION_FORMAT, payload)?)?),
        Topic::Open { .. } => Message::Open(if retained_empty {
            None
        } else {
            Some(open(typed(OPEN_FORMAT, payload)?)?)
        }),
        Topic::Session { .. } => Message::Session(if retained_empty {
            None
        } else {
            Some(presence(typed(SESSION_FORMAT, payload)?)?)
        }),
    })
}

/// The payload as `T` when its format is `format` and its version 1.
fn typed<T: DeserializeOwned>(format: &str, payload: &[u8]) -> Result<T, Dropped> {
    if payload.is_empty() {
        return Err("an empty payload");
    }
    let text = std::str::from_utf8(payload).map_err(|_| "a payload that is not UTF-8")?;
    let header: Header = serde_json::from_str(text)
        .map_err(|_| "a payload that is not JSON with a format and a version")?;
    if header.format != format {
        return Err("a payload of another format than its topic's");
    }
    if header.version != u64::from(VERSION) {
        return Err("a payload of an unknown version");
    }
    serde_json::from_str(text).map_err(|_| "a payload with missing or wrong fields")
}

fn session(text: &str) -> Result<String, Dropped> {
    if check::is_session(text) {
        Ok(text.to_owned())
    } else {
        Err("a session that is not a UUID")
    }
}

fn optional_time(text: Option<&str>) -> Result<Option<String>, Dropped> {
    text.map(check::time).transpose()
}

fn clamp(value: u64, (low, high): (u32, u32)) -> u64 {
    value.clamp(u64::from(low), u64::from(high))
}

fn lock(file: &str, raw: LockSummary) -> Result<LockSummary, Dropped> {
    if !check::is_path(&raw.path) {
        return Err("a lock with a path that is not relative to the project");
    }
    if file_id(&raw.path) != file {
        return Err("a lock whose path is not its topic's file");
    }
    if let Some(commit) = &raw.commit
        && !check::is_commit(commit)
    {
        return Err("a lock with a commit that is not a commit id");
    }
    Ok(LockSummary {
        owner: Person {
            name: check::name(&raw.owner.name)?,
        },
        session: session(&raw.session)?,
        taken_at: check::time(&raw.taken_at)?,
        active_at: check::time(&raw.active_at)?,
        idle_since: optional_time(raw.idle_since.as_deref())?,
        idle_minutes: clamp(raw.idle_minutes, IDLE_MINUTES),
        poll_seconds: clamp(raw.poll_seconds, POLL_SECONDS),
        ..raw
    })
}

fn request(raw: Request) -> Result<Request, Dropped> {
    let for_session = raw.for_session.as_deref().map(session).transpose()?;
    let needs_for = matches!(raw.kind, RequestKind::Receipt | RequestKind::Answer);
    if needs_for && for_session.is_none() {
        return Err("a receipt or an answer without the request it is for");
    }
    if raw.kind == RequestKind::Answer && raw.answer.is_none() {
        return Err("an answer without the answer");
    }
    if raw.kind != RequestKind::Answer && raw.answer.is_some() {
        return Err("an answer in a message that is not one");
    }
    let name = match raw.name.as_deref() {
        Some(name) => Some(check::name(name)?),
        None if raw.kind == RequestKind::Withdrawn => None,
        None => return Err("a request message without a name"),
    };
    let message = match raw.message.as_deref().map(check::message).transpose()? {
        Some(message) if message.is_empty() => None,
        message => message,
    };
    Ok(Request {
        session: session(&raw.session)?,
        name,
        at: check::time(&raw.at)?,
        for_session,
        until: optional_time(raw.until.as_deref())?,
        message,
        ..raw
    })
}

fn version(raw: Version) -> Result<Version, Dropped> {
    if !check::is_branch(&raw.branch) {
        return Err("a version on a branch name that is not allowed");
    }
    if !check::is_commit(&raw.commit) {
        return Err("a version with a commit that is not a commit id");
    }
    Ok(Version {
        session: session(&raw.session)?,
        name: raw.name.as_deref().map(check::name).transpose()?,
        at: check::time(&raw.at)?,
        ..raw
    })
}

fn open(raw: OpenEntry) -> Result<OpenEntry, Dropped> {
    Ok(OpenEntry {
        name: check::name(&raw.name)?,
        since: check::time(&raw.since)?,
        ..raw
    })
}

fn presence(raw: Presence) -> Result<Presence, Dropped> {
    Ok(Presence {
        since: optional_time(raw.since.as_deref())?,
        ..raw
    })
}

/// The format of the payloads of a topic.
pub fn format_of(topic: &Topic) -> &'static str {
    match topic {
        Topic::Lock { .. } => LOCK_FORMAT,
        Topic::Requests { .. } => REQUEST_FORMAT,
        Topic::Versions { .. } => VERSION_FORMAT,
        Topic::Open { .. } => OPEN_FORMAT,
        Topic::Session { .. } => SESSION_FORMAT,
    }
}
