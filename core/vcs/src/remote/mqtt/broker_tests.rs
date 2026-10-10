// SPDX-License-Identifier: MIT
//! Live updates against a broker: every scenario runs against the fake
//! broker of the tests (`fake_broker.rs`), and against mosquitto when it is
//! installed (a test tool the dev-env setup scripts install; it is not
//! shipped). mosquitto is found on `PATH`, in the usual `sbin` folders and
//! in `C:\Program Files\mosquitto`, or at `MITCAD_MOSQUITTO`; each test
//! starts its own on free ports in a folder of its own. The TLS tests make
//! a certificate authority and a server certificate for the test with
//! `openssl` (skipped without it); no key is kept in the repository.

use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::fake_broker::FakeBroker;
use super::messages::{self, Topic};
use super::packet::{Connect, Decoder, Packet, Publish};
use super::{HubOptions, LiveHub, MAX_PACKET, Timing};

const P1: &str = "1111111111111111111111111111111111111111";
const P2: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const SA: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const SB: &str = "bbbbbbbb-0000-4000-8000-000000000002";
const SC: &str = "cccccccc-0000-4000-8000-000000000003";
const WAIT: Duration = Duration::from_secs(15);

fn options() -> HubOptions {
    HubOptions {
        timing: Timing {
            keep_alive: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            poll: Duration::from_millis(20),
            retry_initial: Duration::from_millis(100),
            retry_max: Duration::from_secs(1),
            resend: Duration::from_secs(2),
            stable: Duration::from_secs(60),
        },
    }
}

/// A broker the scenarios run against.
trait Broker {
    /// The plain listener (raw test clients use it too).
    fn port(&self) -> u16;
    /// Drops every connection and forgets the retained messages.
    fn restart(&mut self);
}

impl Broker for FakeBroker {
    fn port(&self) -> u16 {
        FakeBroker::port(self)
    }

    fn restart(&mut self) {
        FakeBroker::restart(self);
    }
}

/// A hub and the events it gave.
struct Session {
    hub: LiveHub,
    session: &'static str,
    pending: VecDeque<Value>,
    seen: Vec<Value>,
}

impl Session {
    fn new(session: &'static str) -> Self {
        Self {
            hub: LiveHub::new(options()),
            session,
            pending: VecDeque::new(),
            seen: Vec::new(),
        }
    }

    fn command(&self, command: Value) -> Value {
        let answer = self
            .hub
            .command(&command.to_string())
            .unwrap_or_else(|e| panic!("{command}: {e}"));
        serde_json::from_str(&answer).unwrap()
    }

    /// Subscribes to a project on the broker's plain listener; returns the
    /// connection.
    fn subscribe(&self, port: u16, project: &str) -> String {
        let answer = self.command(
            json!({"cmd": "subscribe", "broker": format!("mqtt://127.0.0.1:{port}"),
                                         "session": self.session, "project": project}),
        );
        answer["connection"].as_str().unwrap().to_owned()
    }

    fn publish(&self, connection: &str, project: &str, message: Value) -> Value {
        self.command(
            json!({"cmd": "publish", "connection": connection, "project": project,
                            "message": message}),
        )
    }

    fn fetch(&mut self, timeout: Duration) {
        let answer: Value = serde_json::from_str(&self.hub.events(timeout)).unwrap();
        for event in answer["events"].as_array().unwrap() {
            self.seen.push(event.clone());
            self.pending.push_back(event.clone());
        }
    }

    /// The first event `matches` takes, waiting for it; the others stay for
    /// later waits (retained messages come in any order).
    fn wait(&mut self, what: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(at) = self.pending.iter().position(&matches) {
                return self.pending.remove(at).unwrap();
            }
            assert!(
                Instant::now() < deadline,
                "{}: no {what} in {:#?}",
                self.session,
                self.seen
            );
            self.fetch(Duration::from_millis(200));
        }
    }

    /// Forgets the events so far (before a change of state is waited for).
    fn clear(&mut self) {
        self.fetch(Duration::ZERO);
        self.pending.clear();
    }

    fn wait_state(&mut self, state: &str) -> Value {
        self.wait(&format!("state {state}"), |e| {
            e["type"] == "state" && e["state"] == state
        })
    }

    fn wait_subscribed(&mut self, project: &str) {
        self.wait("subscribed", |e| {
            e["type"] == "subscribed" && e["project"] == project && e["error"].is_null()
        });
    }

    fn status(&self) -> Value {
        self.command(json!({"cmd": "status"}))
    }
}

/// A plain MQTT client for the tests, made of the same packets.
struct Raw {
    stream: TcpStream,
    decoder: Decoder,
}

impl Raw {
    fn connect(port: u16) -> Raw {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut raw = Raw {
            stream,
            decoder: Decoder::new(MAX_PACKET * 4),
        };
        raw.send(&Packet::Connect(Connect {
            client_id: format!(
                "raw{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ),
            keep_alive: 60,
            clean_session: true,
            will: None,
            user: None,
            password: None,
        }));
        assert!(matches!(
            raw.next(WAIT),
            Some(Packet::ConnAck { code: 0, .. })
        ));
        raw
    }

    fn send(&mut self, packet: &Packet) {
        self.stream.write_all(&packet.encode().unwrap()).unwrap();
    }

    fn publish(&mut self, topic: &str, payload: &[u8], retain: bool) {
        self.send(&Packet::Publish(Publish {
            topic: topic.to_owned(),
            payload: payload.to_vec(),
            qos: 0,
            retain,
            dup: false,
            id: 0,
        }));
    }

    fn next(&mut self, timeout: Duration) -> Option<Packet> {
        let deadline = Instant::now() + timeout;
        let mut buffer = [0u8; 4096];
        loop {
            if let Some(packet) = self.decoder.next_packet().unwrap() {
                return Some(packet);
            }
            if Instant::now() >= deadline {
                return None;
            }
            match self.stream.read(&mut buffer) {
                Ok(0) => return None,
                Ok(n) => self.decoder.push(&buffer[..n]),
                Err(_) => {}
            }
        }
    }

    /// The messages that come until the broker answers a ping (it handles
    /// a client's packets in order: what it sent for those before the ping
    /// has come).
    fn until_ping(&mut self) -> Vec<Publish> {
        self.send(&Packet::PingReq);
        let mut messages = Vec::new();
        loop {
            match self.next(WAIT) {
                Some(Packet::PingResp) => return messages,
                Some(Packet::Publish(publish)) => messages.push(publish),
                other => panic!("no ping answer: {other:?}"),
            }
        }
    }

    /// Subscribes to `filter` (QoS 0); returns the retained copies.
    fn subscribe(&mut self, filter: &str) -> Vec<Publish> {
        self.send(&Packet::Subscribe {
            id: 1,
            filters: vec![(filter.to_owned(), 0)],
        });
        let mut messages = Vec::new();
        loop {
            match self.next(WAIT) {
                Some(Packet::SubAck { .. }) => break,
                Some(Packet::Publish(publish)) => messages.push(publish),
                other => panic!("no SUBACK: {other:?}"),
            }
        }
        messages.extend(self.until_ping());
        messages
    }

    /// The topics of the retained messages the broker has on `filter`.
    fn retained(port: u16, filter: &str) -> Vec<String> {
        let mut raw = Raw::connect(port);
        let topics = raw.subscribe(filter).into_iter().map(|p| p.topic).collect();
        raw.close();
        topics
    }

    /// Waits for a message on `topic` whose payload `matches`.
    fn wait(&mut self, topic: &str, matches: impl Fn(&Value) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(Packet::Publish(publish)) = self.next(Duration::from_millis(100))
                && publish.topic == topic
                && serde_json::from_slice::<Value>(&publish.payload)
                    .is_ok_and(|payload| matches(&payload))
            {
                return;
            }
            assert!(Instant::now() < deadline, "nothing on {topic}");
        }
    }

    /// Ends with a DISCONNECT, after a round trip that makes sure the
    /// broker has handled what was sent.
    fn close(mut self) {
        self.send(&Packet::PingReq);
        while !matches!(self.next(WAIT), Some(Packet::PingResp) | None) {}
        self.send(&Packet::Disconnect);
    }
}

fn file(path: &str) -> String {
    messages::file_id(path)
}

fn lock(name: &str) -> Value {
    json!({"owner": {"name": name}, "taken_at": "2026-10-08T13:40:00Z",
           "active_at": "2026-10-08T13:41:00Z", "idle_minutes": 10, "poll_seconds": 10})
}

/// Two sessions on one project: locks, who has a design open, versions,
/// requests; a third that comes later gets the retained state; a clean
/// close leaves no trace.
fn messages_between_sessions(broker: &mut dyn Broker) {
    let port = broker.port();
    let mut a = Session::new(SA);
    let mut b = Session::new(SB);
    let ca = a.subscribe(port, P1);
    let cb = b.subscribe(port, P1);
    a.wait_subscribed(P1);
    b.wait_subscribed(P1);
    let status = a.status();
    assert_eq!(status["connections"][0]["state"], "connected", "{status}");
    assert_eq!(status["connections"][0]["tls"], false);

    let answer = a.publish(
        &ca,
        P1,
        json!({"type": "lock", "path": "dir/part.mitcad",
                                           "lock": lock("Alex")}),
    );
    assert_eq!(answer["retain"], true);
    a.publish(
        &ca,
        P1,
        json!({"type": "open", "path": "dir/part.mitcad", "name": "Alex",
                              "mode": "editing"}),
    );
    a.publish(
        &ca,
        P1,
        json!({"type": "version", "branch": "main", "commit": P2, "name": "Alex"}),
    );
    let lock_event = b.wait("lock", |e| e["type"] == "lock" && !e["lock"].is_null());
    assert_eq!(lock_event["lock"]["owner"]["name"], "Alex");
    assert_eq!(lock_event["lock"]["session"], SA);
    assert_eq!(lock_event["file"], file("dir/part.mitcad"));
    assert_eq!(lock_event["own"], false);
    let open = b.wait("open", |e| e["type"] == "open" && !e["entry"].is_null());
    assert_eq!(open["entry"]["mode"], "editing");
    assert_eq!(open["session"], SA);
    let version = b.wait("version", |e| e["type"] == "version");
    assert_eq!(
        (&version["branch"], &version["commit"]),
        (&json!("main"), &json!(P2))
    );
    // B follows A's presence since A's lock named it.
    let presence = b.wait("presence", |e| e["type"] == "session" && e["session"] == SA);
    assert_eq!(presence["state"], "online");
    // A sees its own messages, marked as its own.
    let own = a.wait("own lock", |e| e["type"] == "lock");
    assert_eq!(own["own"], true);

    b.publish(
        &cb,
        P1,
        json!({"type": "request", "path": "dir/part.mitcad", "name": "Sam",
                              "message": "May I?\u{202e}"}),
    );
    let request = a.wait("request", |e| {
        e["type"] == "request" && e["kind"] == "request"
    });
    assert_eq!(request["session"], SB);
    assert_eq!(request["message"], "May I?", "cleaned");
    a.publish(
        &ca,
        P1,
        json!({"type": "receipt", "path": "dir/part.mitcad", "name": "Alex",
                              "for": SB}),
    );
    let receipt = b.wait("receipt", |e| {
        e["type"] == "request" && e["kind"] == "receipt"
    });
    assert_eq!(receipt["for"], SB);

    // A third session later gets what is retained.
    let mut c = Session::new(SC);
    c.subscribe(port, P1);
    let lock_event = c.wait("retained lock", |e| e["type"] == "lock");
    assert_eq!(lock_event["retained"], true);
    let open = c.wait("retained open", |e| e["type"] == "open");
    assert_eq!(open["entry"]["name"], "Alex");

    // Released and closed.
    a.publish(
        &ca,
        P1,
        json!({"type": "lock", "path": "dir/part.mitcad", "lock": null}),
    );
    b.wait("lock released", |e| {
        e["type"] == "lock" && e["lock"].is_null()
    });
    let closed = a.command(json!({"cmd": "unsubscribe", "connection": ca, "project": P1}));
    assert_eq!(closed["closed"], true);
    b.wait("open entry cleared", |e| {
        e["type"] == "open" && e["entry"].is_null()
    });
    b.wait("left", |e| {
        e["type"] == "session" && e["session"] == SA && e["state"] == "left"
    });
    a.wait_state("closed");
    let status = a.status();
    assert_eq!(status["connections"], json!([]));
    // Quitting: the connections disconnect before the answer.
    c.command(json!({"cmd": "watch", "connection": "c1", "session": SB}));
    c.wait("B online", |e| {
        e["type"] == "session" && e["session"] == SB && e["state"] == "online"
    });
    let closed = b.command(json!({"cmd": "close", "wait_ms": 5000}));
    assert_eq!(closed["finished"], true, "{closed}");
    c.wait("left", |e| {
        e["type"] == "session" && e["session"] == SB && e["state"] == "left"
    });
}

/// A connection that dies: the broker publishes its will, the others see
/// the session offline and clear the designs it had open.
fn will_on_a_killed_connection(broker: &mut dyn Broker) {
    let port = broker.port();
    let mut a = Session::new(SA);
    let mut b = Session::new(SB);
    let ca = a.subscribe(port, P1);
    let cb = b.subscribe(port, P1);
    a.wait_subscribed(P1);
    b.wait_subscribed(P1);
    a.publish(
        &ca,
        P1,
        json!({"type": "open", "path": "part.mitcad", "name": "Alex",
                              "mode": "read-only"}),
    );
    b.wait("open", |e| e["type"] == "open" && !e["entry"].is_null());
    b.wait("online", |e| {
        e["type"] == "session" && e["state"] == "online"
    });
    a.hub.kill(&ca);
    let offline = b.wait("offline", |e| e["type"] == "session" && e["session"] == SA);
    assert_eq!(offline["state"], "offline");
    let cleared = b.wait("cleared", |e| e["type"] == "open" && e["entry"].is_null());
    assert_eq!(cleared["session"], SA);
    assert_eq!(cleared["cleared"], true);
    // Given once the broker acknowledged the clearing: it has no entry.
    let open = format!("mitcad/{P1}/open/#");
    assert_eq!(Raw::retained(port, &open), Vec::<String>::new());
    // A newcomer does not see it: what is retained comes before a version
    // published after its subscribe.
    let mut c = Session::new(SC);
    c.subscribe(port, P1);
    c.wait_subscribed(P1);
    b.publish(
        &cb,
        P1,
        json!({"type": "version", "branch": "main", "commit": P2, "name": "Sam"}),
    );
    c.wait("version", |e| e["type"] == "version");
    assert!(
        !c.seen.iter().any(|e| e["type"] == "open"),
        "nothing retained: {:#?}",
        c.seen
    );
    // B's own clearing coming back is not another event.
    b.publish(
        &cb,
        P1,
        json!({"type": "version", "branch": "main", "commit": P1, "name": "Sam"}),
    );
    b.wait("own version", |e| {
        e["type"] == "version" && e["commit"] == P1
    });
    assert!(
        !b.pending.iter().any(|e| e["type"] == "open"),
        "{:#?}",
        b.seen
    );
}

/// A session dies while no other follows it: its open entry stays on the
/// broker with its will. A newcomer holds the entry until it knows the
/// session's presence, never gives it, and clears it on the broker.
fn will_seen_by_a_newcomer(broker: &mut dyn Broker) {
    let port = broker.port();
    let mut a = Session::new(SA);
    let ca = a.subscribe(port, P1);
    a.wait_subscribed(P1);
    a.publish(
        &ca,
        P1,
        json!({"type": "open", "path": "part.mitcad", "name": "Alex",
                              "mode": "editing"}),
    );
    a.wait("own open", |e| e["type"] == "open");
    // The will is on the broker before the newcomer comes.
    let presence = Topic::Session {
        session: SA.to_owned(),
    }
    .name("mitcad");
    let mut watcher = Raw::connect(port);
    watcher.subscribe(&presence);
    a.hub.kill(&ca);
    watcher.wait(&presence, |p| p["state"] == "offline");
    watcher.close();
    let open = format!("mitcad/{P1}/open/#");
    assert_eq!(Raw::retained(port, &open).len(), 1);

    let mut c = Session::new(SC);
    let cc = c.subscribe(port, P1);
    c.wait_subscribed(P1);
    let offline = c.wait("offline", |e| e["type"] == "session" && e["session"] == SA);
    assert_eq!(offline["state"], "offline");
    // Cleared on the broker by the newcomer.
    let deadline = Instant::now() + WAIT;
    while !Raw::retained(port, &open).is_empty() {
        assert!(Instant::now() < deadline, "the entry stays on the broker");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Its own clearing came back before this version: no open event at all.
    c.publish(
        &cc,
        P1,
        json!({"type": "version", "branch": "main", "commit": P2, "name": "Kim"}),
    );
    c.wait("own version", |e| e["type"] == "version");
    assert!(
        !c.seen.iter().any(|e| e["type"] == "open"),
        "an entry of a session offline: {:#?}",
        c.seen
    );
}

/// The broker restarts: the connection comes back, subscribes again and
/// publishes the session's retained state again.
fn reconnect(broker: &mut dyn Broker) {
    let port = broker.port();
    let mut a = Session::new(SA);
    let ca = a.subscribe(port, P1);
    a.wait_subscribed(P1);
    a.publish(
        &ca,
        P1,
        json!({"type": "open", "path": "part.mitcad", "name": "Alex",
                              "mode": "editing"}),
    );
    a.wait("own open", |e| e["type"] == "open");
    a.clear();
    broker.restart();
    let offline = a.wait_state("offline");
    assert_eq!(offline["error"]["class"], "network", "{offline}");
    a.wait_state("connected");
    a.wait_subscribed(P1);
    let mut b = Session::new(SB);
    b.subscribe(port, P1);
    let open = b.wait("open again", |e| e["type"] == "open");
    assert_eq!(open["entry"]["name"], "Alex");
    let status = a.status();
    assert!(
        status["connections"][0]["connects"].as_u64().unwrap() >= 2,
        "{status}"
    );
}

/// Two projects on one broker share one connection.
fn shared_connection(broker: &mut dyn Broker) {
    let port = broker.port();
    let mut a = Session::new(SA);
    let c1 = a.subscribe(port, P1);
    let c2 = a.subscribe(port, P2);
    assert_eq!(c1, c2);
    a.wait_subscribed(P1);
    a.wait_subscribed(P2);
    let status = a.status();
    assert_eq!(status["connections"].as_array().unwrap().len(), 1);
    assert_eq!(status["connections"][0]["projects"], json!([P1, P2]));
    let mut raw = Raw::connect(port);
    let version = json!({"format": "mitcad-live-version", "version": 1, "branch": "main",
                         "commit": P1, "session": SB, "at": "2026-10-08T13:40:00Z"});
    raw.publish(
        &format!("mitcad/{P2}/versions"),
        version.to_string().as_bytes(),
        false,
    );
    raw.close();
    let event = a.wait("version", |e| e["type"] == "version");
    assert_eq!(event["project"], P2);
    let answer = a.command(json!({"cmd": "unsubscribe", "connection": c1, "project": P1}));
    assert_eq!(answer["closed"], false);
    let answer = a.command(json!({"cmd": "unsubscribe", "connection": c1, "project": P2}));
    assert_eq!(answer["closed"], true);
    a.wait_state("closed");
}

/// Bad messages are dropped and counted; a flood or a packet too large
/// ends the connection, which comes back after the back-off.
fn untrusted_messages(broker: &mut dyn Broker) {
    let port = broker.port();
    let version = json!({"format": "mitcad-live-version", "version": 1, "branch": "main",
                         "commit": P1, "session": SB, "at": "2026-10-08T13:40:00Z"});
    // A retained copy of a version: news is never retained.
    let mut raw = Raw::connect(port);
    raw.publish(
        &format!("mitcad/{P1}/versions"),
        version.to_string().as_bytes(),
        true,
    );
    raw.close();
    let mut a = Session::new(SA);
    let ca = a.subscribe(port, P1);
    a.wait_subscribed(P1);
    let mut raw = Raw::connect(port);
    let f = file("part.mitcad");
    raw.publish(&format!("mitcad/{P1}/versions"), b"not json", false);
    raw.publish(&format!("mitcad/{P1}/chat"), b"{}", false);
    let wrong_path = json!({"format": "mitcad-live-lock", "version": 1, "path": "other.mitcad",
        "owner": {"name": "Eve"}, "session": SB, "state": "active",
        "taken_at": "2026-10-08T13:40:00Z", "active_at": "2026-10-08T13:40:00Z",
        "idle_minutes": 10, "poll_seconds": 10});
    raw.publish(
        &Topic::Lock {
            project: P1.to_owned(),
            file: f.clone(),
        }
        .name("mitcad"),
        wrong_path.to_string().as_bytes(),
        false,
    );
    raw.publish(
        &format!("mitcad/{P1}/versions"),
        &vec![b' '; 20 * 1024],
        false,
    );
    raw.close();
    let dropped = a.wait("5 dropped", |e| e["type"] == "dropped" && e["count"] == 5);
    assert_eq!(dropped["connection"], ca);
    let status = a.status();
    let reasons = &status["connections"][0]["dropped_reasons"];
    assert_eq!(reasons.as_object().unwrap().len(), 5, "{status}");
    assert_eq!(status["connections"][0]["state"], "connected");
    assert!(
        !a.seen.iter().any(|e| e["type"] == "version"),
        "the retained copy is not an event"
    );

    // A flood.
    a.clear();
    let mut raw = Raw::connect(port);
    for _ in 0..400 {
        raw.publish(
            &format!("mitcad/{P1}/versions"),
            version.to_string().as_bytes(),
            false,
        );
    }
    raw.close();
    let offline = a.wait_state("offline");
    assert_eq!(offline["error"]["class"], "flood", "{offline}");
    a.wait_state("connected");
    a.wait_subscribed(P1);

    // A packet larger than 64 KiB.
    a.clear();
    let mut raw = Raw::connect(port);
    raw.publish(
        &format!("mitcad/{P1}/versions"),
        &vec![b'x'; MAX_PACKET + 10],
        false,
    );
    raw.close();
    let offline = a.wait_state("offline");
    assert_eq!(offline["error"]["class"], "protocol", "{offline}");
    a.wait_state("connected");
}

/// The Test button against the broker.
fn test_command(broker: &mut dyn Broker) {
    let a = Session::new(SA);
    let answer = a.command(
        json!({"cmd": "test", "broker": format!("mqtt://127.0.0.1:{}", broker.port()),
                                  "prefix": "team/mitcad"}),
    );
    assert_eq!(answer["ok"], true, "{answer}");
    assert!(
        answer["topic"]
            .as_str()
            .unwrap()
            .starts_with("team/mitcad/test/")
    );
    let steps: Vec<&str> = answer["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["step"].as_str().unwrap())
        .collect();
    assert_eq!(
        steps,
        ["connect", "sign_in", "subscribe", "publish", "receive"]
    );
    assert!(answer["warning"].as_str().unwrap().contains("plain text"));
    // Nothing listens there.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    let answer = a.command(json!({"cmd": "test", "broker": format!("mqtt://127.0.0.1:{port}")}));
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["class"], "network", "{answer}");
}

macro_rules! fake_broker_tests {
    ($($name:ident),*) => {
        mod fake {
            $(
                #[test]
                fn $name() {
                    let mut broker = super::FakeBroker::start(None);
                    super::$name(&mut broker);
                }
            )*
        }
        mod mosquitto {
            $(
                #[test]
                fn $name() {
                    let Some(mut broker) = super::Mosquitto::start(None, None) else {
                        eprintln!("mosquitto is not installed: skipped");
                        return;
                    };
                    super::$name(&mut broker);
                }
            )*
        }
    };
}

fake_broker_tests!(
    messages_between_sessions,
    will_on_a_killed_connection,
    will_seen_by_a_newcomer,
    reconnect,
    shared_connection,
    untrusted_messages,
    test_command
);

/// A mosquitto of the test's own, on free ports in a folder of its own.
struct Mosquitto {
    program: PathBuf,
    dir: PathBuf,
    config: PathBuf,
    port: u16,
    tls_port: Option<u16>,
    child: Option<Child>,
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// A program on `PATH` or in one of `folders`.
fn find_program(name: &str, folders: &[&str]) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(folders.iter().map(PathBuf::from))
        .map(|dir| dir.join(&exe))
        .find(|p| p.is_file())
}

const MOSQUITTO_FOLDERS: [&str; 5] = [
    "/usr/sbin",
    "/usr/local/sbin",
    "/opt/homebrew/sbin",
    "/opt/homebrew/opt/mosquitto/sbin",
    "C:\\Program Files\\mosquitto",
];

/// The mosquitto program: `MITCAD_MOSQUITTO`, else on `PATH` or in the
/// usual folders.
fn mosquitto() -> Option<PathBuf> {
    std::env::var_os("MITCAD_MOSQUITTO")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| find_program("mosquitto", &MOSQUITTO_FOLDERS))
}

/// Test files of a certificate authority and a server certificate for
/// `localhost`, made with openssl.
struct Certificates {
    ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
}

fn openssl() -> Option<PathBuf> {
    find_program("openssl", &["C:\\Program Files\\Git\\usr\\bin"])
}

fn certificates(dir: &Path) -> Option<Certificates> {
    let openssl = openssl()?;
    let config = dir.join("openssl.cnf");
    fs::write(
        &config,
        "[req]\ndistinguished_name = dn\nprompt = no\n[dn]\nCN = Mitcad test\n\
         [ca]\nbasicConstraints = critical,CA:TRUE\nkeyUsage = critical,keyCertSign,cRLSign\n\
         subjectKeyIdentifier = hash\n\
         [server]\nbasicConstraints = critical,CA:FALSE\nkeyUsage = critical,digitalSignature\n\
         extendedKeyUsage = serverAuth\nsubjectAltName = DNS:localhost\n",
    )
    .ok()?;
    let run = |args: &[&str]| {
        Command::new(&openssl)
            .args(args)
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let ec = [
        "-newkey",
        "ec",
        "-pkeyopt",
        "ec_paramgen_curve:prime256v1",
        "-nodes",
    ];
    let ok = run(&[
        &["req", "-x509", "-new"][..],
        &ec,
        &[
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "2",
            "-config",
            "openssl.cnf",
            "-extensions",
            "ca",
            "-subj",
            "/CN=Mitcad test CA",
        ],
    ]
    .concat())
        && run(&[
            &["req", "-new"][..],
            &ec,
            &[
                "-keyout",
                "server.key",
                "-out",
                "server.csr",
                "-config",
                "openssl.cnf",
                "-subj",
                "/CN=localhost",
            ],
        ]
        .concat())
        && run(&[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-set_serial",
            "2",
            "-out",
            "server.pem",
            "-days",
            "2",
            "-extfile",
            "openssl.cnf",
            "-extensions",
            "server",
        ]);
    ok.then(|| Certificates {
        ca: dir.join("ca.pem"),
        cert: dir.join("server.pem"),
        key: dir.join("server.key"),
    })
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Starts mosquitto with `config`; None when it ended before it listened
/// on every one of `ports` (a port taken) or did not start in time.
fn launch(program: &Path, config: &Path, ports: &[u16]) -> Option<Child> {
    let mut child = Command::new(program)
        .arg("-c")
        .arg(config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("mosquitto starts");
    let deadline = Instant::now() + WAIT;
    let ended = |child: &mut Child| child.try_wait().ok().flatten().is_some();
    loop {
        if ended(&mut child) {
            return None;
        }
        if ports
            .iter()
            .all(|port| TcpStream::connect(("127.0.0.1", *port)).is_ok())
        {
            // Still running a moment later: the listener is its own.
            std::thread::sleep(Duration::from_millis(30));
            return (!ended(&mut child)).then_some(child);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

impl Mosquitto {
    /// None when mosquitto is not installed. `tls`: a second listener with
    /// TLS; `password`: a user and password every user name is checked
    /// against (anonymous connections stay allowed).
    fn start(tls: Option<&Certificates>, password: Option<(&str, &str)>) -> Option<Mosquitto> {
        let program = mosquitto()?;
        let dir = test_dir("mosquitto");
        let mut settings = format!(
            "per_listener_settings false\nallow_anonymous true\npersistence false\n\
             log_dest file {}\nlog_type error\nlog_type warning\nlog_type notice\n",
            slashes(&dir.join("mosquitto.log"))
        );
        if let Some((user, pass)) = password {
            let tool = program
                .parent()
                .map(|d| {
                    d.join(
                        program
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .replace("mosquitto", "mosquitto_passwd"),
                    )
                })
                .filter(|p| p.is_file())
                .or_else(|| find_program("mosquitto_passwd", &["/usr/bin", "/opt/homebrew/bin"]))?;
            let file = dir.join("passwd");
            let ok = Command::new(tool)
                .args(["-b", "-c"])
                .arg(&file)
                .args([user, pass])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !ok {
                return None;
            }
            settings.push_str(&format!("password_file {}\n", slashes(&file)));
        }
        // The ports were free when chosen, but another test may take one
        // first: mosquitto then ends at once, and other ports are tried.
        let config = dir.join("mosquitto.conf");
        for _ in 0..5 {
            let port = free_port();
            let mut text = format!("{settings}listener {port} 127.0.0.1\n");
            let tls_port = tls.map(|certificates| {
                let tls_port = free_port();
                text.push_str(&format!(
                    "listener {tls_port} 127.0.0.1\ncafile {}\ncertfile {}\nkeyfile {}\n",
                    slashes(&certificates.ca),
                    slashes(&certificates.cert),
                    slashes(&certificates.key)
                ));
                tls_port
            });
            fs::write(&config, text).unwrap();
            let ports: Vec<u16> = std::iter::once(port).chain(tls_port).collect();
            if let Some(child) = launch(&program, &config, &ports) {
                return Some(Mosquitto {
                    program,
                    dir,
                    config,
                    port,
                    tls_port,
                    child: Some(child),
                });
            }
        }
        panic!(
            "mosquitto did not start: {}",
            fs::read_to_string(dir.join("mosquitto.log")).unwrap_or_default()
        );
    }

    fn spawn(&mut self) {
        let ports: Vec<u16> = std::iter::once(self.port).chain(self.tls_port).collect();
        self.child = launch(&self.program, &self.config, &ports);
        assert!(
            self.child.is_some(),
            "mosquitto did not start again: {}",
            fs::read_to_string(self.dir.join("mosquitto.log")).unwrap_or_default()
        );
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Broker for Mosquitto {
    fn port(&self) -> u16 {
        self.port
    }

    fn restart(&mut self) {
        self.stop();
        self.spawn();
    }
}

impl Drop for Mosquitto {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn test_dir(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mitcad-live-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// TLS with a certificate authority made for the test: connected with
/// it, refused without it and for another host name; a password checked
/// over TLS; the Test button.
#[test]
fn mosquitto_tls() {
    if mosquitto().is_none() {
        eprintln!("mosquitto is not installed: skipped");
        return;
    }
    let dir = test_dir("certificates");
    let Some(certificates) = certificates(&dir) else {
        eprintln!("openssl is not installed: skipped");
        return;
    };
    let Some(broker) = Mosquitto::start(Some(&certificates), Some(("alex", "secret"))) else {
        eprintln!("mosquitto or mosquitto_passwd is not installed: skipped");
        return;
    };
    let tls_port = broker.tls_port.unwrap();
    let ca = slashes(&certificates.ca);
    let mut a = Session::new(SA);
    let connect = |a: &Session, host: &str, ca: Option<&str>, password: &str| {
        a.command(
            json!({"cmd": "connect", "broker": format!("mqtts://{host}:{tls_port}"),
                         "session": SA, "user": "alex", "password": password, "ca": ca}),
        )
    };
    // Without the test's authority: not trusted.
    connect(&a, "localhost", None, "secret");
    let failed = a.wait_state("failed");
    assert_eq!(failed["error"]["class"], "certificate", "{failed}");
    a.command(json!({"cmd": "disconnect"}));
    // Another host name than the certificate's.
    connect(&a, "127.0.0.1", Some(&ca), "secret");
    let failed = a.wait_state("failed");
    assert_eq!(failed["error"]["class"], "certificate", "{failed}");
    assert!(
        failed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("host name"),
        "{failed}"
    );
    a.command(json!({"cmd": "disconnect"}));
    // A wrong password, then the right one on the same connection.
    let answer = connect(&a, "localhost", Some(&ca), "wrong");
    assert_eq!(answer["tls"], true);
    assert!(answer["warning"].is_null());
    let failed = a.wait_state("failed");
    assert_eq!(failed["error"]["class"], "auth_failed", "{failed}");
    let again = connect(&a, "localhost", Some(&ca), "secret");
    assert_eq!(again["connection"], answer["connection"]);
    a.wait_state("connected");
    let connection = answer["connection"].as_str().unwrap();
    a.command(json!({"cmd": "subscribe", "connection": connection, "project": P1}));
    a.wait_subscribed(P1);
    let status = a.status();
    assert_eq!(status["connections"][0]["tls"], true, "{status}");

    // The will over TLS.
    let mut b = Session::new(SB);
    b.subscribe(broker.port(), P1);
    b.wait_subscribed(P1);
    b.command(json!({"cmd": "watch", "connection": "c1", "session": SA}));
    b.wait("online", |e| {
        e["type"] == "session" && e["state"] == "online"
    });
    a.hub.kill(connection);
    b.wait("offline", |e| {
        e["type"] == "session" && e["state"] == "offline"
    });

    // The Test button.
    let test = a.command(
        json!({"cmd": "test", "broker": format!("mqtts://localhost:{tls_port}"),
                                "user": "alex", "password": "secret", "ca": ca}),
    );
    assert_eq!(test["ok"], true, "{test}");
    assert_eq!(test["tls"], true);
    let test = a.command(
        json!({"cmd": "test", "broker": format!("mqtts://localhost:{tls_port}"),
                                "user": "alex", "password": "nope", "ca": ca}),
    );
    assert_eq!(test["ok"], false);
    assert_eq!(test["error"]["class"], "auth_failed", "{test}");
    assert_eq!(test["steps"].as_array().unwrap().len(), 2);
    drop(broker);
    let _ = fs::remove_dir_all(&dir);
}
