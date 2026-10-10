// SPDX-License-Identifier: MIT
//! The hub of an application's live updates: its connections, one per
//! broker, user and prefix, shared by the projects on them; the JSON
//! commands (`core/model/src/api/commands.md`, "Live updates"); and the
//! events of every connection in one queue.
//!
//! A [`LiveHub`] is `Sync`: the application runs the commands on its UI
//! thread and reads the events with a blocking [`LiveHub::events`] on a
//! worker thread, both through `&LiveHub`.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mitcad_model::file::settings::{BrokerAddress, DEFAULT_PREFIX, EditLocks, valid_prefix};
use rustls::pki_types::CertificateDer;
use serde::Deserialize;
use serde_json::{Value, json};

use super::check;
use super::connection::{self, Command, Outgoing, SharedStatus, State, Status};
use super::events::EventQueue;
use super::messages::{
    self, Answer, LockState, LockSummary, OpenEntry, OpenMode, Person, Request, RequestKind, Topic,
    Version,
};
use super::transport::read_authorities;
use super::{LiveError, LiveErrorClass, probe};

/// The times of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// MQTT's keep-alive: a PINGREQ after three quarters of it without
    /// sending; no answer within it ends the connection.
    pub keep_alive: Duration,
    /// Connecting (per address), the TLS handshake, the CONNACK, a write.
    pub timeout: Duration,
    /// The longest a read waits before the thread looks at its commands.
    pub poll: Duration,
    /// The first delay before connecting again; it doubles after each
    /// failure up to `retry_max`.
    pub retry_initial: Duration,
    pub retry_max: Duration,
    /// A QoS 1 message without PUBACK this long is sent again.
    pub resend: Duration,
    /// A connection that lasted this long starts the back-off anew.
    pub stable: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            keep_alive: super::KEEP_ALIVE,
            timeout: Duration::from_secs(10),
            poll: Duration::from_millis(100),
            retry_initial: Duration::from_secs(1),
            retry_max: Duration::from_secs(300),
            resend: Duration::from_secs(20),
            stable: Duration::from_secs(60),
        }
    }
}

/// How a hub works.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HubOptions {
    pub timing: Timing,
}

impl HubOptions {
    /// The defaults, with `MITCAD_TEST_LIVE_RETRY_MS` (tests) as the first
    /// delay before connecting again and at most 20 times it.
    pub fn from_env() -> Self {
        let mut options = Self::default();
        if let Some(ms) = std::env::var("MITCAD_TEST_LIVE_RETRY_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
        {
            options.timing.retry_initial = Duration::from_millis(ms);
            options.timing.retry_max = Duration::from_millis(ms.saturating_mul(20));
        }
        options
    }
}

/// A connection of the hub.
struct Entry {
    id: String,
    broker: BrokerAddress,
    user: Option<String>,
    prefix: String,
    password: Option<String>,
    sender: Sender<Command>,
    status: SharedStatus,
    projects: BTreeSet<String>,
    thread: Option<JoinHandle<()>>,
}

impl Entry {
    fn state(&self) -> State {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .state
    }

    fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn close(&self) {
        let _ = self.sender.send(Command::Close);
    }
}

#[derive(Default)]
struct HubState {
    /// The application's session, set by the first `connect`.
    session: Option<String>,
    connections: Vec<Entry>,
    next: u64,
    closed: bool,
}

/// The live updates of an application.
pub struct LiveHub {
    state: Mutex<HubState>,
    events: Arc<EventQueue>,
    options: HubOptions,
}

/// Where and as whom to connect: the fields of `connect` and `test`.
pub(super) struct Target {
    pub broker: BrokerAddress,
    pub prefix: String,
    pub session: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub authorities: Vec<CertificateDer<'static>>,
}

const TARGET_FIELDS: [&str; 6] = ["broker", "prefix", "session", "user", "password", "ca"];

impl Target {
    pub(super) fn read(command: &Value) -> Result<Self, LiveError> {
        let broker = BrokerAddress::parse(text(command, "broker")?).map_err(LiveError::invalid)?;
        let prefix = optional(command, "prefix")?.unwrap_or(DEFAULT_PREFIX);
        if !valid_prefix(prefix) {
            return Err(LiveError::invalid(format!(
                "'{prefix}' is not a topic prefix (letters, digits, -, _ and /)"
            )));
        }
        let session = optional(command, "session")?
            .map(|session| {
                if check::is_session(session) {
                    Ok(session.to_owned())
                } else {
                    Err(LiveError::invalid(format!(
                        "'{session}' is not a session (a UUID in lowercase)"
                    )))
                }
            })
            .transpose()?;
        let user = optional(command, "user")?.map(str::to_owned);
        if user
            .as_ref()
            .is_some_and(|u| u.len() > 1024 || u.contains('\0'))
        {
            return Err(LiveError::invalid(
                "the user name is too long or has U+0000",
            ));
        }
        let password = optional(command, "password")?.map(str::to_owned);
        if password.is_some() && !broker.tls {
            return Err(LiveError::new(
                LiveErrorClass::Insecure,
                format!(
                    "a password is sent only over TLS: {broker} is plain text; use mqtts:// \
                     or connect without a password"
                ),
            ));
        }
        if password.is_some() && user.is_none() {
            return Err(LiveError::invalid("a password needs a user name"));
        }
        let authorities = match optional(command, "ca")? {
            Some(path) => read_authorities(Path::new(path))?,
            None => Vec::new(),
        };
        Ok(Self {
            broker,
            prefix: prefix.to_owned(),
            session,
            user,
            password,
            authorities,
        })
    }

    /// What to tell about a plain-text connection.
    pub(super) fn warning(&self) -> Option<String> {
        (!self.broker.tls).then(|| {
            format!(
                "{} is plain text: anyone on the network between this computer and the broker \
                 can read the messages (names and the designs open)",
                self.broker
            )
        })
    }
}

fn invalid(message: impl Into<String>) -> LiveError {
    LiveError::invalid(message)
}

/// Refuses fields a command does not have (a typo would otherwise be lost).
fn allowed(command: &Value, fields: &[&str]) -> Result<(), LiveError> {
    let object = command
        .as_object()
        .ok_or_else(|| invalid("a live command or message is a JSON object"))?;
    match object.keys().find(|key| !fields.contains(&key.as_str())) {
        Some(key) => Err(invalid(format!("unknown field \"{key}\""))),
        None => Ok(()),
    }
}

fn text<'a>(command: &'a Value, key: &str) -> Result<&'a str, LiveError> {
    optional(command, key)?.ok_or_else(|| invalid(format!("the command needs \"{key}\" (text)")))
}

/// An optional text field; missing, null and empty are None.
fn optional<'a>(command: &'a Value, key: &str) -> Result<Option<&'a str>, LiveError> {
    match command.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(invalid(format!("\"{key}\" must be text"))),
    }
}

/// `wait_ms` of `disconnect` and `close`: at most 10 s, default none.
fn wait_ms(command: &Value) -> Result<Duration, LiveError> {
    match command.get("wait_ms") {
        None | Some(Value::Null) => Ok(Duration::ZERO),
        Some(ms) => ms
            .as_u64()
            .map(|ms| Duration::from_millis(ms.min(10_000)))
            .ok_or_else(|| invalid("\"wait_ms\" must be a whole number")),
    }
}

fn project_field(command: &Value) -> Result<&str, LiveError> {
    let project = text(command, "project")?;
    if check::is_commit(project) {
        Ok(project)
    } else {
        Err(invalid(format!(
            "'{project}' is not a project id (its first commit in hexadecimal)"
        )))
    }
}

impl Default for LiveHub {
    fn default() -> Self {
        Self::new(HubOptions::default())
    }
}

impl LiveHub {
    pub fn new(options: HubOptions) -> Self {
        Self {
            state: Mutex::default(),
            events: Arc::new(EventQueue::default()),
            options,
        }
    }

    fn lock(&self) -> MutexGuard<'_, HubState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs a JSON command (commands.md, "Live updates"); answers in JSON.
    pub fn command(&self, json: &str) -> Result<String, LiveError> {
        let command: Value =
            serde_json::from_str(json).map_err(|e| invalid(format!("not a JSON command: {e}")))?;
        let answer = self.run(&command)?;
        // `test` answers in text for people with "text": true.
        Ok(match (command["cmd"].as_str(), command.get("text")) {
            (Some("test"), Some(Value::Bool(true))) => probe::describe(&answer),
            _ => answer.to_string(),
        })
    }

    /// The events that waited, after waiting up to `timeout` for one:
    /// `{"events": [...], "lost": n, "closed": bool}`.
    pub fn events(&self, timeout: Duration) -> String {
        let (events, lost, closed) = self.events.take(timeout);
        json!({"events": events, "lost": lost, "closed": closed}).to_string()
    }

    fn run(&self, command: &Value) -> Result<Value, LiveError> {
        let name = text(command, "cmd")?;
        let mut fields = vec!["cmd"];
        match name {
            "connect" => {
                fields.extend(TARGET_FIELDS);
                allowed(command, &fields)?;
                self.connect(command).map(|(_, answer)| answer)
            }
            "subscribe" => {
                fields.extend(TARGET_FIELDS);
                fields.extend(["connection", "project"]);
                allowed(command, &fields)?;
                self.subscribe(command)
            }
            "unsubscribe" => {
                allowed(command, &["cmd", "connection", "project"])?;
                self.unsubscribe(command)
            }
            "watch" => {
                allowed(command, &["cmd", "connection", "session"])?;
                self.watch(command)
            }
            "publish" => {
                allowed(command, &["cmd", "connection", "project", "message"])?;
                self.publish(command)
            }
            "disconnect" => {
                allowed(command, &["cmd", "connection", "wait_ms"])?;
                self.disconnect(optional(command, "connection")?, wait_ms(command)?)
            }
            "status" => {
                allowed(command, &["cmd"])?;
                Ok(self.status())
            }
            "test" => {
                fields.extend(TARGET_FIELDS);
                fields.extend(["timeout_ms", "text"]);
                allowed(command, &fields)?;
                let target = Target::read(command)?;
                let timeout = match command.get("timeout_ms") {
                    None | Some(Value::Null) => self.options.timing.timeout,
                    Some(ms) => Duration::from_millis(
                        ms.as_u64()
                            .ok_or_else(|| invalid("\"timeout_ms\" must be a whole number"))?
                            .clamp(100, 60_000),
                    ),
                };
                Ok(probe::run(&target, timeout))
            }
            "close" => {
                allowed(command, &["cmd", "wait_ms"])?;
                let answer = self.disconnect(None, wait_ms(command)?)?;
                self.lock().closed = true;
                self.events.close();
                Ok(answer)
            }
            other => Err(invalid(format!("unknown live command '{other}'"))),
        }
    }

    /// Opens the connection for a broker, user and prefix, or finds the
    /// one open; returns its id and the answer.
    fn connect(&self, command: &Value) -> Result<(String, Value), LiveError> {
        let target = Target::read(command)?;
        let session = target
            .session
            .clone()
            .ok_or_else(|| invalid("the command needs \"session\" (text)"))?;
        let mut state = self.lock();
        if state.closed {
            return Err(LiveError::new(
                LiveErrorClass::Closed,
                "live updates are closed",
            ));
        }
        match &state.session {
            Some(own) if *own != session => {
                return Err(invalid(format!(
                    "this application's session is {own}, not {session}"
                )));
            }
            Some(_) => {}
            None => state.session = Some(session.clone()),
        }
        let warning = target.warning();
        let found = state.connections.iter().position(|e| {
            e.broker == target.broker && e.user == target.user && e.prefix == target.prefix
        });
        let index = match found {
            Some(index) => {
                let entry = &mut state.connections[index];
                let changed = entry.password != target.password;
                entry.password = target.password.clone();
                if changed || matches!(entry.state(), State::Failed | State::Offline) {
                    let _ = entry.sender.send(Command::Retry {
                        password: target.password.clone(),
                        authorities: target.authorities.clone(),
                    });
                }
                index
            }
            None => {
                state.next += 1;
                let id = format!("c{}", state.next);
                let (sender, receiver) = mpsc::channel();
                let status = SharedStatus::default();
                let settings = connection::Settings {
                    id: id.clone(),
                    broker: target.broker.clone(),
                    prefix: target.prefix.clone(),
                    session,
                    user: target.user.clone(),
                    password: target.password.clone(),
                    authorities: target.authorities.clone(),
                    timing: self.options.timing,
                };
                let thread = connection::spawn(
                    settings,
                    receiver,
                    Arc::clone(&self.events),
                    Arc::clone(&status),
                )
                .map_err(|e| {
                    LiveError::new(
                        LiveErrorClass::Internal,
                        format!("cannot start a connection's thread: {e}"),
                    )
                })?;
                state.connections.push(Entry {
                    id,
                    broker: target.broker.clone(),
                    user: target.user.clone(),
                    prefix: target.prefix.clone(),
                    password: target.password.clone(),
                    sender,
                    status,
                    projects: BTreeSet::new(),
                    thread: Some(thread),
                });
                state.connections.len() - 1
            }
        };
        let entry = &state.connections[index];
        let answer = json!({
            "connection": entry.id,
            "broker": entry.broker.to_string(),
            "prefix": entry.prefix,
            "user": entry.user,
            "tls": entry.broker.tls,
            "state": entry.state(),
            "warning": warning,
        });
        Ok((entry.id.clone(), answer))
    }

    fn subscribe(&self, command: &Value) -> Result<Value, LiveError> {
        let project = project_field(command)?.to_owned();
        let id = match optional(command, "connection")? {
            Some(id) => id.to_owned(),
            None => self.connect(command)?.0,
        };
        let mut state = self.lock();
        let entry = find(&mut state, &id)?;
        entry.projects.insert(project.clone());
        let _ = entry.sender.send(Command::Subscribe(project.clone()));
        Ok(json!({
            "connection": id,
            "project": project,
            "topic": messages::project_filter(&entry.prefix, &project),
            "state": entry.state(),
        }))
    }

    fn unsubscribe(&self, command: &Value) -> Result<Value, LiveError> {
        let id = text(command, "connection")?;
        let project = project_field(command)?.to_owned();
        let mut state = self.lock();
        let entry = find(&mut state, id)?;
        let was = entry.projects.remove(&project);
        let _ = entry.sender.send(Command::Unsubscribe(project.clone()));
        let closed = entry.projects.is_empty();
        if closed {
            entry.close();
            state.connections.retain(|e| e.id != id);
        }
        Ok(json!({"connection": id, "project": project, "subscribed": was, "closed": closed}))
    }

    fn watch(&self, command: &Value) -> Result<Value, LiveError> {
        let id = text(command, "connection")?;
        let session = text(command, "session")?;
        if !check::is_session(session) {
            return Err(invalid(format!("'{session}' is not a session (a UUID)")));
        }
        let mut state = self.lock();
        let entry = find(&mut state, id)?;
        let _ = entry.sender.send(Command::Watch(session.to_owned()));
        Ok(json!({"connection": id, "session": session}))
    }

    fn publish(&self, command: &Value) -> Result<Value, LiveError> {
        let id = text(command, "connection")?;
        let project = project_field(command)?;
        let message = command
            .get("message")
            .ok_or_else(|| invalid("the command needs \"message\" (an object)"))?;
        let mut state = self.lock();
        let session = state.session.clone().unwrap_or_default();
        let entry = find(&mut state, id)?;
        if !entry.projects.contains(project) {
            return Err(invalid(format!(
                "project {project} is not subscribed on connection {id}"
            )));
        }
        let outgoing = outgoing(&entry.prefix, &session, project, message)?;
        let answer = json!({
            "connection": id,
            "topic": outgoing.topic,
            "qos": outgoing.qos,
            "retain": outgoing.retain,
            "connected": entry.state() == State::Connected,
        });
        let _ = entry.sender.send(Command::Publish(outgoing));
        Ok(answer)
    }

    /// Closes a connection, or all; waits up to `wait` for them to have
    /// disconnected (the application before it quits).
    fn disconnect(&self, id: Option<&str>, wait: Duration) -> Result<Value, LiveError> {
        let mut state = self.lock();
        if let Some(id) = id {
            find(&mut state, id)?;
        }
        let mut closed = Vec::new();
        let mut threads = Vec::new();
        state.connections.retain_mut(|entry| {
            if id.is_none_or(|id| entry.id == id) {
                entry.close();
                closed.push(entry.id.clone());
                threads.extend(entry.thread.take());
                false
            } else {
                true
            }
        });
        drop(state);
        let deadline = Instant::now() + wait;
        while threads.iter().any(|t| !t.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let finished = threads.iter().all(JoinHandle::is_finished);
        Ok(json!({"closed": closed, "finished": finished}))
    }

    fn status(&self) -> Value {
        let state = self.lock();
        let connections: Vec<Value> = state
            .connections
            .iter()
            .map(|entry| {
                let mut status = json!(entry.status());
                status["connection"] = json!(entry.id);
                status["broker"] = json!(entry.broker.to_string());
                status["prefix"] = json!(entry.prefix);
                status["user"] = json!(entry.user);
                status["projects"] = json!(entry.projects);
                status
            })
            .collect();
        json!({"session": state.session, "closed": state.closed, "connections": connections})
    }

    /// Ends a connection's socket at once, as a crash would: the broker
    /// publishes its will (tests).
    #[cfg(test)]
    pub(super) fn kill(&self, id: &str) {
        let mut state = self.lock();
        if let Ok(entry) = find(&mut state, id) {
            let _ = entry.sender.send(Command::Kill);
        }
        state.connections.retain(|e| e.id != id);
    }
}

impl Drop for LiveHub {
    fn drop(&mut self) {
        for entry in &self.lock().connections {
            entry.close();
        }
        self.events.close();
    }
}

fn find<'a>(state: &'a mut HubState, id: &str) -> Result<&'a mut Entry, LiveError> {
    state
        .connections
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| invalid(format!("no live connection '{id}'")))
}

/// The owner of a lock as the application gives it (`lock.json`'s owner:
/// its email address is not sent).
#[derive(Deserialize)]
struct OwnerInput {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockInput {
    owner: OwnerInput,
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    state: Option<LockState>,
    #[serde(default)]
    taken_at: Option<String>,
    #[serde(default)]
    active_at: Option<String>,
    #[serde(default)]
    idle_since: Option<String>,
    #[serde(default)]
    idle_minutes: Option<u64>,
    #[serde(default)]
    poll_seconds: Option<u64>,
    #[serde(default)]
    commit: Option<String>,
}

/// The message of a `publish` command made into what is sent, checked by
/// reading it back as every other session will.
pub(super) fn outgoing(
    prefix: &str,
    session: &str,
    project: &str,
    message: &Value,
) -> Result<Outgoing, LiveError> {
    let kind = text(message, "type")?;
    let now = check::format_time(check::now());
    let project = project.to_owned();
    let path = || -> Result<String, LiveError> {
        let path = text(message, "path")?;
        if check::is_path(path) {
            Ok(path.to_owned())
        } else {
            Err(invalid(format!(
                "'{path}' is not a path relative to the project ('/' between folders)"
            )))
        }
    };
    let optional_text = |key: &str| optional(message, key).map(|v| v.map(str::to_owned));
    let (topic, payload, retain, owned) = match kind {
        "lock" => {
            allowed(message, &["type", "path", "lock"])?;
            let path = path()?;
            let topic = Topic::Lock {
                project,
                file: messages::file_id(&path),
            };
            match message.get("lock") {
                None | Some(Value::Null) => (topic, Vec::new(), true, true),
                Some(lock) => {
                    let input: LockInput = serde_json::from_value(lock.clone())
                        .map_err(|e| invalid(format!("\"lock\": {e}")))?;
                    let defaults = EditLocks::default();
                    let holder = input.session.unwrap_or_else(|| session.to_owned());
                    let summary = LockSummary {
                        path,
                        owner: Person {
                            name: input.owner.name,
                        },
                        session: holder.clone(),
                        state: input.state.unwrap_or(LockState::Active),
                        taken_at: input.taken_at.unwrap_or_else(|| now.clone()),
                        active_at: input.active_at.unwrap_or_else(|| now.clone()),
                        idle_since: input.idle_since,
                        idle_minutes: input
                            .idle_minutes
                            .unwrap_or(u64::from(defaults.idle_minutes)),
                        poll_seconds: input
                            .poll_seconds
                            .unwrap_or(u64::from(defaults.poll_seconds)),
                        commit: input.commit,
                    };
                    // A summary of another session's lock (a hand-over) is
                    // that session's to keep up to date.
                    let owned = holder == session;
                    (
                        topic,
                        messages::encode(messages::LOCK_FORMAT, &summary),
                        true,
                        owned,
                    )
                }
            }
        }
        "request" | "receipt" | "answer" | "withdrawn" => {
            allowed(
                message,
                &["type", "path", "name", "for", "answer", "until", "message"],
            )?;
            let path = path()?;
            let kind = match kind {
                "request" => RequestKind::Request,
                "receipt" => RequestKind::Receipt,
                "answer" => RequestKind::Answer,
                _ => RequestKind::Withdrawn,
            };
            let answer = match optional(message, "answer")? {
                None => None,
                Some("released") => Some(Answer::Released),
                Some("keep") => Some(Answer::Keep),
                Some("declined") => Some(Answer::Declined),
                Some(other) => {
                    return Err(invalid(format!(
                        "'{other}' is not an answer (released, keep or declined)"
                    )));
                }
            };
            let request = Request {
                kind,
                session: session.to_owned(),
                name: optional_text("name")?,
                at: now.clone(),
                for_session: optional_text("for")?,
                answer,
                until: optional_text("until")?,
                message: optional_text("message")?,
            };
            let topic = Topic::Requests {
                project,
                file: messages::file_id(&path),
            };
            (
                topic,
                messages::encode(messages::REQUEST_FORMAT, &request),
                false,
                false,
            )
        }
        "version" => {
            allowed(message, &["type", "branch", "commit", "name"])?;
            let version = Version {
                branch: text(message, "branch")?.to_owned(),
                commit: text(message, "commit")?.to_owned(),
                session: session.to_owned(),
                name: optional_text("name")?,
                at: now.clone(),
            };
            (
                Topic::Versions { project },
                messages::encode(messages::VERSION_FORMAT, &version),
                false,
                false,
            )
        }
        "open" => {
            allowed(message, &["type", "path", "name", "mode", "since"])?;
            let path = path()?;
            let mode = match text(message, "mode")? {
                "editing" => OpenMode::Editing,
                "read-only" => OpenMode::ReadOnly,
                other => {
                    return Err(invalid(format!(
                        "'{other}' is not a mode (editing or read-only)"
                    )));
                }
            };
            let entry = OpenEntry {
                name: text(message, "name")?.to_owned(),
                mode,
                since: optional_text("since")?.unwrap_or_else(|| now.clone()),
            };
            let topic = Topic::Open {
                project,
                file: messages::file_id(&path),
                session: session.to_owned(),
            };
            (
                topic,
                messages::encode(messages::OPEN_FORMAT, &entry),
                true,
                true,
            )
        }
        "closed" => {
            allowed(message, &["type", "path"])?;
            let path = path()?;
            let topic = Topic::Open {
                project,
                file: messages::file_id(&path),
                session: session.to_owned(),
            };
            (topic, Vec::new(), true, true)
        }
        other => {
            return Err(invalid(format!(
                "unknown live message '{other}' (lock, request, receipt, answer, withdrawn, \
                 version, open, closed)"
            )));
        }
    };
    // What others will read: names cleaned, times in UTC, numbers clamped.
    let payload = if payload.is_empty() {
        payload
    } else {
        match messages::read(&topic, &payload) {
            Ok(message) => messages::encode_message(&message),
            Err(reason) => return Err(invalid(format!("the message cannot be sent: {reason}"))),
        }
    };
    Ok(Outgoing {
        topic: topic.name(prefix),
        payload,
        qos: 1,
        retain,
        owned,
    })
}
