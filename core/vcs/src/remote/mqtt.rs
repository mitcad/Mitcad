// SPDX-License-Identifier: MIT
//! Live updates (mitcad#89): an MQTT 3.1.1 client, all in Rust, that
//! tells the projects open in the application at once about edit locks,
//! requests for them, new versions and who has a design open.
//!
//! It is only an accelerator: every message makes the application poll git
//! at once, and only the lock refs on the remote grant a lock. Nothing a
//! broker or another user sends is read by C or C++: the socket
//! (`std::net`, blocking with timeouts, a thread per connection, no async
//! runtime), TLS (rustls with ring; the broker's certificate checked by
//! rustls-webpki against the system's root certificates), the packets
//! ([`packet`]) and the payloads ([`messages`], [`check`]) are read here,
//! and the application gets checked events as JSON.
//!
//! - **One connection per broker, user and prefix** ([`LiveHub`]), shared
//!   by every open project on it; each project subscribes to
//!   `<prefix>/<project id>/#` and unsubscribes when it closes, the project
//!   id being the repository's first commit. The connection's will marks
//!   the session `offline` at `<prefix>/sessions/<session>`; a clean
//!   disconnect clears it.
//! - **Credentials only over TLS**: a password with `mqtt://` is refused
//!   before anything is sent; `mqtt://` without one works, with a warning.
//! - **Untrusted input** (mitcad#89, "Untrusted input"): a packet's
//!   remaining length at most [`MAX_PACKET`] (checked before the packet is
//!   read; larger: disconnect), at most [`MAX_MESSAGES_PER_SECOND`]
//!   messages a second per connection (more: disconnect and reconnect
//!   after the back-off; the retained copies after a subscribe count apart,
//!   [`MAX_RETAINED_PER_SECOND`]), at most [`MAX_RETAINED_PER_FILE`] retained
//!   messages per file; topics only under the project's prefix and of the
//!   known forms; payloads read into typed structures of a known format and
//!   version, fields checked, text for people cleaned. A message that does
//!   not pass is dropped and counted (`status`, `dropped` events), never an
//!   error.
//! - MQTT 3.1.1 over TLS (`mqtts://`, port 8883) or plain TCP (`mqtt://`,
//!   1883); CONNECT with the will, user name and password; SUBSCRIBE and
//!   UNSUBSCRIBE; PUBLISH QoS 0 and 1 with PUBACK, sent again after a
//!   reconnect and when unacknowledged; retained messages; keep-alive
//!   [`KEEP_ALIVE`]; reconnect with an exponential back-off.
//!
//! The JSON commands and events are in `core/model/src/api/commands.md`,
//! "Live updates".

pub mod check;
mod connection;
mod events;
mod hub;
pub mod messages;
pub mod packet;
mod probe;
mod transport;

#[cfg(test)]
mod broker_tests;
#[cfg(test)]
mod fake_broker;
#[cfg(test)]
mod tests;

use std::fmt;
use std::time::Duration;

use serde::Serialize;

pub use hub::{HubOptions, LiveHub, Timing};

/// The largest remaining length of a packet read (64 KiB).
pub const MAX_PACKET: usize = 64 * 1024;
/// More messages than this in a second end the connection.
pub const MAX_MESSAGES_PER_SECOND: u32 = 200;
/// Retained copies, which a broker sends at once after a subscribe, count
/// apart from the others: more than this many in a second end the
/// connection.
pub const MAX_RETAINED_PER_SECOND: u32 = 2_000;
/// Retained messages (a lock and who has the design open) per file.
pub const MAX_RETAINED_PER_FILE: usize = 64;
/// Retained messages tracked per project; more are dropped.
pub const MAX_RETAINED_PER_PROJECT: usize = 10_000;
/// Sessions whose presence one connection follows.
pub const MAX_WATCHED_SESSIONS: usize = 512;
/// The keep-alive of a connection.
pub const KEEP_ALIVE: Duration = Duration::from_secs(30);

/// The kinds of failure of live updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveErrorClass {
    /// A command with a missing or wrong field.
    Invalid,
    /// A password over a connection without TLS.
    Insecure,
    /// The broker cannot be reached, or the connection broke.
    Network,
    /// The broker did not answer in time.
    TimedOut,
    /// The broker's certificate is not trusted (or no roots were found).
    Certificate,
    /// Another TLS failure.
    Tls,
    /// The broker refused the user name or password, or the access.
    AuthFailed,
    /// The broker refused the connection for another reason.
    Refused,
    /// The broker broke MQTT's rules (a packet too large, malformed).
    Protocol,
    /// The broker sent more messages than allowed.
    Flood,
    /// The hub or the connection is closed.
    Closed,
    /// A failure inside Mitcad.
    Internal,
}

impl LiveErrorClass {
    /// Whether connecting again cannot help until something changes (the
    /// password, the certificate authorities, the broker's settings).
    pub fn is_final(self) -> bool {
        matches!(
            self,
            LiveErrorClass::Invalid
                | LiveErrorClass::Insecure
                | LiveErrorClass::Certificate
                | LiveErrorClass::AuthFailed
                | LiveErrorClass::Refused
        )
    }
}

/// A failure of live updates: its class and a message for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveError {
    pub class: LiveErrorClass,
    pub message: String,
}

impl LiveError {
    pub fn new(class: LiveErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(LiveErrorClass::Invalid, message)
    }
}

impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LiveError {}

/// Entry points for fuzzing (`core/vcs/fuzz`): the packet decoder and the
/// payload readers on arbitrary bytes. They must never panic.
#[doc(hidden)]
pub mod fuzzing {
    use super::messages::{self, Topic};
    use super::packet::Decoder;

    /// Decodes `data` as a stream of packets until it ends or fails.
    pub fn packets(data: &[u8]) {
        let mut decoder = Decoder::new(super::MAX_PACKET);
        for chunk in data.chunks(97) {
            decoder.push(chunk);
            loop {
                match decoder.next_packet() {
                    Ok(Some(packet)) => {
                        // What was read is written again without a failure.
                        let _ = packet.encode();
                    }
                    Ok(None) => break,
                    Err(_) => return,
                }
            }
        }
    }

    /// Reads `data` as a topic name (up to the first 0 byte) and a payload.
    pub fn message(data: &[u8]) {
        let (topic, payload) = match data.iter().position(|&b| b == 0) {
            Some(at) => (&data[..at], &data[at + 1..]),
            None => (data, &[][..]),
        };
        let topic = String::from_utf8_lossy(topic);
        if let Some(topic) = Topic::parse("mitcad", &topic) {
            let _ = messages::read(&topic, payload);
        }
        // Every topic form with this payload.
        let project = "0123456789abcdef0123456789abcdef01234567";
        let file = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let session = "01234567-89ab-cdef-0123-456789abcdef";
        for topic in [
            Topic::Lock {
                project: project.to_owned(),
                file: file.to_owned(),
            },
            Topic::Requests {
                project: project.to_owned(),
                file: file.to_owned(),
            },
            Topic::Versions {
                project: project.to_owned(),
            },
            Topic::Open {
                project: project.to_owned(),
                file: file.to_owned(),
                session: session.to_owned(),
            },
            Topic::Session {
                session: session.to_owned(),
            },
        ] {
            let _ = messages::read(&topic, payload);
        }
        let text = String::from_utf8_lossy(data);
        let _ = super::check::clean_text(&text);
        let _ = super::check::parse_time(&text);
    }
}
