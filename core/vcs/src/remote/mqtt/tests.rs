// SPDX-License-Identifier: MIT
//! Tests of live updates without a broker: packets, limits, topics,
//! payloads, the cleaning of text, the hub's commands, and a seeded random
//! run of mutated and truncated input through every reader.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::check::{self, clean_text};
use super::connection::{RateLimit, Tracked};
use super::events::{EventQueue, MAX_EVENTS};
use super::hub::outgoing;
use super::messages::{self, Message, Topic};
use super::packet::{Connect, Decoder, Packet, PacketError, Publish, Will, valid_topic_filter};
use super::{
    LiveErrorClass, LiveHub, MAX_MESSAGES_PER_SECOND, MAX_PACKET, MAX_RETAINED_PER_SECOND, fuzzing,
};

const PROJECT: &str = "0123456789abcdef0123456789abcdef01234567";
const SESSION: &str = "01234567-89ab-cdef-0123-456789abcdef";
const OTHER: &str = "fedcba98-7654-3210-fedc-ba9876543210";

fn decode_all(bytes: &[u8]) -> Result<Vec<Packet>, PacketError> {
    let mut decoder = Decoder::new(MAX_PACKET);
    decoder.push(bytes);
    let mut packets = Vec::new();
    while let Some(packet) = decoder.next_packet()? {
        packets.push(packet);
    }
    assert_eq!(decoder.buffered(), 0, "whole packets only");
    Ok(packets)
}

fn sample_packets() -> Vec<Packet> {
    vec![
        Packet::Connect(Connect {
            client_id: "mitcad0123456789abcdef0".to_owned(),
            keep_alive: 30,
            clean_session: true,
            will: Some(Will {
                topic: format!("mitcad/sessions/{SESSION}"),
                payload: br#"{"format":"mitcad-live-session","version":1,"state":"offline"}"#
                    .to_vec(),
                qos: 1,
                retain: true,
            }),
            user: Some("user".to_owned()),
            password: Some(b"secret".to_vec()),
        }),
        Packet::Connect(Connect {
            client_id: String::new(),
            keep_alive: 0,
            clean_session: false,
            will: None,
            user: None,
            password: None,
        }),
        Packet::ConnAck {
            session_present: false,
            code: 0,
        },
        Packet::ConnAck {
            session_present: true,
            code: 5,
        },
        Packet::Publish(Publish {
            topic: format!("mitcad/{PROJECT}/versions"),
            payload: b"{}".to_vec(),
            qos: 1,
            retain: false,
            dup: true,
            id: 7,
        }),
        Packet::Publish(Publish {
            topic: "a/b".to_owned(),
            payload: Vec::new(),
            qos: 0,
            retain: true,
            dup: false,
            id: 0,
        }),
        Packet::PubAck(65535),
        Packet::Subscribe {
            id: 1,
            filters: vec![("a/+/c".to_owned(), 1), ("a/#".to_owned(), 0)],
        },
        Packet::SubAck {
            id: 1,
            codes: vec![1, 0x80],
        },
        Packet::Unsubscribe {
            id: 2,
            filters: vec!["a/#".to_owned()],
        },
        Packet::UnsubAck(2),
        Packet::PingReq,
        Packet::PingResp,
        Packet::Disconnect,
    ]
}

#[test]
fn packets_round_trip() {
    for packet in sample_packets() {
        let bytes = packet.encode().unwrap();
        assert_eq!(decode_all(&bytes).unwrap(), vec![packet.clone()]);
        // A byte at a time.
        let mut decoder = Decoder::new(MAX_PACKET);
        let mut decoded = None;
        for (i, byte) in bytes.iter().enumerate() {
            decoder.push(&[*byte]);
            let next = decoder.next_packet().unwrap();
            assert_eq!(next.is_some(), i + 1 == bytes.len(), "{packet:?} at {i}");
            decoded = decoded.or(next);
        }
        assert_eq!(decoded, Some(packet));
    }
    // Several in one read.
    let all: Vec<u8> = sample_packets()
        .iter()
        .flat_map(|p| p.encode().unwrap())
        .collect();
    assert_eq!(decode_all(&all).unwrap(), sample_packets());
}

#[test]
fn remaining_lengths() {
    // The body of a QoS 0 PUBLISH to "t" is 3 bytes and the payload.
    for (body, header) in [
        (3usize, 2usize),
        (127, 2),
        (128, 3),
        (16_383, 3),
        (16_384, 4),
        (MAX_PACKET, 4),
    ] {
        let packet = Packet::Publish(Publish {
            topic: "t".to_owned(),
            payload: vec![b'x'; body - 3],
            qos: 0,
            retain: false,
            dup: false,
            id: 0,
        });
        let bytes = packet.encode().unwrap();
        assert_eq!(bytes.len(), header + body);
        assert_eq!(decode_all(&bytes).unwrap(), vec![packet]);
    }
}

#[test]
fn packet_limits() {
    // The remaining length is refused as soon as it is read, before any
    // of the body: 65537 = 1 + 0 * 128 + 4 * 16384.
    let mut decoder = Decoder::new(MAX_PACKET);
    decoder.push(&[0x30, 0x81, 0x80, 0x04]);
    assert_eq!(decoder.next_packet(), Err(PacketError::TooLarge(65_537)));
    // Exactly the limit is read.
    let mut decoder = Decoder::new(10);
    decoder.push(&[0x30, 10, 0, 1, b't']);
    assert_eq!(decoder.next_packet(), Ok(None));
    decoder.push(&[b'x'; 7]);
    assert!(matches!(
        decoder.next_packet(),
        Ok(Some(Packet::Publish(_)))
    ));
    // A length of five bytes, and one not in its shortest form.
    assert!(decode_all(&[0x30, 0xff, 0xff, 0xff, 0xff, 0x01]).is_err());
    assert!(decode_all(&[0xc0, 0x80, 0x00]).is_err());
    // An incomplete length waits for more.
    let mut decoder = Decoder::new(MAX_PACKET);
    decoder.push(&[0x30, 0x80]);
    assert_eq!(decoder.next_packet(), Ok(None));
}

#[test]
fn malformed_packets() {
    let topic = |t: &str| {
        let mut v = (t.len() as u16).to_be_bytes().to_vec();
        v.extend_from_slice(t.as_bytes());
        v
    };
    let with = |first: u8, body: Vec<u8>| {
        let mut v = vec![first, body.len() as u8];
        v.extend(body);
        v
    };
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("QoS 2", with(0x34, [topic("a"), vec![0, 1]].concat())),
        ("QoS 3", with(0x36, [topic("a"), vec![0, 1]].concat())),
        ("dup with QoS 0", with(0x38, topic("a"))),
        ("wildcard topic", with(0x30, topic("a/#"))),
        ("plus topic", with(0x30, topic("a/+"))),
        ("empty topic", with(0x30, topic(""))),
        ("packet id 0", with(0x32, [topic("a"), vec![0, 0]].concat())),
        ("not UTF-8", with(0x30, vec![0, 2, 0xc3, 0x28])),
        ("U+0000", with(0x30, vec![0, 3, b'a', 0, b'b'])),
        ("short string", with(0x30, vec![0, 5, b'a'])),
        (
            "subscribe flags",
            with(0x80, [vec![0, 1], topic("a"), vec![1]].concat()),
        ),
        (
            "subscribe qos 3",
            with(0x82, [vec![0, 1], topic("a"), vec![3]].concat()),
        ),
        ("subscribe empty", with(0x82, vec![0, 1])),
        (
            "subscribe filter",
            with(0x82, [vec![0, 1], topic("a/#/b"), vec![1]].concat()),
        ),
        ("unsubscribe empty", with(0xa2, vec![0, 1])),
        ("suback code", with(0x90, vec![0, 1, 3])),
        ("suback empty", with(0x90, vec![0, 1])),
        ("connack code", with(0x20, vec![0, 6])),
        ("connack flags", with(0x20, vec![2, 0])),
        ("connack short", with(0x20, vec![0])),
        ("puback trailing", with(0x40, vec![0, 1, 0])),
        ("puback flags", with(0x41, vec![0, 1])),
        ("pubrec", with(0x50, vec![0, 1])),
        ("pubrel", with(0x62, vec![0, 1])),
        ("pubcomp", with(0x70, vec![0, 1])),
        ("reserved 0", with(0x00, vec![])),
        ("reserved 15", with(0xf0, vec![])),
        ("pingresp body", with(0xd0, vec![0])),
        ("disconnect flags", with(0xe1, vec![])),
        (
            "connect protocol",
            with(
                0x10,
                [topic("MQIsdp"), vec![3, 2, 0, 30], topic("x")].concat(),
            ),
        ),
        (
            "connect level",
            with(
                0x10,
                [topic("MQTT"), vec![5, 2, 0, 30], topic("x")].concat(),
            ),
        ),
        (
            "connect reserved",
            with(
                0x10,
                [topic("MQTT"), vec![4, 3, 0, 30], topic("x")].concat(),
            ),
        ),
        (
            "password without user",
            with(
                0x10,
                [topic("MQTT"), vec![4, 0x42, 0, 30], topic("x"), topic("p")].concat(),
            ),
        ),
        (
            "will qos without will",
            with(
                0x10,
                [topic("MQTT"), vec![4, 0x0a, 0, 30], topic("x")].concat(),
            ),
        ),
    ];
    for (name, bytes) in cases {
        assert!(decode_all(&bytes).is_err(), "{name} was accepted");
    }
}

#[test]
fn topic_filters() {
    for good in ["a", "a/b", "+", "#", "a/+/c", "a/#", "+/+", "/"] {
        assert!(valid_topic_filter(good), "{good}");
    }
    for bad in ["", "a#", "a/#/b", "a+", "a/b+/c", "##"] {
        assert!(!valid_topic_filter(bad), "{bad}");
    }
}

#[test]
fn cleaning_text() {
    assert_eq!(clean_text("  Alex \t Smith\n"), "Alex Smith");
    assert_eq!(clean_text("Al\u{0}ex\u{7}\u{1b}[31m"), "Alex[31m");
    // A bidirectional override that would show "exe.txt" as "txt.exe".
    assert_eq!(clean_text("report\u{202e}txt.exe"), "reporttxt.exe");
    assert_eq!(
        clean_text("a\u{200b}b\u{200d}c\u{2066}d\u{2069}e\u{feff}f\u{00ad}g"),
        "abcdefg"
    );
    assert_eq!(clean_text("\u{3164}\u{115f}x\u{e0041}"), "x");
    assert_eq!(clean_text("a\u{2028}b\u{2029}\u{a0}c"), "a b c");
    assert_eq!(clean_text("\u{85}\u{9c}"), "");
    // Markup stays text: the application shows it as plain text.
    assert_eq!(
        clean_text("<img src=x onerror=alert(1)>"),
        "<img src=x onerror=alert(1)>"
    );
    assert_eq!(clean_text("José Müller 日本"), "José Müller 日本");
    assert!(check::has_hidden("a\u{200e}b"));
    assert!(check::has_hidden("a\tb"));
    assert!(!check::has_hidden("a b/c.mitcad"));

    assert_eq!(check::name(" Sam ").unwrap(), "Sam");
    assert!(check::name(" \u{200b} ").is_err());
    assert!(check::name(&"x".repeat(100)).is_ok());
    assert!(check::name(&"x".repeat(101)).is_err());
    assert!(
        check::name(&"é".repeat(100)).is_ok(),
        "characters, not bytes"
    );
    assert!(check::message(&"m".repeat(200)).is_ok());
    assert!(check::message(&"m".repeat(201)).is_err());
    assert_eq!(check::message("").unwrap(), "");
}

#[test]
fn ids_paths_and_branches() {
    assert!(check::is_session(SESSION));
    for bad in [
        "01234567-89AB-cdef-0123-456789abcdef",
        "0123456789abcdef0123456789abcdef",
        "01234567-89ab-cdef-0123-456789abcde",
        "01234567-89ab-cdef-0123-456789abcdefa",
        "01234567_89ab_cdef_0123_456789abcdef",
        "",
    ] {
        assert!(!check::is_session(bad), "{bad}");
    }
    assert!(check::is_commit(PROJECT));
    assert!(check::is_commit(&"a".repeat(64)));
    assert!(!check::is_commit(&"a".repeat(41)));
    assert!(!check::is_commit(&"A".repeat(40)));
    assert!(!check::is_commit(&"g".repeat(40)));
    assert!(check::is_file_id(&"0".repeat(64)));
    assert!(!check::is_file_id(PROJECT));

    for good in [
        "part.mitcad",
        "a/b c/part.mitcad",
        "é/日本.mitcad",
        ".hidden/x",
    ] {
        assert!(check::is_path(good), "{good}");
    }
    let long = "a/".repeat(600);
    for bad in [
        "",
        "/abs.mitcad",
        "a//b",
        "a/../b",
        "./a",
        "a/.",
        "c:/x",
        "a\\b",
        "a/",
        "a\u{202e}b",
        "a\nb",
        long.as_str(),
    ] {
        assert!(!check::is_path(bad), "{bad}");
    }
    for good in ["main", "feature/x-1", "v1.2", "a_b"] {
        assert!(check::is_branch(good), "{good}");
    }
    for bad in [
        "", "-x", ".x", "/x", "x/", "x.", "x.lock", "a..b", "a//b", "a/.b", "a b", "a~b", "a:b",
        "é",
    ] {
        assert!(!check::is_branch(bad), "{bad}");
    }
}

#[test]
fn times() {
    let t = check::parse_time("2026-10-08T13:40:00Z").unwrap();
    assert_eq!(check::format_time(t), "2026-10-08T13:40:00Z");
    assert_eq!(check::parse_time("2026-10-08t13:40:00z"), Some(t));
    assert_eq!(check::parse_time("2026-10-08T16:40:00+03:00"), Some(t));
    assert_eq!(check::parse_time("2026-10-08T13:10:00-00:30"), Some(t));
    assert_eq!(check::parse_time("2026-10-08T13:40:00.123456789Z"), Some(t));
    assert_eq!(
        check::parse_time("2016-12-31T23:59:60Z"),
        None,
        "before the range"
    );
    assert_eq!(
        check::parse_time("2028-12-31T23:59:60Z"),
        check::parse_time("2028-12-31T23:59:59Z"),
        "a leap second"
    );
    assert!(check::parse_time("2028-02-29T00:00:00Z").is_some());
    for bad in [
        "2027-02-29T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-10-08T24:00:00Z",
        "2026-10-08T13:60:00Z",
        "2026-10-08T13:40:61Z",
        "2026-10-08 13:40:00Z",
        "2026-10-08T13:40:00",
        "2026-10-08T13:40:00+3:00",
        "2026-10-08T13:40:00+24:00",
        "2026-10-08T13:40:00.Z",
        "2026-10-08T13:40:00.1234567890Z",
        "2019-12-31T23:59:59Z",
        "2100-01-01T00:00:00Z",
        "+2026-10-08T13:40:00Z",
        "2026-1O-08T13:40:00Z",
        "",
    ] {
        assert_eq!(check::parse_time(bad), None, "{bad}");
    }
    assert_eq!(check::format_time(check::EARLIEST), "2020-01-01T00:00:00Z");
    assert_eq!(check::format_time(check::LATEST), "2100-01-01T00:00:00Z");
    // Every day of the range, both ways.
    let mut day = check::EARLIEST;
    while day < check::LATEST {
        let text = check::format_time(day + 45_296);
        assert_eq!(check::parse_time(&text), Some(day + 45_296), "{text}");
        day += 86_400;
    }
}

fn file(path: &str) -> String {
    messages::file_id(path)
}

#[test]
fn topics() {
    let f = file("part.mitcad");
    let topics = [
        Topic::Lock {
            project: PROJECT.to_owned(),
            file: f.clone(),
        },
        Topic::Requests {
            project: PROJECT.to_owned(),
            file: f.clone(),
        },
        Topic::Versions {
            project: PROJECT.to_owned(),
        },
        Topic::Open {
            project: PROJECT.to_owned(),
            file: f.clone(),
            session: SESSION.to_owned(),
        },
        Topic::Session {
            session: SESSION.to_owned(),
        },
    ];
    for topic in &topics {
        for prefix in ["mitcad", "team-a/mitcad"] {
            let name = topic.name(prefix);
            assert_eq!(Topic::parse(prefix, &name).as_ref(), Some(topic), "{name}");
            assert_eq!(Topic::parse("other", &name), None);
        }
    }
    assert_eq!(topics[0].project(), Some(PROJECT));
    assert_eq!(topics[4].project(), None);
    for bad in [
        format!("mitcadx/{PROJECT}/versions"),
        format!("mitcad/{PROJECT}/versions/x"),
        format!("mitcad/{PROJECT}/locks"),
        format!("mitcad/{PROJECT}/locks/{}", &f[..63]),
        format!("mitcad/{PROJECT}/locks/{}", f.to_uppercase()),
        format!("mitcad/{PROJECT}/open/{f}"),
        format!("mitcad/{PROJECT}/open/{f}/not-a-session"),
        format!("mitcad/{PROJECT}/unknown/{f}"),
        format!("mitcad/{}/versions", &PROJECT[..39]),
        "mitcad/sessions/x".to_owned(),
        format!("mitcad/test/{SESSION}"),
        "mitcad".to_owned(),
        "mitcad/".to_owned(),
    ] {
        assert_eq!(Topic::parse("mitcad", &bad), None, "{bad}");
    }
}

fn lock_topic(path: &str) -> Topic {
    Topic::Lock {
        project: PROJECT.to_owned(),
        file: file(path),
    }
}

fn read(topic: &Topic, value: Value) -> Result<Message, &'static str> {
    messages::read(topic, value.to_string().as_bytes())
}

fn lock_json() -> Value {
    json!({"format": "mitcad-live-lock", "version": 1, "path": "dir/part.mitcad",
           "owner": {"name": "Alex", "email": "not@sent.example"}, "session": SESSION,
           "state": "active", "taken_at": "2026-10-08T13:40:00+03:00",
           "active_at": "2026-10-08T10:42:00Z", "idle_minutes": 10, "poll_seconds": 10,
           "commit": PROJECT, "later": "ignored"})
}

#[test]
fn lock_summaries() {
    let topic = lock_topic("dir/part.mitcad");
    let Ok(Message::Lock(Some(lock))) = read(&topic, lock_json()) else {
        panic!("not read");
    };
    assert_eq!(lock.owner.name, "Alex");
    assert_eq!(lock.taken_at, "2026-10-08T10:40:00Z", "in UTC");
    assert_eq!(lock.commit.as_deref(), Some(PROJECT));
    assert_eq!(read(&topic, json!(null)).ok(), None, "JSON null");
    assert_eq!(messages::read(&topic, b""), Ok(Message::Lock(None)));

    let changed = |field: &str, value: Value| {
        let mut lock = lock_json();
        lock[field] = value;
        read(&topic, lock)
    };
    // Clamped to the settings' bounds: a lock cannot make others wait
    // longer.
    let Ok(Message::Lock(Some(clamped))) = changed("idle_minutes", json!(100_000)) else {
        panic!();
    };
    assert_eq!(clamped.idle_minutes, 120);
    let Ok(Message::Lock(Some(clamped))) = changed("poll_seconds", json!(1)) else {
        panic!();
    };
    assert_eq!(clamped.poll_seconds, 5);
    let Ok(Message::Lock(Some(cleaned))) = changed("owner", json!({"name": "\u{202e}xelA"})) else {
        panic!();
    };
    assert_eq!(cleaned.owner.name, "xelA");
    for (field, value) in [
        ("version", json!(2)),
        ("version", json!("1")),
        ("format", json!("mitcad-live-open")),
        ("path", json!("other.mitcad")),
        ("path", json!("../dir/part.mitcad")),
        ("session", json!("someone")),
        ("state", json!("busy")),
        ("taken_at", json!("yesterday")),
        ("active_at", json!("2101-01-01T00:00:00Z")),
        ("idle_minutes", json!(-1)),
        ("idle_minutes", json!(1e30)),
        ("poll_seconds", json!("10")),
        ("commit", json!("HEAD")),
        ("owner", json!({"name": ""})),
        ("owner", json!({"name": "x".repeat(101)})),
        ("owner", json!("Alex")),
    ] {
        assert!(changed(field, value.clone()).is_err(), "{field}: {value}");
    }
    let mut missing = lock_json();
    missing.as_object_mut().unwrap().remove("session");
    assert!(read(&topic, missing).is_err());
    assert!(messages::read(&topic, &vec![b' '; 16 * 1024 + 1]).is_err());
    assert!(messages::read(&topic, b"\xff\xfe").is_err());
    assert!(messages::read(&topic, b"[1,2]").is_err());
}

#[test]
fn requests_versions_open_and_presence() {
    let requests = Topic::Requests {
        project: PROJECT.to_owned(),
        file: file("a.mitcad"),
    };
    let request = |fields: Value| {
        let mut value = json!({"format": "mitcad-live-request", "version": 1,
                               "session": SESSION, "at": "2026-10-08T13:40:00Z"});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        read(&requests, value)
    };
    assert!(request(json!({"kind": "request", "name": "Sam", "message": "please"})).is_ok());
    let Ok(Message::Request(r)) =
        request(json!({"kind": "request", "name": "Sam", "message": " "}))
    else {
        panic!();
    };
    assert_eq!(r.message, None, "an empty message is none");
    assert!(request(json!({"kind": "withdrawn"})).is_ok());
    assert!(request(json!({"kind": "receipt", "name": "Alex", "for": OTHER})).is_ok());
    assert!(
        request(
            json!({"kind": "answer", "name": "Alex", "for": OTHER, "answer": "keep",
                       "until": "2026-10-08T14:00:00Z"})
        )
        .is_ok()
    );
    for bad in [
        json!({"kind": "request"}),
        json!({"kind": "request", "name": "Sam", "message": "m".repeat(201)}),
        json!({"kind": "receipt", "name": "Alex"}),
        json!({"kind": "receipt", "name": "Alex", "for": "x"}),
        json!({"kind": "answer", "name": "Alex", "for": OTHER}),
        json!({"kind": "answer", "name": "Alex", "for": OTHER, "answer": "maybe"}),
        json!({"kind": "request", "name": "Sam", "answer": "keep"}),
        json!({"kind": "answer", "name": "Alex", "for": OTHER, "answer": "keep", "until": "soon"}),
        json!({"kind": "ask", "name": "Sam"}),
    ] {
        assert!(request(bad.clone()).is_err(), "{bad}");
    }
    assert!(messages::read(&requests, b"").is_err(), "not retained");

    let versions = Topic::Versions {
        project: PROJECT.to_owned(),
    };
    let version = |branch: &str, commit: &str| {
        read(
            &versions,
            json!({"format": "mitcad-live-version", "version": 1, "branch": branch,
                   "commit": commit, "session": SESSION, "name": "Alex",
                   "at": "2026-10-08T13:40:00Z"}),
        )
    };
    assert!(version("main", PROJECT).is_ok());
    assert!(version("main..x", PROJECT).is_err());
    assert!(version("main", "abc").is_err());

    let open = Topic::Open {
        project: PROJECT.to_owned(),
        file: file("a.mitcad"),
        session: SESSION.to_owned(),
    };
    let entry = |mode: &str| {
        read(
            &open,
            json!({"format": "mitcad-live-open", "version": 1, "name": "Kim", "mode": mode,
                   "since": "2026-10-08T13:40:00Z"}),
        )
    };
    assert!(entry("editing").is_ok());
    assert!(entry("read-only").is_ok());
    assert!(entry("read_only").is_err());
    assert_eq!(messages::read(&open, b""), Ok(Message::Open(None)));

    let session = Topic::Session {
        session: SESSION.to_owned(),
    };
    for state in ["online", "offline"] {
        let value = json!({"format": "mitcad-live-session", "version": 1, "state": state});
        assert!(read(&session, value).is_ok());
    }
    let bad = json!({"format": "mitcad-live-session", "version": 1, "state": "away"});
    assert!(read(&session, bad).is_err());
    assert_eq!(messages::read(&session, b""), Ok(Message::Session(None)));
}

#[test]
fn messages_written_are_read() {
    let topic = lock_topic("dir/part.mitcad");
    let Ok(message) = read(&topic, lock_json()) else {
        panic!();
    };
    let bytes = messages::encode_message(&message);
    assert_eq!(messages::read(&topic, &bytes), Ok(message));
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("email"), "no email address: {text}");
    assert!(!text.contains("later"));
}

#[test]
fn outgoing_messages() {
    let out = |message: Value| outgoing("mitcad", SESSION, PROJECT, &message);
    let lock = out(json!({"type": "lock", "path": "dir/part.mitcad",
        "lock": {"owner": {"name": " Alex ", "email": "alex@example.com"},
                 "idle_minutes": 500, "taken_at": "2026-10-08T13:40:00+03:00"}}))
    .unwrap();
    assert_eq!(lock.topic, lock_topic("dir/part.mitcad").name("mitcad"));
    assert!(lock.retain && lock.owned && lock.qos == 1);
    let value: Value = serde_json::from_slice(&lock.payload).unwrap();
    assert_eq!(value["owner"], json!({"name": "Alex"}), "cleaned, no email");
    assert_eq!(value["idle_minutes"], 120, "clamped");
    assert_eq!(value["taken_at"], "2026-10-08T10:40:00Z");
    assert_eq!(value["session"], SESSION);

    let handed = out(json!({"type": "lock", "path": "a.mitcad",
        "lock": {"owner": {"name": "Sam"}, "session": OTHER}}))
    .unwrap();
    assert!(!handed.owned, "another session's lock");
    let released = out(json!({"type": "lock", "path": "a.mitcad", "lock": null})).unwrap();
    assert!(released.payload.is_empty() && released.retain);

    let request = out(json!({"type": "request", "path": "a.mitcad", "name": "Sam",
                             "message": "May I?"}))
    .unwrap();
    assert!(!request.retain && request.qos == 1);
    assert!(request.topic.contains("/requests/"));
    let open = out(json!({"type": "open", "path": "a.mitcad", "name": "Sam",
                          "mode": "read-only"}))
    .unwrap();
    assert!(open.topic.ends_with(SESSION) && open.owned && open.retain);
    let closed = out(json!({"type": "closed", "path": "a.mitcad"})).unwrap();
    assert_eq!(closed.topic, open.topic);
    assert!(closed.payload.is_empty());
    let version = out(json!({"type": "version", "branch": "main", "commit": PROJECT})).unwrap();
    assert!(version.topic.ends_with("/versions"));

    for bad in [
        json!({"type": "lock", "path": "../a.mitcad", "lock": null}),
        json!({"type": "lock", "path": "a.mitcad", "lock": {"owner": {"name": ""}}}),
        json!({"type": "lock", "path": "a.mitcad", "lock": {"owner": {"name": "A"}, "x": 1}}),
        json!({"type": "request", "path": "a.mitcad", "name": "Sam", "message": "m".repeat(201)}),
        json!({"type": "receipt", "path": "a.mitcad", "name": "Alex"}),
        json!({"type": "answer", "path": "a.mitcad", "name": "Alex", "for": OTHER,
               "answer": "perhaps"}),
        json!({"type": "open", "path": "a.mitcad", "name": "Sam", "mode": "viewing"}),
        json!({"type": "version", "branch": "main", "commit": "HEAD"}),
        json!({"type": "chat", "text": "hi"}),
        json!({"type": "open", "path": "a.mitcad", "name": "Sam", "mode": "editing", "x": 1}),
        json!("lock"),
    ] {
        assert!(out(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn hub_commands_without_a_broker() {
    let hub = LiveHub::default();
    let error = |command: Value| hub.command(&command.to_string()).unwrap_err();
    let insecure = error(json!({"cmd": "connect", "broker": "mqtt://127.0.0.1:1",
                                "session": SESSION, "user": "u", "password": "secret"}));
    assert_eq!(insecure.class, LiveErrorClass::Insecure, "{insecure}");
    let test = error(
        json!({"cmd": "test", "broker": "mqtt://127.0.0.1:1", "user": "u",
                            "password": "secret"}),
    );
    assert_eq!(test.class, LiveErrorClass::Insecure);
    for bad in [
        json!({"cmd": "connect", "broker": "http://x", "session": SESSION}),
        json!({"cmd": "connect", "broker": "mqtts://user@x", "session": SESSION}),
        json!({"cmd": "connect", "broker": "mqtts://x", "session": "me"}),
        json!({"cmd": "connect", "broker": "mqtts://x"}),
        json!({"cmd": "connect", "broker": "mqtts://x", "session": SESSION, "prefix": "a/#"}),
        json!({"cmd": "connect", "broker": "mqtts://x", "session": SESSION, "pasword": "x"}),
        json!({"cmd": "connect", "broker": "mqtts://x", "session": SESSION, "password": "x"}),
        json!({"cmd": "connect", "broker": "mqtts://x", "session": SESSION,
               "ca": "/no/such/file.pem"}),
        json!({"cmd": "subscribe", "connection": "c9", "project": PROJECT}),
        json!({"cmd": "subscribe", "connection": "c1", "project": "main"}),
        json!({"cmd": "publish", "connection": "c1", "project": PROJECT,
               "message": {"type": "version", "branch": "main", "commit": PROJECT}}),
        json!({"cmd": "watch", "connection": "c1", "session": "x"}),
        json!({"cmd": "disconnect", "connection": "c1"}),
        json!({"cmd": "dance"}),
        json!({"broker": "mqtts://x"}),
        json!([]),
    ] {
        assert_eq!(error(bad.clone()).class, LiveErrorClass::Invalid, "{bad}");
    }
    assert!(hub.command("not json").is_err());

    let status: Value =
        serde_json::from_str(&hub.command(r#"{"cmd": "status"}"#).unwrap()).unwrap();
    assert_eq!(status["connections"], json!([]));
    let events: Value = serde_json::from_str(&hub.events(Duration::ZERO)).unwrap();
    assert_eq!(events, json!({"events": [], "lost": 0, "closed": false}));
    hub.command(r#"{"cmd": "close"}"#).unwrap();
    let started = Instant::now();
    let events: Value = serde_json::from_str(&hub.events(Duration::from_secs(10))).unwrap();
    assert_eq!(events["closed"], true);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a closed hub does not wait"
    );
    let closed = error(json!({"cmd": "connect", "broker": "mqtts://x", "session": SESSION}));
    assert_eq!(closed.class, LiveErrorClass::Closed);
}

#[test]
fn rate_limit() {
    let start = Instant::now();
    let mut rate = RateLimit::new(start);
    for _ in 0..MAX_MESSAGES_PER_SECOND {
        assert!(!rate.exceeded(start + Duration::from_millis(500), false));
    }
    // Retained copies after a subscribe count apart.
    for _ in 0..MAX_RETAINED_PER_SECOND {
        assert!(!rate.exceeded(start + Duration::from_millis(600), true));
    }
    assert!(rate.exceeded(start + Duration::from_millis(700), true));
    assert!(rate.exceeded(start + Duration::from_millis(999), false));
    // A new second starts the count anew.
    assert!(!rate.exceeded(start + Duration::from_millis(1500), false));
}

#[test]
fn retained_limits() {
    let mut tracked = Tracked::default();
    let f = file("a.mitcad");
    tracked.set(&f, None, true).unwrap();
    let sessions: Vec<String> = (0..70)
        .map(|i| format!("{i:08x}-0000-0000-0000-000000000000"))
        .collect();
    for session in &sessions[..63] {
        tracked.set(&f, Some(session), true).unwrap();
    }
    assert!(tracked.set(&f, Some(&sessions[63]), true).is_err(), "65th");
    // Again the same is not a new one; a removal makes room.
    tracked.set(&f, Some(&sessions[0]), true).unwrap();
    tracked.set(&f, None, false).unwrap();
    tracked.set(&f, Some(&sessions[63]), true).unwrap();
    // Per project.
    let mut tracked = Tracked::default();
    let mut refused = 0;
    for i in 0..10_050 {
        if tracked
            .set(&file(&format!("{i}.mitcad")), None, true)
            .is_err()
        {
            refused += 1;
        }
    }
    assert_eq!(refused, 50);
}

#[test]
fn event_queue() {
    let queue = EventQueue::default();
    let dropped = |n: u64| json!({"type": "dropped", "connection": "c1", "count": n});
    queue.push(json!({"type": "state", "connection": "c1"}));
    for n in 1..=100 {
        queue.push_dropped(dropped(n));
    }
    queue.push_dropped(json!({"type": "dropped", "connection": "c2", "count": 1}));
    let (events, lost, closed) = queue.take(Duration::ZERO);
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[1]["count"], 100);
    assert_eq!((lost, closed), (0, false));
    for n in 0..MAX_EVENTS + 5 {
        queue.push(json!({"n": n}));
    }
    let (events, lost, _) = queue.take(Duration::ZERO);
    assert_eq!(lost, 5);
    assert_eq!(events[0]["n"], 5, "the oldest go");
    let mut total = events.len();
    loop {
        let (events, _, _) = queue.take(Duration::ZERO);
        if events.is_empty() {
            break;
        }
        total += events.len();
    }
    assert_eq!(total, MAX_EVENTS);
    let started = Instant::now();
    let (events, _, _) = queue.take(Duration::from_millis(50));
    assert!(events.is_empty());
    assert!(started.elapsed() >= Duration::from_millis(50));
}

/// A small generator of pseudo-random numbers (xorshift64*), seeded, so
/// that a failure can be repeated.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn mutate(&mut self, mut data: Vec<u8>, pool: &[Vec<u8>]) -> Vec<u8> {
        for _ in 0..=self.below(4) {
            match self.below(8) {
                0 if !data.is_empty() => {
                    let at = self.below(data.len());
                    data[at] ^= 1 << self.below(8);
                }
                1 if !data.is_empty() => {
                    let at = self.below(data.len());
                    data[at] = self.next() as u8;
                }
                2 => data.truncate(self.below(data.len() + 1)),
                3 => {
                    let at = self.below(data.len() + 1);
                    let bytes: Vec<u8> = (0..self.below(16)).map(|_| self.next() as u8).collect();
                    data.splice(at..at, bytes);
                }
                4 if !data.is_empty() => {
                    let from = self.below(data.len());
                    let to = from + self.below(data.len() - from + 1);
                    let copy = data[from..to].to_vec();
                    let at = self.below(data.len() + 1);
                    data.splice(at..at, copy);
                }
                5 => {
                    let other = &pool[self.below(pool.len())];
                    let at = self.below(data.len() + 1);
                    let from = self.below(other.len() + 1);
                    data.truncate(at);
                    data.extend_from_slice(&other[from..]);
                }
                6 if !data.is_empty() => {
                    let at = self.below(data.len());
                    data[at] = *[0x00, 0x7f, 0x80, 0xff, b'"', b'{', b'\\']
                        .get(self.below(7))
                        .unwrap();
                }
                _ => {
                    let at = self.below(data.len() + 1);
                    data.insert(at, self.next() as u8);
                }
            }
        }
        data
    }

    /// A JSON value with a field replaced by something of another kind.
    fn mutate_json(&mut self, mut value: Value) -> Value {
        let replacements = [
            json!(null),
            json!(true),
            json!(-1),
            json!(1e300),
            json!(u64::MAX),
            json!(""),
            json!("\u{202e}\u{0}\u{1b}x"),
            json!("x".repeat(300)),
            json!([1, [2, [3]]]),
            json!({"name": {"name": 1}}),
            json!("2026-02-30T00:00:00Z"),
        ];
        if let Some(object) = value.as_object_mut() {
            let keys: Vec<String> = object.keys().cloned().collect();
            if !keys.is_empty() {
                let key = &keys[self.below(keys.len())];
                if self.below(4) == 0 {
                    object.remove(key);
                } else {
                    object.insert(
                        key.clone(),
                        replacements[self.below(replacements.len())].clone(),
                    );
                }
            }
        }
        value
    }
}

#[test]
fn random_input_is_dropped_or_read_never_a_panic() {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let packets: Vec<Vec<u8>> = sample_packets()
        .iter()
        .map(|p| p.encode().unwrap())
        .collect();
    let payloads: Vec<Vec<u8>> = [
        lock_json(),
        json!({"format": "mitcad-live-request", "version": 1, "kind": "answer",
               "session": SESSION, "name": "Alex", "at": "2026-10-08T13:40:00Z", "for": OTHER,
               "answer": "keep", "until": "2026-10-08T14:00:00Z", "message": "later"}),
        json!({"format": "mitcad-live-version", "version": 1, "branch": "main",
               "commit": PROJECT, "session": SESSION, "at": "2026-10-08T13:40:00Z"}),
        json!({"format": "mitcad-live-open", "version": 1, "name": "Kim", "mode": "editing",
               "since": "2026-10-08T13:40:00Z"}),
        json!({"format": "mitcad-live-session", "version": 1, "state": "online",
               "since": "2026-10-08T13:40:00Z"}),
    ]
    .iter()
    .map(|v| v.to_string().into_bytes())
    .collect();
    let topics: Vec<Vec<u8>> = [
        lock_topic("dir/part.mitcad").name("mitcad"),
        format!("mitcad/{PROJECT}/requests/{}", file("a")),
        format!("mitcad/{PROJECT}/versions"),
        format!("mitcad/{PROJECT}/open/{}/{SESSION}", file("a")),
        format!("mitcad/sessions/{SESSION}"),
    ]
    .iter()
    .map(|t| t.clone().into_bytes())
    .collect();
    let mut read = 0;
    for _ in 0..30_000 {
        // Packets, mutated and truncated, through the decoder.
        let sample = packets[random.below(packets.len())].clone();
        let data = random.mutate(sample, &packets);
        fuzzing::packets(&data);
        // Payloads on their topics, mutated as bytes and as JSON.
        let i = random.below(payloads.len());
        let bytes = random.mutate(payloads[i].clone(), &payloads);
        let topic = random.mutate(topics[i].clone(), &topics);
        let mut message = topic.clone();
        message.push(0);
        message.extend_from_slice(&bytes);
        fuzzing::message(&message);
        let value: Value = serde_json::from_slice(&payloads[i]).unwrap();
        let value = random.mutate_json(value);
        let value = random.mutate_json(value);
        if let Some(topic) = Topic::parse("mitcad", &String::from_utf8_lossy(&topics[i]))
            && messages::read(&topic, value.to_string().as_bytes()).is_ok()
        {
            read += 1;
        }
    }
    // Some mutations keep a message valid (an optional field removed).
    assert!(read > 0);
}
