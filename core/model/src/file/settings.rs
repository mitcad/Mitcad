// SPDX-License-Identifier: MIT
//! A project's settings (mitcad#89): what everyone who works on the
//! project shares, in its marker file `.mitcad/project.json` (versioned),
//! and what is set on this computer only, in `.mitcad/local/`.
//!
//! - **Shared** ([`SharedSettings`]): edit locks (on or off, the idle time
//!   and the poll interval) and live updates (an MQTT broker and a topic
//!   prefix). A marker without them means locks on with the defaults and no
//!   live updates.
//! - **This computer** ([`LocalSettings`]): live updates for this project
//!   (the project's broker, off, or another broker: `mqtt.json`) and sync
//!   (send each version at once, how often to check the remote:
//!   `sync.json`), each value unset meaning the application's default.
//!
//! The marker of a cloned project is written by anyone who can push to its
//! remote, so reading is defensive: at most [`MAX_SETTINGS_SIZE`] bytes,
//! numbers clamped to their bounds, a broker that is not a plain
//! `mqtt://` or `mqtts://` host and port dropped, each problem reported.
//! Writing keeps the marker's other fields.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value, json};

use super::project::{LOCAL_DIR, PROJECT_MARKER, write_atomically};

/// The file of the live-updates setting of this computer, relative to the
/// project's folder.
pub const LOCAL_LIVE: &str = ".mitcad/local/mqtt.json";
/// The file of the sync settings of this computer, relative to the
/// project's folder.
pub const LOCAL_SYNC: &str = ".mitcad/local/sync.json";
/// Settings files larger than this are not read.
pub const MAX_SETTINGS_SIZE: u64 = 64 * 1024;

/// The bounds of the idle time of an edit lock, in minutes.
pub const IDLE_MINUTES: (u32, u32) = (1, 120);
/// The bounds of the poll interval of edit locks, in seconds.
pub const POLL_SECONDS: (u32, u32) = (5, 600);
/// The bounds of the interval of checking the remote, in minutes (0:
/// never).
pub const CHECK_MINUTES: (u32, u32) = (0, 1440);
/// The topic prefix when none is given.
pub const DEFAULT_PREFIX: &str = "mitcad";

/// Edit locks of a project's designs (mitcad#89).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditLocks {
    pub enabled: bool,
    /// Minutes without activity after which a holder's lock is released or
    /// marked idle.
    pub idle_minutes: u32,
    /// Seconds between two reads of the remote's lock refs.
    pub poll_seconds: u32,
}

impl Default for EditLocks {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_minutes: 10,
            poll_seconds: 10,
        }
    }
}

/// The address of an MQTT broker: `mqtts://host:port` (TLS) or
/// `mqtt://host:port` (plain text); the port defaults to 8883 and 1883.
/// No user information, path, query or fragment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BrokerAddress {
    pub tls: bool,
    /// A host name, an IPv4 address or an IPv6 address (without brackets).
    pub host: String,
    pub port: u16,
}

impl BrokerAddress {
    /// Reads an address; the error says what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let bad = |why: &str| format!("'{text}' is not a broker address: {why}");
        let (tls, rest) = if let Some(rest) = strip_prefix_ignore_case(text, "mqtts://") {
            (true, rest)
        } else if let Some(rest) = strip_prefix_ignore_case(text, "mqtt://") {
            (false, rest)
        } else {
            return Err(bad("it must start with mqtts:// or mqtt://"));
        };
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        if rest.contains(['@', '/', '?', '#', ' ', '\\']) {
            return Err(bad(
                "only a host and a port, without a user, a path or a query",
            ));
        }
        let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
            let (host, after) = inner
                .split_once(']')
                .ok_or_else(|| bad("an IPv6 address needs its closing ]"))?;
            if host.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(bad("not an IPv6 address in [ ]"));
            }
            let port = match after.strip_prefix(':') {
                Some(port) => Some(port),
                None if after.is_empty() => None,
                None => return Err(bad("text after the IPv6 address")),
            };
            (host.to_owned(), port)
        } else {
            let (host, port) = match rest.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (rest, None),
            };
            if !valid_host(host) {
                return Err(bad("not a host name or an IPv4 address"));
            }
            (host.to_ascii_lowercase(), port)
        };
        let port = match port {
            None => {
                if tls {
                    8883
                } else {
                    1883
                }
            }
            // Digits only, without leading zeros ("+80" and "080" parse).
            Some(text) => match text.parse::<u16>() {
                Ok(port) if port > 0 && port.to_string() == text => port,
                _ => return Err(bad("the port is a number from 1 to 65535")),
            },
        };
        Ok(Self { tls, host, port })
    }

    /// The host as it goes into an address (IPv6 in brackets).
    pub fn host_in_url(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    (text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &text[prefix.len()..])
}

/// A host name (labels of letters, digits and `-`, not starting or ending
/// with `-`, at most 63 characters each, 253 in all) or an IPv4 address.
fn valid_host(host: &str) -> bool {
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) && !host
        .split('.')
        .all(|label| label.chars().all(|c| c.is_ascii_digit()))
}

impl fmt::Display for BrokerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scheme = if self.tls { "mqtts" } else { "mqtt" };
        write!(f, "{scheme}://{}:{}", self.host_in_url(), self.port)
    }
}

/// Whether `prefix` is a topic prefix: 1 to 64 characters of letters,
/// digits, `-`, `_` and `/`, not starting or ending with `/`, without
/// `//` (MQTT's wildcards `+` and `#` are never in it).
pub fn valid_prefix(prefix: &str) -> bool {
    !prefix.is_empty()
        && prefix.len() <= 64
        && !prefix.starts_with('/')
        && !prefix.ends_with('/')
        && !prefix.contains("//")
        && prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/'))
}

/// Live updates through an MQTT broker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveUpdates {
    pub broker: BrokerAddress,
    pub prefix: String,
}

impl LiveUpdates {
    fn to_json(&self) -> Value {
        json!({"broker": self.broker.to_string(), "prefix": self.prefix})
    }

    /// Reads `{"broker", "prefix"}`; the prefix defaults to
    /// [`DEFAULT_PREFIX`].
    fn from_json(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or("live updates must be an object with \"broker\" and \"prefix\"")?;
        let broker = object
            .get("broker")
            .and_then(Value::as_str)
            .ok_or("live updates need a \"broker\" address")?;
        let broker = BrokerAddress::parse(broker)?;
        let prefix = match object.get("prefix") {
            None | Some(Value::Null) => DEFAULT_PREFIX.to_owned(),
            Some(Value::String(prefix)) if valid_prefix(prefix) => prefix.clone(),
            Some(other) => {
                return Err(format!(
                    "{other} is not a topic prefix (letters, digits, -, _ and /)"
                ));
            }
        };
        Ok(Self { broker, prefix })
    }
}

/// What everyone in a project shares: kept in its marker, versioned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedSettings {
    pub edit_locks: EditLocks,
    /// None: no live updates.
    pub live_updates: Option<LiveUpdates>,
}

impl SharedSettings {
    /// `{"edit_locks": {"enabled", "idle_minutes", "poll_seconds"},
    /// "live_updates": {"broker", "prefix"} | null}`.
    pub fn to_json(&self) -> Value {
        json!({
            "edit_locks": {
                "enabled": self.edit_locks.enabled,
                "idle_minutes": self.edit_locks.idle_minutes,
                "poll_seconds": self.edit_locks.poll_seconds,
            },
            "live_updates": self.live_updates.as_ref().map(LiveUpdates::to_json),
        })
    }

    /// Reads the shared settings from an object with `edit_locks` and
    /// `live_updates` (a marker, or a command's fields). Missing values
    /// are the defaults. `strict`: a value out of bounds or a broker that
    /// cannot be used is an error (a command); otherwise numbers are
    /// clamped and a bad broker dropped, each reported (a marker someone
    /// else wrote).
    pub fn from_json(value: &Value, strict: bool) -> Result<(Self, Vec<String>), String> {
        let mut problems = Vec::new();
        let mut settings = Self::default();
        if let Some(locks) = value.get("edit_locks").filter(|v| !v.is_null()) {
            let locks = locks
                .as_object()
                .ok_or("\"edit_locks\" must be an object")?;
            if let Some(enabled) = locks.get("enabled") {
                settings.edit_locks.enabled = enabled
                    .as_bool()
                    .ok_or("\"enabled\" of edit locks must be true or false")?;
            }
            let mut number = |name: &str, bounds: (u32, u32), target: &mut u32| {
                let Some(value) = locks.get(name) else {
                    return Ok(());
                };
                let n = value
                    .as_u64()
                    .ok_or(format!("\"{name}\" of edit locks must be a whole number"))?;
                let clamped = n.clamp(u64::from(bounds.0), u64::from(bounds.1));
                if clamped != n {
                    let problem = format!(
                        "\"{name}\" of edit locks is {n}, outside {} to {}",
                        bounds.0, bounds.1
                    );
                    if strict {
                        return Err(problem);
                    }
                    problems.push(format!("{problem}: {clamped} is used"));
                }
                *target = u32::try_from(clamped).expect("within u32");
                Ok::<(), String>(())
            };
            number(
                "idle_minutes",
                IDLE_MINUTES,
                &mut settings.edit_locks.idle_minutes,
            )?;
            number(
                "poll_seconds",
                POLL_SECONDS,
                &mut settings.edit_locks.poll_seconds,
            )?;
        }
        match value.get("live_updates") {
            None | Some(Value::Null) => {}
            Some(live) => match LiveUpdates::from_json(live) {
                Ok(live) => settings.live_updates = Some(live),
                Err(problem) if strict => return Err(problem),
                Err(problem) => problems.push(format!("live updates are off: {problem}")),
            },
        }
        Ok((settings, problems))
    }
}

/// Live updates for a project on this computer (`mqtt.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LiveHere {
    /// The project's broker, if it has one.
    #[default]
    Project,
    Off,
    /// Another broker, which the project's files do not show.
    Broker(LiveUpdates),
}

/// Sync of a project on this computer (`sync.json`); None: the
/// application's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncHere {
    pub send_at_once: Option<bool>,
    /// Minutes between checks of the remote for newer versions; 0: never.
    pub check_minutes: Option<u32>,
}

/// What is set for a project on this computer only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalSettings {
    pub live: LiveHere,
    pub sync: SyncHere,
}

impl LocalSettings {
    /// `{"live": {"mode": "project" | "off" | "broker", "broker",
    /// "prefix"}, "sync": {"send_at_once", "check_minutes"}}`.
    pub fn to_json(&self) -> Value {
        let live = match &self.live {
            LiveHere::Project => json!({"mode": "project"}),
            LiveHere::Off => json!({"mode": "off"}),
            LiveHere::Broker(live) => {
                let mut value = live.to_json();
                value["mode"] = json!("broker");
                value
            }
        };
        json!({
            "live": live,
            "sync": {
                "send_at_once": self.sync.send_at_once,
                "check_minutes": self.sync.check_minutes,
            },
        })
    }

    /// Reads what [`LocalSettings::to_json`] writes; missing parts are the
    /// defaults. `strict` as for [`SharedSettings::from_json`].
    pub fn from_json(value: &Value, strict: bool) -> Result<(Self, Vec<String>), String> {
        let mut problems = Vec::new();
        let mut settings = Self::default();
        if let Some(live) = value.get("live").filter(|v| !v.is_null()) {
            settings.live = live_here(live, strict, &mut problems)?;
        }
        if let Some(sync) = value.get("sync").filter(|v| !v.is_null()) {
            settings.sync = sync_here(sync, strict, &mut problems)?;
        }
        Ok((settings, problems))
    }
}

fn live_here(value: &Value, strict: bool, problems: &mut Vec<String>) -> Result<LiveHere, String> {
    match value.get("mode").and_then(Value::as_str) {
        None | Some("project") => Ok(LiveHere::Project),
        Some("off") => Ok(LiveHere::Off),
        Some("broker") => match LiveUpdates::from_json(value) {
            Ok(live) => Ok(LiveHere::Broker(live)),
            Err(problem) if strict => Err(problem),
            Err(problem) => {
                problems.push(format!("this computer's broker is not used: {problem}"));
                Ok(LiveHere::Project)
            }
        },
        Some(other) => Err(format!(
            "live updates' mode is project, off or broker, not '{other}'"
        )),
    }
}

fn sync_here(value: &Value, strict: bool, problems: &mut Vec<String>) -> Result<SyncHere, String> {
    let mut sync = SyncHere::default();
    match value.get("send_at_once") {
        None | Some(Value::Null) => {}
        Some(Value::Bool(send)) => sync.send_at_once = Some(*send),
        Some(_) => return Err("\"send_at_once\" must be true, false or null".to_owned()),
    }
    match value.get("check_minutes") {
        None | Some(Value::Null) => {}
        Some(minutes) => {
            let n = minutes
                .as_u64()
                .ok_or("\"check_minutes\" must be a whole number or null")?;
            let clamped = n.min(u64::from(CHECK_MINUTES.1));
            if clamped != n {
                let problem = format!("\"check_minutes\" is {n}, more than {}", CHECK_MINUTES.1);
                if strict {
                    return Err(problem);
                }
                problems.push(format!("{problem}: {clamped} is used"));
            }
            sync.check_minutes = Some(u32::try_from(clamped).expect("within u32"));
        }
    }
    Ok(sync)
}

/// A project's settings, shared and of this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectSettings {
    pub shared: SharedSettings,
    pub local: LocalSettings,
}

impl ProjectSettings {
    /// The settings of the project in folder `root`. Unreadable or broken
    /// files give the defaults; what was not used is in the problems.
    pub fn read(root: &Path) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let mut settings = Self::default();
        if let Some(marker) = read_object(&root.join(PROJECT_MARKER), &mut problems) {
            match SharedSettings::from_json(&marker, false) {
                Ok((shared, more)) => {
                    settings.shared = shared;
                    problems.extend(more);
                }
                Err(problem) => problems.push(format!("{PROJECT_MARKER}: {problem}")),
            }
        }
        let live = read_object(&root.join(LOCAL_LIVE), &mut problems);
        let sync = read_object(&root.join(LOCAL_SYNC), &mut problems);
        let local = json!({"live": live, "sync": sync});
        match LocalSettings::from_json(&local, false) {
            Ok((local, more)) => {
                settings.local = local;
                problems.extend(more);
            }
            Err(problem) => problems.push(format!("{LOCAL_DIR}: {problem}")),
        }
        (settings, problems)
    }

    /// The live updates this computer uses for the project: its own broker,
    /// else the project's, unless they are off here.
    pub fn live_updates(&self) -> Option<&LiveUpdates> {
        match &self.local.live {
            LiveHere::Project => self.shared.live_updates.as_ref(),
            LiveHere::Off => None,
            LiveHere::Broker(live) => Some(live),
        }
    }
}

/// A settings file as a JSON object; None when it is missing, too large or
/// not an object (the last two reported).
fn read_object(path: &Path, problems: &mut Vec<String>) -> Option<Value> {
    let size = match fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return None,
    };
    let name = path.display();
    if size > MAX_SETTINGS_SIZE {
        problems.push(format!(
            "{name} is not read: {size} bytes, more than {MAX_SETTINGS_SIZE}"
        ));
        return None;
    }
    let text = match fs::read(path) {
        Ok(text) => text,
        Err(e) => {
            problems.push(format!("{name} cannot be read: {e}"));
            return None;
        }
    };
    match serde_json::from_slice::<Value>(&text) {
        Ok(value) if value.is_object() => Some(value),
        Ok(_) => {
            problems.push(format!("{name} is not a JSON object"));
            None
        }
        Err(e) => {
            problems.push(format!("{name} is not valid JSON: {e}"));
            None
        }
    }
}

/// Writes the shared settings into the marker of the project in folder
/// `root`, keeping its other fields; a lock setting equal to the default
/// is written too, so that the file says what is used. Returns whether the
/// file changed.
pub fn write_shared(root: &Path, settings: &SharedSettings) -> io::Result<bool> {
    let path = root.join(PROJECT_MARKER);
    let old = fs::read(&path)?;
    let mut marker: Map<String, Value> = serde_json::from_slice::<Value>(&old)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    marker
        .entry("format")
        .or_insert_with(|| json!("mitcad-project"));
    marker.entry("version").or_insert_with(|| json!(1));
    let value = settings.to_json();
    marker.insert("edit_locks".to_owned(), value["edit_locks"].clone());
    match &settings.live_updates {
        Some(_) => {
            marker.insert("live_updates".to_owned(), value["live_updates"].clone());
        }
        None => {
            marker.remove("live_updates");
        }
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(marker)).expect("serializable");
    text.push('\n');
    if text.as_bytes() == old.as_slice() {
        return Ok(false);
    }
    write_atomically(&path, text.as_bytes())?;
    Ok(true)
}

/// Writes this computer's settings of the project in folder `root`
/// (`mqtt.json` and `sync.json`; a default setting removes its file).
pub fn write_local(root: &Path, settings: &LocalSettings) -> io::Result<()> {
    let value = settings.to_json();
    let files = [
        (
            LOCAL_LIVE,
            &value["live"],
            settings.live == LiveHere::Project,
        ),
        (
            LOCAL_SYNC,
            &value["sync"],
            settings.sync == SyncHere::default(),
        ),
    ];
    for (name, content, default) in files {
        let path = root.join(name);
        if default {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        } else {
            let mut text = serde_json::to_string_pretty(content).expect("serializable");
            text.push('\n');
            write_atomically(&path, text.as_bytes())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_addresses() {
        let a = BrokerAddress::parse("mqtts://Broker.Example.com:8884").unwrap();
        assert_eq!(a.to_string(), "mqtts://broker.example.com:8884");
        assert!(a.tls);
        assert_eq!(
            BrokerAddress::parse("mqtt://10.0.0.5").unwrap().to_string(),
            "mqtt://10.0.0.5:1883"
        );
        assert_eq!(
            BrokerAddress::parse("MQTTS://host/").unwrap().to_string(),
            "mqtts://host:8883"
        );
        assert_eq!(
            BrokerAddress::parse("mqtts://[::1]:9000")
                .unwrap()
                .to_string(),
            "mqtts://[::1]:9000"
        );
        for bad in [
            "",
            "broker.example.com",
            "https://broker.example.com",
            "mqtts://user:pass@broker.example.com",
            "mqtts://broker.example.com/topic",
            "mqtts://broker.example.com:0",
            "mqtts://broker.example.com:65536",
            "mqtts://broker.example.com:+80",
            "mqtts://broker.example.com:080",
            "mqtts://-bad.example.com",
            "mqtts://bad_host",
            "mqtts://[::1",
            "mqtts://[not-ipv6]",
            "mqtts://999.1.1.1",
            "mqtts://exa mple.com",
        ] {
            assert!(BrokerAddress::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn prefixes() {
        for good in ["mitcad", "team-a/mitcad", "a_b"] {
            assert!(valid_prefix(good), "{good}");
        }
        for bad in [
            "",
            "/a",
            "a/",
            "a//b",
            "a+b",
            "a#",
            "a b",
            "\u{e9}",
            &"a".repeat(65),
        ] {
            assert!(!valid_prefix(bad), "{bad}");
        }
    }

    #[test]
    fn shared_settings_defaults_and_clamping() {
        let (settings, problems) = SharedSettings::from_json(&json!({}), true).unwrap();
        assert_eq!(settings, SharedSettings::default());
        assert!(settings.edit_locks.enabled);
        assert!(problems.is_empty());

        let marker = json!({"format": "mitcad-project", "version": 1,
            "edit_locks": {"enabled": false, "idle_minutes": 100000, "poll_seconds": 1},
            "live_updates": {"broker": "mqtts://user@evil.example.com"}});
        let (settings, problems) = SharedSettings::from_json(&marker, false).unwrap();
        assert!(!settings.edit_locks.enabled);
        assert_eq!(settings.edit_locks.idle_minutes, 120);
        assert_eq!(settings.edit_locks.poll_seconds, 5);
        assert_eq!(settings.live_updates, None);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(SharedSettings::from_json(&marker, true).is_err());

        let (settings, _) = SharedSettings::from_json(
            &json!({"live_updates": {"broker": "mqtts://b.example.com"}}),
            true,
        )
        .unwrap();
        let live = settings.live_updates.unwrap();
        assert_eq!(live.prefix, DEFAULT_PREFIX);
        assert_eq!(live.broker.port, 8883);
        assert!(SharedSettings::from_json(&json!({"edit_locks": 3}), false).is_err());
    }

    #[test]
    fn local_settings_round_trip() {
        let local = LocalSettings {
            live: LiveHere::Broker(LiveUpdates {
                broker: BrokerAddress::parse("mqtt://lan-broker:1884").unwrap(),
                prefix: "office".to_owned(),
            }),
            sync: SyncHere {
                send_at_once: Some(false),
                check_minutes: Some(0),
            },
        };
        let (read, problems) = LocalSettings::from_json(&local.to_json(), true).unwrap();
        assert_eq!(read, local);
        assert!(problems.is_empty());
        let (read, problems) =
            LocalSettings::from_json(&json!({"sync": {"check_minutes": 99999}}), false).unwrap();
        assert_eq!(read.sync.check_minutes, Some(CHECK_MINUTES.1));
        assert_eq!(problems.len(), 1);
        assert!(LocalSettings::from_json(&json!({"live": {"mode": "maybe"}}), false).is_err());
    }

    #[test]
    fn read_and_write_in_a_project() {
        let dir = std::env::temp_dir().join(format!(
            "mitcad-settings-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&dir);
        let (project, _) = super::super::Project::init(&dir).unwrap();
        let root = project.root();
        let (settings, problems) = ProjectSettings::read(root);
        assert_eq!(settings, ProjectSettings::default());
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(settings.live_updates(), None);

        let shared = SharedSettings {
            edit_locks: EditLocks {
                enabled: true,
                idle_minutes: 15,
                poll_seconds: 30,
            },
            live_updates: Some(LiveUpdates {
                broker: BrokerAddress::parse("mqtts://broker.example.com").unwrap(),
                prefix: "mitcad".to_owned(),
            }),
        };
        assert!(write_shared(root, &shared).unwrap());
        assert!(!write_shared(root, &shared).unwrap(), "unchanged");
        let marker: Value =
            serde_json::from_slice(&fs::read(root.join(PROJECT_MARKER)).unwrap()).unwrap();
        assert_eq!(marker["format"], "mitcad-project");
        assert_eq!(marker["version"], 1);
        assert_eq!(marker["edit_locks"]["idle_minutes"], 15);

        let local = LocalSettings {
            live: LiveHere::Off,
            sync: SyncHere::default(),
        };
        write_local(root, &local).unwrap();
        assert!(root.join(LOCAL_LIVE).is_file());
        assert!(!root.join(LOCAL_SYNC).exists());
        let (settings, problems) = ProjectSettings::read(root);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(settings.shared, shared);
        assert_eq!(settings.local, local);
        assert_eq!(settings.live_updates(), None, "off here");

        write_local(root, &LocalSettings::default()).unwrap();
        assert!(!root.join(LOCAL_LIVE).exists());
        let (settings, _) = ProjectSettings::read(root);
        assert_eq!(settings.live_updates(), shared.live_updates.as_ref());

        write_shared(root, &SharedSettings::default()).unwrap();
        let marker: Value =
            serde_json::from_slice(&fs::read(root.join(PROJECT_MARKER)).unwrap()).unwrap();
        assert!(marker.get("live_updates").is_none());

        fs::write(root.join(PROJECT_MARKER), vec![b' '; 70 * 1024]).unwrap();
        let (settings, problems) = ProjectSettings::read(root);
        assert_eq!(settings.shared, SharedSettings::default());
        assert_eq!(problems.len(), 1, "{problems:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Markers someone else wrote, mutated and cut at random (seeded, so
    /// the same each run): each is read or refused, never a panic, and
    /// what is read is within the bounds.
    #[test]
    fn mutated_markers_are_read_or_refused() {
        let sample = br#"{"format": "mitcad-project", "version": 1,
            "edit_locks": {"enabled": true, "idle_minutes": 15, "poll_seconds": 30},
            "live_updates": {"broker": "mqtts://broker.example.com:8883", "prefix": "team/mitcad"},
            "live": {"mode": "broker", "broker": "mqtt://[::1]:1883", "prefix": "a"},
            "sync": {"send_at_once": false, "check_minutes": 30}}"#;
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let table = b"{}[]\":,0123456789-e.tfn x/";
        let mut read = 0;
        for _ in 0..5000 {
            let mut bytes = sample.to_vec();
            for _ in 0..(next() % 6) {
                let at = (next() as usize) % bytes.len();
                bytes[at] = table[(next() as usize) % table.len()];
            }
            let cut = (next() as usize) % (bytes.len() + 1);
            let Ok(value) = serde_json::from_slice::<Value>(&bytes[..cut.max(1)]) else {
                continue;
            };
            read += 1;
            if let Ok((shared, _)) = SharedSettings::from_json(&value, false) {
                let locks = shared.edit_locks;
                assert!((IDLE_MINUTES.0..=IDLE_MINUTES.1).contains(&locks.idle_minutes));
                assert!((POLL_SECONDS.0..=POLL_SECONDS.1).contains(&locks.poll_seconds));
                if let Some(live) = shared.live_updates {
                    assert!(valid_prefix(&live.prefix));
                }
            }
            let _ = SharedSettings::from_json(&value, true);
            let _ = LocalSettings::from_json(&value, false);
            let _ = LocalSettings::from_json(&value, true);
        }
        assert!(read > 0);
    }
}
