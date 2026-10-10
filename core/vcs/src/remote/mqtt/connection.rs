// SPDX-License-Identifier: MIT
//! One connection to a broker, on a thread of its own: connecting and
//! reconnecting with a back-off, the projects' subscriptions, publishing
//! with PUBACK and resending, keep-alive, and every message received
//! checked before it becomes an event.
//!
//! The thread owns the socket. The hub talks to it through a channel of
//! [`Command`]s, which the thread reads between reads of the socket (a read
//! waits at most [`Timing::poll`]); it reports its state in a shared
//! [`Status`] and its events in the hub's [`EventQueue`].
//!
//! What the session wants survives a reconnect: the projects, the sessions
//! whose presence it follows, the retained messages it owns (its lock
//! summaries and the designs it has open: published again after each
//! reconnect, so that the broker has their latest state) and QoS 1 messages
//! not acknowledged yet (sent again, marked as such).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mitcad_model::file::settings::BrokerAddress;
use rustls::pki_types::CertificateDer;
use serde::Serialize;
use serde_json::{Value, json};

use super::check;
use super::events::EventQueue;
use super::messages::{self, Message, Presence, PresenceState, Topic};
use super::packet::{Connect, Decoder, Packet, Publish, Will};
use super::transport::{Transport, hex, io_error, is_timeout, random_bytes};
use super::{
    LiveError, LiveErrorClass, MAX_MESSAGES_PER_SECOND, MAX_PACKET, MAX_RETAINED_PER_FILE,
    MAX_RETAINED_PER_PROJECT, MAX_RETAINED_PER_SECOND, MAX_WATCHED_SESSIONS, Timing,
};

/// QoS 1 messages sent and not acknowledged at most at once (besides the
/// retained ones the session owns).
const MAX_INFLIGHT: usize = 32;
/// QoS 1 messages waiting to be sent at most; more push out the oldest.
const MAX_OUTBOX: usize = 256;
/// Session filters per SUBSCRIBE.
const FILTERS_PER_SUBSCRIBE: usize = 32;

/// What a connection is for.
pub struct Settings {
    /// The hub's name of the connection (`c1`).
    pub id: String,
    pub broker: BrokerAddress,
    pub prefix: String,
    /// The application's session.
    pub session: String,
    pub user: Option<String>,
    /// Only with TLS (the hub checks it).
    pub password: Option<String>,
    /// Certificate authorities trusted besides the system's.
    pub authorities: Vec<CertificateDer<'static>>,
    pub timing: Timing,
}

/// A message to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    /// A retained message of this session's own state, published again
    /// after each reconnect while it is the topic's latest.
    pub owned: bool,
}

pub enum Command {
    Subscribe(String),
    Unsubscribe(String),
    /// Follow a session's presence.
    Watch(String),
    Publish(Outgoing),
    /// Connect again now (when not connected), with these credentials and
    /// certificate authorities from now on.
    Retry {
        password: Option<String>,
        authorities: Vec<CertificateDer<'static>>,
    },
    /// Clear this session's state on the broker, disconnect and end.
    Close,
    /// End the socket at once, as a crash would (tests).
    #[cfg(test)]
    Kill,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Connecting,
    Connected,
    /// Waiting to connect again.
    Offline,
    /// Not connecting again until a new `connect` (the password, the
    /// certificate).
    Failed,
    Closed,
}

/// What the hub shows of a connection.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    pub state: State,
    pub error: Option<LiveError>,
    pub retry_in_ms: Option<u64>,
    /// RFC 3339.
    pub connected_since: Option<String>,
    pub tls: bool,
    /// Successful connections so far.
    pub connects: u64,
    /// Messages received.
    pub received: u64,
    /// Messages dropped, in all and per reason.
    pub dropped: u64,
    pub dropped_reasons: BTreeMap<String, u64>,
    /// QoS 1 messages not acknowledged yet, and waiting to be sent.
    pub inflight: usize,
    pub queued: usize,
    pub watched_sessions: usize,
}

pub type SharedStatus = Arc<Mutex<Status>>;

/// Starts a connection's thread.
pub fn spawn(
    settings: Settings,
    commands: Receiver<Command>,
    events: Arc<EventQueue>,
    status: SharedStatus,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let name = format!("mitcad-live-{}", settings.id);
    std::thread::Builder::new().name(name).spawn(move || {
        let client = Client::new(settings, commands, Arc::clone(&events), Arc::clone(&status));
        let id = client.settings.id.clone();
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| client.run())).is_err() {
            let error = LiveError::new(LiveErrorClass::Internal, "the connection's thread failed");
            if let Ok(mut status) = status.lock() {
                status.state = State::Closed;
                status.error = Some(error.clone());
            }
            events.push(
                json!({"type": "state", "connection": id, "state": State::Closed,
                               "error": error, "retry_in_ms": null}),
            );
        }
    })
}

struct InFlight {
    message: Outgoing,
    sent_at: Instant,
    /// The clearing of another session's open entry (that session went
    /// offline): the `open` event it gives once the broker has acknowledged
    /// it, none for an entry never given as an event.
    clearing: Option<Option<Value>>,
}

/// What a SUBSCRIBE waiting for its SUBACK is for.
enum Subscription {
    Project(String),
    /// The presence of these sessions, one filter each.
    Sessions(Vec<String>),
}

/// A followed session's presence as last received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Online,
    /// Its will: its connection was lost.
    Offline,
    /// Disconnected cleanly (its presence emptied).
    Left,
    /// The broker refused the subscription to its presence: never known.
    Refused,
}

/// The retained messages tracked of a project: per file, whether it has a
/// lock summary and the sessions that have it open.
#[derive(Default)]
pub struct Tracked {
    files: BTreeMap<String, FileEntries>,
    entries: usize,
}

#[derive(Default)]
struct FileEntries {
    lock: bool,
    open: BTreeSet<String>,
}

impl FileEntries {
    fn len(&self) -> usize {
        usize::from(self.lock) + self.open.len()
    }
}

impl Tracked {
    /// Records that a file's lock summary or a session's open entry
    /// (`session`) is there or not; refused beyond the limits.
    pub fn set(
        &mut self,
        file: &str,
        session: Option<&str>,
        present: bool,
    ) -> Result<(), &'static str> {
        let entries = self.files.entry(file.to_owned()).or_default();
        let had = match session {
            None => entries.lock,
            Some(session) => entries.open.contains(session),
        };
        if present && !had {
            if entries.len() >= MAX_RETAINED_PER_FILE {
                return Err("more than 64 retained messages for one file");
            }
            if self.entries >= MAX_RETAINED_PER_PROJECT {
                return Err("more than 10000 retained messages in one project");
            }
            self.entries += 1;
        } else if !present && had {
            self.entries -= 1;
        }
        match session {
            None => entries.lock = present,
            Some(session) if present => {
                entries.open.insert(session.to_owned());
            }
            Some(session) => {
                entries.open.remove(session);
            }
        }
        if entries.len() == 0 {
            self.files.remove(file);
        }
        Ok(())
    }

    /// Whether a session has a file open.
    fn has_open(&self, file: &str, session: &str) -> bool {
        self.files
            .get(file)
            .is_some_and(|entries| entries.open.contains(session))
    }

    /// The files a session has open.
    fn opened_by(&self, session: &str) -> Vec<String> {
        self.files
            .iter()
            .filter(|(_, entries)| entries.open.contains(session))
            .map(|(file, _)| file.clone())
            .collect()
    }
}

/// Counts messages per second: at most [`MAX_MESSAGES_PER_SECOND`], and
/// apart from them [`MAX_RETAINED_PER_SECOND`] retained copies, which the
/// broker sends all at once after a subscribe (a project with many
/// designs open has many).
pub struct RateLimit {
    start: Instant,
    count: u32,
    retained: u32,
}

impl RateLimit {
    pub fn new(now: Instant) -> Self {
        Self {
            start: now,
            count: 0,
            retained: 0,
        }
    }

    /// Counts a message at `now`; true when the second it is in has more
    /// than allowed.
    pub fn exceeded(&mut self, now: Instant, retained: bool) -> bool {
        if now.duration_since(self.start) >= Duration::from_secs(1) {
            self.start = now;
            self.count = 0;
            self.retained = 0;
        }
        if retained {
            self.retained += 1;
            self.retained > MAX_RETAINED_PER_SECOND
        } else {
            self.count += 1;
            self.count > MAX_MESSAGES_PER_SECOND
        }
    }
}

/// How a connected period ended.
enum End {
    /// Closed on request.
    Closed,
    /// Lost, or ended because of the broker: connect again.
    Lost(LiveError),
    #[cfg(test)]
    Killed,
}

/// The socket's side of a connected period.
struct Link {
    transport: Transport,
    decoder: Decoder,
    last_sent: Instant,
    ping_sent: Option<Instant>,
    rate: RateLimit,
}

struct Client {
    settings: Settings,
    commands: Receiver<Command>,
    events: Arc<EventQueue>,
    status: SharedStatus,
    client_id: String,
    projects: BTreeSet<String>,
    watched: BTreeSet<String>,
    owned: BTreeMap<String, Vec<u8>>,
    outbox: VecDeque<Outgoing>,
    inflight: BTreeMap<u16, InFlight>,
    /// SUBSCRIBE packets waiting for their SUBACK.
    subscribing: BTreeMap<u16, Subscription>,
    tracked: BTreeMap<String, Tracked>,
    /// The followed sessions' presence received in this connected period.
    presence: BTreeMap<String, Seen>,
    /// Open entries of followed sessions not known to be online yet: per
    /// session, the `open` event of each topic, given once the session is
    /// seen online (an entry of a session gone offline is never given).
    held: BTreeMap<String, BTreeMap<String, Value>>,
    next_id: u16,
    backoff: Duration,
    link: Option<Link>,
}

/// The JSON of an error, or null.
fn error_json(error: Option<&LiveError>) -> Value {
    json!(error)
}

impl Client {
    fn new(
        settings: Settings,
        commands: Receiver<Command>,
        events: Arc<EventQueue>,
        status: SharedStatus,
    ) -> Self {
        // 23 characters at most, as MQTT 3.1.1 asks brokers to accept.
        let client_id = format!("mitcad{}", &hex(&random_bytes::<9>())[..17]);
        let backoff = settings.timing.retry_initial;
        Self {
            settings,
            commands,
            events,
            status,
            client_id,
            projects: BTreeSet::new(),
            watched: BTreeSet::new(),
            owned: BTreeMap::new(),
            outbox: VecDeque::new(),
            inflight: BTreeMap::new(),
            subscribing: BTreeMap::new(),
            tracked: BTreeMap::new(),
            presence: BTreeMap::new(),
            held: BTreeMap::new(),
            next_id: 1,
            backoff,
            link: None,
        }
    }

    fn update(&self, f: impl FnOnce(&mut Status)) {
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut status);
        status.inflight = self.inflight.len();
        status.queued = self.outbox.len();
        status.watched_sessions = self.watched.len();
    }

    fn set_state(&self, state: State, error: Option<LiveError>, retry: Option<Duration>) {
        let retry_in_ms = retry.map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let tls = self
            .link
            .as_ref()
            .is_some_and(|link| link.transport.is_tls());
        self.update(|status| {
            status.state = state;
            status.error = error.clone();
            status.retry_in_ms = retry_in_ms;
            if state == State::Connected {
                status.connected_since = Some(check::format_time(check::now()));
                status.connects += 1;
                status.tls = tls;
            } else {
                status.connected_since = None;
            }
        });
        self.events.push(json!({
            "type": "state",
            "connection": self.settings.id,
            "broker": self.settings.broker.to_string(),
            "prefix": self.settings.prefix,
            "state": state,
            "error": error_json(error.as_ref()),
            "retry_in_ms": retry_in_ms,
        }));
    }

    fn presence_topic(&self) -> String {
        Topic::Session {
            session: self.settings.session.clone(),
        }
        .name(&self.settings.prefix)
    }

    fn presence(&self, state: PresenceState) -> Vec<u8> {
        let since = (state == PresenceState::Online).then(|| check::format_time(check::now()));
        messages::encode(messages::SESSION_FORMAT, &Presence { state, since })
    }

    fn run(mut self) {
        loop {
            self.set_state(State::Connecting, None, None);
            let error = match self.connect() {
                Ok(()) => {
                    let started = Instant::now();
                    match self.serve() {
                        End::Closed => break,
                        #[cfg(test)]
                        End::Killed => return,
                        End::Lost(error) => {
                            if started.elapsed() >= self.settings.timing.stable
                                && error.class != LiveErrorClass::Flood
                            {
                                self.backoff = self.settings.timing.retry_initial;
                            }
                            error
                        }
                    }
                }
                Err(error) => error,
            };
            self.link = None;
            // The broker discards a clean session's state: the owned
            // retained messages are all published again after a reconnect.
            // Clearings of others' open entries are not sent again either
            // (that session may be back by then): the retained copies and
            // the presence that come after the subscribe tell anew.
            self.inflight
                .retain(|_, m| !m.message.owned && m.clearing.is_none());
            self.subscribing.clear();
            self.tracked.clear();
            self.presence.clear();
            self.held.clear();
            let wait = if error.class.is_final() {
                self.set_state(State::Failed, Some(error), None);
                None
            } else {
                let delay = jitter(self.backoff);
                self.backoff = (self.backoff * 2).min(self.settings.timing.retry_max);
                self.set_state(State::Offline, Some(error), Some(delay));
                Some(delay)
            };
            match self.wait(wait) {
                Wait::Retry => {}
                Wait::Close => break,
                #[cfg(test)]
                Wait::Killed => return,
            }
        }
        self.set_state(State::Closed, None, None);
    }

    /// Waits for the time to connect again (None: until asked), following
    /// the commands meanwhile.
    fn wait(&mut self, timeout: Option<Duration>) -> Wait {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            let command = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Wait::Retry;
                    }
                    self.commands.recv_timeout(deadline - now)
                }
                None => self
                    .commands
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected),
            };
            match command {
                Ok(Command::Retry {
                    password,
                    authorities,
                }) => {
                    self.settings.password = password;
                    self.settings.authorities = authorities;
                    self.backoff = self.settings.timing.retry_initial;
                    return Wait::Retry;
                }
                Ok(Command::Close) | Err(RecvTimeoutError::Disconnected) => return Wait::Close,
                #[cfg(test)]
                Ok(Command::Kill) => return Wait::Killed,
                Ok(command) => self.offline_command(command),
                Err(RecvTimeoutError::Timeout) => return Wait::Retry,
            }
        }
    }

    /// A command while there is no connection: what the session wants
    /// changes, nothing is sent.
    fn offline_command(&mut self, command: Command) {
        match command {
            Command::Subscribe(project) => {
                self.projects.insert(project);
            }
            Command::Unsubscribe(project) => self.forget_project(&project),
            Command::Watch(session) => {
                self.watch(&session);
            }
            Command::Publish(message) => self.queue(message),
            _ => {}
        }
        self.update(|_| {});
    }

    /// Records a retained message this session publishes: its own state is
    /// kept to be published again after a reconnect; anything else
    /// published on a topic replaces what the session kept there.
    fn own(&mut self, message: &Outgoing) {
        if message.owned {
            self.owned
                .insert(message.topic.clone(), message.payload.clone());
        } else if message.retain {
            self.owned.remove(&message.topic);
        }
    }

    /// Keeps a message to publish once connected.
    fn queue(&mut self, message: Outgoing) {
        self.own(&message);
        if message.owned {
            // Published from `owned` after the reconnect.
        } else if message.qos > 0 {
            if self.outbox.len() >= MAX_OUTBOX {
                self.outbox.pop_front();
            }
            self.outbox.push_back(message);
        }
        // QoS 0 while offline: lost, as QoS 0 may be.
    }

    /// Stops following a project: this session's open entries in it are
    /// cleared (published empty), its other owned messages forgotten.
    fn forget_project(&mut self, project: &str) {
        self.projects.remove(project);
        self.tracked.remove(project);
        for entries in self.held.values_mut() {
            entries.retain(|_, event| event["project"] != project);
        }
        self.held.retain(|_, entries| !entries.is_empty());
        let topics: Vec<String> = self.owned.keys().cloned().collect();
        for topic in topics {
            match Topic::parse(&self.settings.prefix, &topic) {
                // Its open entries are cleared on the broker ...
                Some(Topic::Open { project: p, .. }) if p == project => {
                    self.owned.insert(topic, Vec::new());
                }
                // ... its lock summaries stay as the lock refs are, but are
                // not published again.
                Some(t)
                    if t.project() == Some(project)
                        && self.owned.get(&topic).is_some_and(|p| !p.is_empty()) =>
                {
                    self.owned.remove(&topic);
                }
                _ => {}
            }
        }
    }

    /// Follows a session's presence; false when it already was, is this
    /// session or the limit is reached.
    fn watch(&mut self, session: &str) -> bool {
        session != self.settings.session
            && self.watched.len() < MAX_WATCHED_SESSIONS
            && check::is_session(session)
            && self.watched.insert(session.to_owned())
    }

    fn connect(&mut self) -> Result<(), LiveError> {
        let s = &self.settings;
        let timing = s.timing;
        let mut transport = Transport::open(&s.broker, &s.authorities, timing.timeout)?;
        let connect = Packet::Connect(Connect {
            client_id: self.client_id.clone(),
            keep_alive: u16::try_from(timing.keep_alive.as_secs()).unwrap_or(u16::MAX),
            clean_session: true,
            will: Some(Will {
                topic: self.presence_topic(),
                payload: self.presence(PresenceState::Offline),
                qos: 1,
                retain: true,
            }),
            user: s.user.clone(),
            password: s.password.as_ref().map(|p| p.as_bytes().to_vec()),
        });
        let bytes = connect
            .encode()
            .map_err(|e| LiveError::invalid(e.to_string()))?;
        transport
            .write_all(&bytes)
            .map_err(|e| io_error("cannot send to the broker", &e))?;
        let mut decoder = Decoder::new(MAX_PACKET);
        let deadline = Instant::now() + timing.timeout;
        let packet = read_packet(&mut transport, &mut decoder, deadline)?;
        match packet {
            Packet::ConnAck { code: 0, .. } => {}
            Packet::ConnAck { code, .. } => return Err(refused(code)),
            _ => {
                return Err(LiveError::new(
                    LiveErrorClass::Protocol,
                    "the broker did not answer the connection as MQTT 3.1.1 does",
                ));
            }
        }
        transport
            .set_read_timeout(timing.poll)
            .map_err(|e| io_error("cannot set up the connection", &e))?;
        let now = Instant::now();
        self.link = Some(Link {
            transport,
            decoder,
            last_sent: now,
            ping_sent: None,
            rate: RateLimit::new(now),
        });
        Ok(())
    }

    fn send(&mut self, packet: &Packet) -> Result<(), LiveError> {
        let bytes = packet.encode().map_err(|e| {
            LiveError::new(
                LiveErrorClass::Internal,
                format!("cannot encode a packet: {e}"),
            )
        })?;
        let link = self.link.as_mut().expect("connected");
        link.transport
            .write_all(&bytes)
            .map_err(|e| io_error("cannot send to the broker", &e))?;
        link.last_sent = Instant::now();
        Ok(())
    }

    /// A packet identifier not in use.
    fn packet_id(&mut self) -> u16 {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.checked_add(1).unwrap_or(1);
            if !self.inflight.contains_key(&id) && !self.subscribing.contains_key(&id) {
                return id;
            }
        }
    }

    /// Sends a message; returns its packet identifier (0 with QoS 0). `id`:
    /// sent again under that identifier.
    fn publish(&mut self, message: Outgoing, dup: bool, id: Option<u16>) -> Result<u16, LiveError> {
        if message.owned && id.is_none() {
            // A newer state of the topic: the older one is not sent again,
            // so that it cannot arrive after this one.
            self.inflight
                .retain(|_, m| !(m.message.owned && m.message.topic == message.topic));
        }
        let id = match (message.qos, id) {
            (0, _) => 0,
            (_, Some(id)) => id,
            (_, None) => self.packet_id(),
        };
        self.send(&Packet::Publish(Publish {
            topic: message.topic.clone(),
            payload: message.payload.clone(),
            qos: message.qos,
            retain: message.retain,
            dup,
            id,
        }))?;
        if message.qos > 0 {
            // Sent again: still the clearing it was.
            let clearing = self.inflight.remove(&id).and_then(|m| m.clearing);
            self.inflight.insert(
                id,
                InFlight {
                    message,
                    sent_at: Instant::now(),
                    clearing,
                },
            );
        }
        Ok(id)
    }

    fn subscribe(
        &mut self,
        filters: Vec<String>,
        subscription: Subscription,
    ) -> Result<(), LiveError> {
        let id = self.packet_id();
        self.subscribing.insert(id, subscription);
        self.send(&Packet::Subscribe {
            id,
            filters: filters.into_iter().map(|f| (f, 1)).collect(),
        })
    }

    fn subscribe_sessions(&mut self, sessions: Vec<String>) -> Result<(), LiveError> {
        for chunk in sessions.chunks(FILTERS_PER_SUBSCRIBE) {
            let filters = chunk
                .iter()
                .map(|session| {
                    Topic::Session {
                        session: session.clone(),
                    }
                    .name(&self.settings.prefix)
                })
                .collect();
            self.subscribe(filters, Subscription::Sessions(chunk.to_vec()))?;
        }
        Ok(())
    }

    /// Sends what waits in the outbox, as far as the in-flight limit lets.
    fn flush(&mut self) -> Result<(), LiveError> {
        while self.inflight.len() < MAX_INFLIGHT {
            let Some(message) = self.outbox.pop_front() else {
                break;
            };
            self.publish(message, false, None)?;
        }
        Ok(())
    }

    /// The start of a connected period: subscriptions, presence, the owned
    /// messages and those waiting.
    fn start(&mut self) -> Result<(), LiveError> {
        for project in self.projects.clone() {
            let filter = messages::project_filter(&self.settings.prefix, &project);
            self.subscribe(vec![filter], Subscription::Project(project))?;
        }
        let watched: Vec<String> = self.watched.iter().cloned().collect();
        self.subscribe_sessions(watched)?;
        let unacknowledged: Vec<(u16, Outgoing)> = self
            .inflight
            .iter()
            .map(|(id, m)| (*id, m.message.clone()))
            .collect();
        // Owned: not sent again after a reconnect, which publishes it anew.
        let online = Outgoing {
            topic: self.presence_topic(),
            payload: self.presence(PresenceState::Online),
            qos: 1,
            retain: true,
            owned: true,
        };
        self.publish(online, false, None)?;
        for (id, message) in unacknowledged {
            self.publish(message, true, Some(id))?;
        }
        for (topic, payload) in self.owned.clone() {
            self.publish(
                Outgoing {
                    topic,
                    payload,
                    qos: 1,
                    retain: true,
                    owned: true,
                },
                false,
                None,
            )?;
        }
        self.flush()
    }

    /// A connected period, until it is closed or lost.
    fn serve(&mut self) -> End {
        self.set_state(State::Connected, None, None);
        if let Err(error) = self.start() {
            return End::Lost(error);
        }
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            loop {
                match self.commands.try_recv() {
                    Ok(Command::Close) | Err(TryRecvError::Disconnected) => {
                        self.close();
                        return End::Closed;
                    }
                    #[cfg(test)]
                    Ok(Command::Kill) => {
                        if let Some(link) = self.link.as_mut() {
                            link.transport.kill();
                        }
                        return End::Killed;
                    }
                    Ok(command) => {
                        if let Err(error) = self.online_command(command) {
                            return End::Lost(error);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Err(error) = self.receive(&mut buffer) {
                return End::Lost(error);
            }
            if let Err(error) = self.timers() {
                return End::Lost(error);
            }
            self.update(|_| {});
        }
    }

    fn online_command(&mut self, command: Command) -> Result<(), LiveError> {
        match command {
            Command::Subscribe(project) => {
                if self.projects.insert(project.clone()) {
                    let filter = messages::project_filter(&self.settings.prefix, &project);
                    self.subscribe(vec![filter], Subscription::Project(project))?;
                }
            }
            Command::Unsubscribe(project) => {
                let was = self.projects.contains(&project);
                self.forget_project(&project);
                self.publish_cleared()?;
                if was {
                    let id = self.packet_id();
                    let filter = messages::project_filter(&self.settings.prefix, &project);
                    self.send(&Packet::Unsubscribe {
                        id,
                        filters: vec![filter],
                    })?;
                }
            }
            Command::Watch(session) => {
                if self.watch(&session) {
                    self.subscribe_sessions(vec![session])?;
                }
            }
            Command::Publish(message) => {
                if message.owned || message.qos == 0 {
                    self.own(&message);
                    self.publish(message, false, None)?;
                } else {
                    self.queue(message);
                    self.flush()?;
                }
            }
            Command::Retry {
                password,
                authorities,
            } => {
                // Connected already: used for the next connection.
                self.settings.password = password;
                self.settings.authorities = authorities;
            }
            Command::Close => {}
            #[cfg(test)]
            Command::Kill => {}
        }
        Ok(())
    }

    /// Publishes the owned topics cleared (empty) and not sent yet.
    fn publish_cleared(&mut self) -> Result<(), LiveError> {
        let sending: BTreeSet<String> = self
            .inflight
            .values()
            .filter(|m| m.message.owned && m.message.payload.is_empty())
            .map(|m| m.message.topic.clone())
            .collect();
        let cleared: Vec<String> = self
            .owned
            .iter()
            .filter(|(topic, payload)| payload.is_empty() && !sending.contains(*topic))
            .map(|(topic, _)| topic.clone())
            .collect();
        for topic in cleared {
            self.publish(
                Outgoing {
                    topic,
                    payload: Vec::new(),
                    qos: 1,
                    retain: true,
                    owned: true,
                },
                false,
                None,
            )?;
        }
        Ok(())
    }

    /// A clean end: this session's open entries and presence cleared, then
    /// DISCONNECT (the broker does not publish the will).
    fn close(&mut self) {
        let prefix = self.settings.prefix.clone();
        let open: Vec<String> = self
            .owned
            .iter()
            .filter(|(topic, payload)| {
                !payload.is_empty()
                    && matches!(Topic::parse(&prefix, topic), Some(Topic::Open { .. }))
            })
            .map(|(topic, _)| topic.clone())
            .collect();
        for topic in open {
            self.owned.insert(topic, Vec::new());
        }
        let _ = self.publish_cleared();
        let presence = Outgoing {
            topic: self.presence_topic(),
            payload: Vec::new(),
            qos: 1,
            retain: true,
            owned: true,
        };
        let _ = self.publish(presence, false, None);
        let _ = self.send(&Packet::Disconnect);
        if let Some(link) = self.link.as_mut() {
            link.transport.close();
        }
    }

    /// Reads what the broker sent (waiting at most the poll interval) and
    /// handles each packet.
    fn receive(&mut self, buffer: &mut [u8]) -> Result<(), LiveError> {
        let link = self.link.as_mut().expect("connected");
        match link.transport.read(buffer) {
            Ok(0) => {
                return Err(LiveError::new(
                    LiveErrorClass::Network,
                    "the broker closed the connection",
                ));
            }
            Ok(n) => link.decoder.push(&buffer[..n]),
            Err(e) if is_timeout(&e) => return Ok(()),
            Err(e) => return Err(io_error("the connection to the broker broke", &e)),
        }
        loop {
            let link = self.link.as_mut().expect("connected");
            let packet = match link.decoder.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(()),
                Err(e) => {
                    return Err(LiveError::new(
                        LiveErrorClass::Protocol,
                        format!("the broker sent {e}"),
                    ));
                }
            };
            self.packet(packet)?;
        }
    }

    fn packet(&mut self, packet: Packet) -> Result<(), LiveError> {
        if let Some(link) = self.link.as_mut() {
            link.ping_sent = None;
        }
        match packet {
            Packet::Publish(publish) => {
                let link = self.link.as_mut().expect("connected");
                if link.rate.exceeded(Instant::now(), publish.retain) {
                    let (limit, what) = if publish.retain {
                        (MAX_RETAINED_PER_SECOND, "retained messages")
                    } else {
                        (MAX_MESSAGES_PER_SECOND, "messages")
                    };
                    return Err(LiveError::new(
                        LiveErrorClass::Flood,
                        format!("the broker sent more than {limit} {what} in a second"),
                    ));
                }
                if publish.qos == 1 {
                    self.send(&Packet::PubAck(publish.id))?;
                }
                self.message(publish)?;
            }
            Packet::PubAck(id) => {
                if let Some(done) = self.inflight.remove(&id) {
                    // The broker has the entry cleared: a session that
                    // subscribes from now on does not get it.
                    if let Some(Some(event)) = done.clearing {
                        self.events.push(event);
                    }
                    let m = done.message;
                    if m.owned
                        && m.payload.is_empty()
                        && self.owned.get(&m.topic).is_some_and(Vec::is_empty)
                    {
                        self.owned.remove(&m.topic);
                    }
                }
                self.flush()?;
            }
            Packet::SubAck { id, codes } => match self.subscribing.remove(&id) {
                Some(Subscription::Project(project)) if self.projects.contains(&project) => {
                    let refused = codes.contains(&0x80);
                    let error = refused.then(|| {
                        LiveError::new(
                            LiveErrorClass::Refused,
                            "the broker refused the subscription to the project's topics",
                        )
                    });
                    self.events.push(json!({
                        "type": "subscribed",
                        "connection": self.settings.id,
                        "project": project,
                        "error": error_json(error.as_ref()),
                    }));
                }
                Some(Subscription::Sessions(sessions)) => {
                    // A presence this session may not read never comes: the
                    // entries held for it are given as they are.
                    for (session, code) in sessions.iter().zip(&codes) {
                        if *code == 0x80 {
                            self.presence.insert(session.clone(), Seen::Refused);
                            self.release(session);
                        }
                    }
                }
                _ => {}
            },
            Packet::UnsubAck(_) | Packet::PingResp => {}
            _ => {
                return Err(LiveError::new(
                    LiveErrorClass::Protocol,
                    "the broker sent a packet a client never gets",
                ));
            }
        }
        Ok(())
    }

    fn dropped(&mut self, reason: &str, topic: &str) {
        let mut total = 0;
        self.update(|status| {
            status.dropped += 1;
            *status.dropped_reasons.entry(reason.to_owned()).or_default() += 1;
            total = status.dropped;
        });
        let mut topic = check::clean_text(topic);
        if topic.chars().count() > 200 {
            topic = topic.chars().take(200).collect();
        }
        self.events.push_dropped(json!({
            "type": "dropped",
            "connection": self.settings.id,
            "reason": reason,
            "topic": topic,
            "count": total,
        }));
    }

    /// A message from the broker: checked, tracked, an event.
    fn message(&mut self, publish: Publish) -> Result<(), LiveError> {
        self.update(|status| status.received += 1);
        let Some(topic) = Topic::parse(&self.settings.prefix, &publish.topic) else {
            self.dropped("a topic that is not one of Mitcad's", &publish.topic);
            return Ok(());
        };
        if let Some(project) = topic.project()
            && !self.projects.contains(project)
        {
            self.dropped("a topic of a project not subscribed", &publish.topic);
            return Ok(());
        }
        if let Topic::Session { session } = &topic
            && !self.watched.contains(session)
        {
            self.dropped("the presence of a session not followed", &publish.topic);
            return Ok(());
        }
        // Requests and versions are news: a retained copy would come again
        // with every subscribe.
        if publish.retain && matches!(topic, Topic::Requests { .. } | Topic::Versions { .. }) {
            self.dropped(
                "a retained copy of a message never retained",
                &publish.topic,
            );
            return Ok(());
        }
        let message = match messages::read(&topic, &publish.payload) {
            Ok(message) => message,
            Err(reason) => {
                self.dropped(reason, &publish.topic);
                return Ok(());
            }
        };
        // Whether an open entry there was given as an event (its closing is
        // given only then).
        let given = match &topic {
            Topic::Open {
                project,
                file,
                session,
            } => {
                self.tracked
                    .get(project)
                    .is_some_and(|t| t.has_open(file, session))
                    && !self
                        .held
                        .get(session)
                        .is_some_and(|held| held.contains_key(&publish.topic))
            }
            _ => false,
        };
        let tracking = match (&topic, &message) {
            (Topic::Lock { project, file }, Message::Lock(summary)) => {
                Some((project, file, None, summary.is_some()))
            }
            (
                Topic::Open {
                    project,
                    file,
                    session,
                },
                Message::Open(entry),
            ) => Some((project, file, Some(session.as_str()), entry.is_some())),
            _ => None,
        };
        if let Some((project, file, session, present)) = tracking {
            let tracked = self.tracked.entry(project.clone()).or_default();
            if let Err(reason) = tracked.set(file, session, present) {
                self.dropped(reason, &publish.topic);
                return Ok(());
            }
        }
        // Another session's lock where this one kept its own (taken over):
        // this session's summary is not published again.
        if let Message::Lock(Some(summary)) = &message
            && summary.session != self.settings.session
        {
            self.owned.remove(&publish.topic);
        }
        // The sessions it names, to learn when they go offline.
        let mut sessions = Vec::new();
        match &message {
            Message::Lock(Some(summary)) => sessions.push(summary.session.clone()),
            Message::Request(request) => {
                sessions.push(request.session.clone());
                sessions.extend(request.for_session.clone());
            }
            Message::Version(version) => sessions.push(version.session.clone()),
            _ => {}
        }
        if let Topic::Open { session, .. } = &topic {
            sessions.push(session.clone());
        }
        let new: Vec<String> = sessions.into_iter().filter(|s| self.watch(s)).collect();
        if !new.is_empty() {
            self.subscribe_sessions(new)?;
        }
        let event = self.event(&topic, &message, publish.retain);
        match (&topic, &message) {
            (
                Topic::Open {
                    project,
                    file,
                    session,
                },
                Message::Open(entry),
            ) if *session != self.settings.session => {
                let open = OpenTopic {
                    project,
                    file,
                    session,
                    topic: &publish.topic,
                };
                self.open_entry(&open, entry.is_some(), given, event)?;
            }
            (Topic::Session { session }, Message::Session(presence)) => {
                let seen = match presence {
                    Some(p) if p.state == PresenceState::Online => Seen::Online,
                    Some(_) => Seen::Offline,
                    None => Seen::Left,
                };
                self.presence.insert(session.clone(), seen);
                self.events.push(event);
                match seen {
                    Seen::Online => self.release(session),
                    Seen::Offline => self.clear_session(session)?,
                    Seen::Left | Seen::Refused => {}
                }
            }
            _ => self.events.push(event),
        }
        Ok(())
    }

    /// Another session's open entry (`present`) or its closing. An entry
    /// is given as an event when that session is online; while its presence
    /// is not known yet (the retained copies of a subscribe come before it)
    /// or it left, the entry is held until it is seen online; when it is
    /// offline (its will), the entry is not given and is cleared on the
    /// broker. A closing is given when the entry was (`given`).
    fn open_entry(
        &mut self,
        open: &OpenTopic,
        present: bool,
        given: bool,
        event: Value,
    ) -> Result<(), LiveError> {
        if let Some(held) = self.held.get_mut(open.session) {
            held.remove(open.topic);
            if held.is_empty() {
                self.held.remove(open.session);
            }
        }
        if !present {
            if given {
                self.events.push(event);
            }
            return Ok(());
        }
        match self.presence.get(open.session) {
            Some(Seen::Online | Seen::Refused) => self.events.push(event),
            Some(Seen::Offline) => self.clear_entry(open.project, open.file, open.session, None)?,
            _ if self.watched.contains(open.session) => {
                self.held
                    .entry(open.session.to_owned())
                    .or_default()
                    .insert(open.topic.to_owned(), event);
            }
            // Not followed (the limit of followed sessions): its presence
            // never comes, the entry is given as it is.
            _ => self.events.push(event),
        }
        Ok(())
    }

    /// Gives the entries held for a session (seen online, or its presence
    /// cannot be known).
    fn release(&mut self, session: &str) {
        for event in self.held.remove(session).unwrap_or_default().into_values() {
            self.events.push(event);
        }
    }

    /// Clears the open entries of a session that went offline (its will),
    /// in every project followed. Those given as events become `open`
    /// events with `cleared` once the broker has acknowledged the clearing;
    /// those held are cleared without one.
    fn clear_session(&mut self, session: &str) -> Result<(), LiveError> {
        let held = self.held.remove(session).unwrap_or_default();
        let mut cleared = Vec::new();
        for (project, tracked) in &self.tracked {
            for file in tracked.opened_by(session) {
                cleared.push((project.clone(), file));
            }
        }
        for (project, file) in cleared {
            let topic = open_topic(&self.settings.prefix, &project, &file, session);
            let event = (!held.contains_key(&topic)).then(|| {
                json!({
                    "type": "open",
                    "connection": self.settings.id,
                    "project": project,
                    "file": file,
                    "session": session,
                    "entry": null,
                    "retained": false,
                    "own": false,
                    "cleared": true,
                })
            });
            self.clear_entry(&project, &file, session, event)?;
        }
        Ok(())
    }

    /// Clears another session's open entry on the broker (published empty
    /// and retained); `event` is given when the broker has acknowledged it,
    /// so that a session subscribing after it does not get the entry.
    fn clear_entry(
        &mut self,
        project: &str,
        file: &str,
        session: &str,
        event: Option<Value>,
    ) -> Result<(), LiveError> {
        if let Some(tracked) = self.tracked.get_mut(project) {
            let _ = tracked.set(file, Some(session), false);
        }
        let id = self.publish(
            Outgoing {
                topic: open_topic(&self.settings.prefix, project, file, session),
                payload: Vec::new(),
                qos: 1,
                retain: true,
                owned: false,
            },
            false,
            None,
        )?;
        if let Some(sent) = self.inflight.get_mut(&id) {
            sent.clearing = Some(event);
        }
        Ok(())
    }

    fn event(&self, topic: &Topic, message: &Message, retained: bool) -> Value {
        let own = |session: &str| session == self.settings.session;
        let mut event = match (topic, message) {
            (Topic::Lock { project, file }, Message::Lock(summary)) => json!({
                "type": "lock",
                "project": project,
                "file": file,
                "lock": summary,
                "own": summary.as_ref().is_some_and(|s| own(&s.session)),
            }),
            (Topic::Requests { project, file }, Message::Request(request)) => {
                let mut event = json!({
                    "type": "request",
                    "project": project,
                    "file": file,
                    "own": own(&request.session),
                });
                if let (Value::Object(event), Ok(Value::Object(fields))) =
                    (&mut event, serde_json::to_value(request))
                {
                    event.extend(fields);
                }
                event
            }
            (Topic::Versions { project }, Message::Version(version)) => {
                let mut event = json!({
                    "type": "version",
                    "project": project,
                    "own": own(&version.session),
                });
                if let (Value::Object(event), Ok(Value::Object(fields))) =
                    (&mut event, serde_json::to_value(version))
                {
                    event.extend(fields);
                }
                event
            }
            (
                Topic::Open {
                    project,
                    file,
                    session,
                },
                Message::Open(entry),
            ) => json!({
                "type": "open",
                "project": project,
                "file": file,
                "session": session,
                "entry": entry,
                "own": own(session),
            }),
            (Topic::Session { session }, Message::Session(presence)) => json!({
                "type": "session",
                "session": session,
                "state": match presence {
                    Some(p) if p.state == PresenceState::Online => "online",
                    Some(_) => "offline",
                    None => "left",
                },
                "since": presence.as_ref().and_then(|p| p.since.clone()),
            }),
            _ => json!({"type": "unknown"}),
        };
        event["connection"] = json!(self.settings.id);
        event["retained"] = json!(retained);
        event
    }

    /// Keep-alive and resending.
    fn timers(&mut self) -> Result<(), LiveError> {
        let timing = self.settings.timing;
        let now = Instant::now();
        let link = self.link.as_ref().expect("connected");
        if let Some(sent) = link.ping_sent
            && now.duration_since(sent) >= timing.keep_alive
        {
            return Err(LiveError::new(
                LiveErrorClass::TimedOut,
                "the broker stopped answering",
            ));
        }
        if link.ping_sent.is_none()
            && now.duration_since(link.last_sent) >= timing.keep_alive * 3 / 4
        {
            self.send(&Packet::PingReq)?;
            self.link.as_mut().expect("connected").ping_sent = Some(now);
        }
        let late: Vec<(u16, Outgoing)> = self
            .inflight
            .iter()
            .filter(|(_, m)| now.duration_since(m.sent_at) >= timing.resend)
            .map(|(id, m)| (*id, m.message.clone()))
            .collect();
        for (id, message) in late {
            self.publish(message, true, Some(id))?;
        }
        Ok(())
    }
}

/// An open entry's topic, by its parts and by its name.
struct OpenTopic<'a> {
    project: &'a str,
    file: &'a str,
    session: &'a str,
    topic: &'a str,
}

/// The name of a session's open entry topic.
fn open_topic(prefix: &str, project: &str, file: &str, session: &str) -> String {
    Topic::Open {
        project: project.to_owned(),
        file: file.to_owned(),
        session: session.to_owned(),
    }
    .name(prefix)
}

enum Wait {
    Retry,
    Close,
    #[cfg(test)]
    Killed,
}

/// A delay of 80 % to 120 % of `delay`.
fn jitter(delay: Duration) -> Duration {
    let [a, b] = random_bytes::<2>();
    let r = f64::from(u16::from_le_bytes([a, b])) / f64::from(u16::MAX);
    delay.mul_f64(0.8 + 0.4 * r)
}

/// The failure of a refused connection (CONNACK's return code).
pub fn refused(code: u8) -> LiveError {
    match code {
        4 => LiveError::new(
            LiveErrorClass::AuthFailed,
            "the broker refused the user name or password",
        ),
        5 => LiveError::new(
            LiveErrorClass::AuthFailed,
            "the broker refused the connection: not authorised",
        ),
        3 => LiveError::new(
            LiveErrorClass::Network,
            "the broker is not available at the moment",
        ),
        1 => LiveError::new(
            LiveErrorClass::Refused,
            "the broker does not speak MQTT 3.1.1",
        ),
        _ => LiveError::new(
            LiveErrorClass::Refused,
            "the broker refused the client identifier",
        ),
    }
}

/// Reads one packet before `deadline` (the connection's set-up).
pub fn read_packet(
    transport: &mut Transport,
    decoder: &mut Decoder,
    deadline: Instant,
) -> Result<Packet, LiveError> {
    let mut buffer = [0u8; 4096];
    loop {
        match decoder.next_packet() {
            Ok(Some(packet)) => return Ok(packet),
            Ok(None) => {}
            Err(e) => {
                return Err(LiveError::new(
                    LiveErrorClass::Protocol,
                    format!("the broker sent {e}"),
                ));
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(LiveError::new(
                LiveErrorClass::TimedOut,
                "the broker did not answer in time",
            ));
        }
        let _ = transport.set_read_timeout((deadline - now).max(Duration::from_millis(1)));
        match transport.read(&mut buffer) {
            Ok(0) => {
                return Err(LiveError::new(
                    LiveErrorClass::Network,
                    "the broker closed the connection",
                ));
            }
            Ok(n) => decoder.push(&buffer[..n]),
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(io_error("the connection to the broker broke", &e)),
        }
    }
}
