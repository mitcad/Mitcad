// SPDX-License-Identifier: MIT
//! SSH servers' identity (mitcad#89): the host keys a server offers, read
//! with `ssh-keyscan` as git's ssh would see them, compared with the keys
//! GitHub, GitLab and Codeberg publish and with `~/.ssh/known_hosts`, and a
//! key the user trusts appended there; the user's public key for a
//! service's SSH key page.
//!
//! `ssh-keyscan` is the git installation's (Git for Windows' `usr/bin`), else
//! the system's (`PATH`, Windows' OpenSSH); `MITCAD_SSH_KEYSCAN` names
//! another. It runs as git does: without a terminal, stdin closed, for at
//! most 20 s, its output limited to 64 KiB. What the server sends is
//! untrusted: only lines of known key types for the host asked about are
//! read, keys must be base64 of a key blob of their type, and fingerprints
//! are computed here. A key that differs from what a service publishes
//! cannot be trusted from Mitcad.

use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mitcad_model::Sha256;
use serde::Serialize;
use serde_json::Value;

use super::Context;
use crate::remote::{ErrorClass, RemoteError, failure, redact};
use crate::{VcsError, io_error};

/// The host key types read (what OpenSSH servers offer).
const KEY_TYPES: [&str; 5] = [
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "ssh-rsa",
];
/// The public key types of a user's key.
const USER_KEY_TYPES: [&str; 7] = [
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "ssh-rsa",
    "sk-ssh-ed25519@openssh.com",
    "sk-ecdsa-sha2-nistp256@openssh.com",
];
/// `ssh-keyscan`'s output read at most; more stops it.
pub const MAX_OUTPUT: usize = 64 * 1024;
/// A key blob's size at most (an RSA key of 16384 bits is about 2 KiB).
const MAX_KEY_BYTES: usize = 8 * 1024;
/// The keys of a host read at most.
const MAX_KEYS: usize = 16;
/// How long `ssh-keyscan` may run, and its own time per connection.
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: &str = "10";
/// `known_hosts` files larger than this are not read.
const MAX_KNOWN_HOSTS: u64 = 4 << 20;
/// A public key file larger than this is not read.
const MAX_PUBLIC_KEY: u64 = 16 * 1024;

/// The fingerprints (SHA-256 of the key, base64 without padding) of the
/// host keys services publish, for their SSH host names. A key a server
/// sends under such a name must be one of them.
const PUBLISHED: [(&str, &str, [&str; 3]); 3] = [
    // https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints
    (
        "github.com",
        "GitHub",
        [
            // RSA
            "SHA256:uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s",
            // ECDSA
            "SHA256:p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM",
            // Ed25519
            "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU",
        ],
    ),
    // https://docs.gitlab.com/user/gitlab_com/#ssh-host-keys-fingerprints
    (
        "gitlab.com",
        "GitLab",
        [
            // ECDSA
            "SHA256:HbW3g8zUjNSksFbqTiUWPWg2Bq1x8xdGUrliXFzSnUw",
            // ED25519
            "SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8",
            // RSA
            "SHA256:ROQFvPThGrW4RuWLoL9tq9I9zJ42fK4XywyRtbOz/EQ",
        ],
    ),
    // https://docs.codeberg.org/security/ssh-fingerprint/
    (
        "codeberg.org",
        "Codeberg",
        [
            // RSA
            "SHA256:6QQmYi4ppFS4/+zSZ5S4IU+4sa6rwvQ4PbhCtPEBekQ",
            // ECDSA
            "SHA256:T9FYDEHELhVkulEKKwge5aVhVTbqCW0MIRwAfpARs/E",
            // ED25519
            "SHA256:mIlxA9k46MmM6qdJOdMnAQpzGxF4WIVVL+fj+wZbw0g",
        ],
    ),
];

/// A host key a server offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostKey {
    #[serde(rename = "type")]
    pub kind: String,
    /// The key blob as base64.
    pub key: String,
    /// `SHA256:…`, as `ssh-keygen -l` shows it.
    pub fingerprint: String,
}

/// How a server's keys compare with those its service publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Published {
    /// Every key is one the service publishes.
    Verified,
    /// A key is not: it cannot be trusted from Mitcad.
    Mismatch,
}

/// What [`host_keys`] finds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostKeys {
    pub host: String,
    pub port: u16,
    pub keys: Vec<HostKey>,
    /// `GitHub`, `GitLab` or `Codeberg` for their host names.
    pub service: Option<String>,
    /// The keys compared with those the service publishes.
    pub published: Option<Published>,
    /// A key of the host is in `known_hosts`.
    pub known: bool,
    /// `known_hosts` has a key of the same type for the host that differs
    /// from the server's: the key changed (or someone is in between).
    pub changed: bool,
    /// The `known_hosts` file.
    pub known_hosts: Option<String>,
    /// Lines of the output that were not read (malformed, another host,
    /// another key type).
    pub dropped: usize,
}

/// A key trusted ([`trust_host_key`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trusted {
    pub path: String,
    /// False: it was there.
    pub written: bool,
}

/// The user's public key ([`ssh_public_key`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicKey {
    pub path: String,
    pub text: String,
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 with padding (`pad`) or without.
fn base64(data: &[u8], pad: bool) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
        }
        if pad {
            for _ in chunk.len()..3 {
                out.push('=');
            }
        }
    }
    out
}

/// Strict base64 with padding: a multiple of 4 characters of the standard
/// alphabet, `=` only at the end; None otherwise.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|v| v as u32);
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let chunks = bytes.len() / 4;
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index + 1 == chunks;
        let padding = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = if i >= 4 - padding { 0 } else { value(c)? };
            n = n << 6 | v;
        }
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&decoded[..3 - padding]);
    }
    Some(out)
}

/// The fingerprint of a key blob: `SHA256:` and the base64 of its SHA-256
/// without padding.
pub fn fingerprint(blob: &[u8]) -> String {
    format!("SHA256:{}", base64(&Sha256::of(blob).0, false))
}

/// A key blob's type: the SSH string it starts with.
fn blob_type(blob: &[u8]) -> Option<&str> {
    let length = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    std::str::from_utf8(blob.get(4..4 + length)?).ok()
}

/// A key of type `kind` as base64: its blob, checked (the right type at
/// its start, not too large).
fn key_blob(kind: &str, key: &str) -> Option<Vec<u8>> {
    if key.len() > MAX_KEY_BYTES * 2 {
        return None;
    }
    let blob = decode_base64(key)?;
    (blob.len() <= MAX_KEY_BYTES && blob.len() > 8 && blob_type(&blob) == Some(kind))
        .then_some(blob)
}

/// A host name or an IP address, lowercase; an error otherwise.
fn check_host(host: &str) -> Result<String, VcsError> {
    let host = host.trim();
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let valid = if bare.contains(':') {
        bare.parse::<std::net::Ipv6Addr>().is_ok()
    } else {
        !bare.is_empty()
            && bare.len() <= 253
            && bare.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && label
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            })
    };
    if valid {
        Ok(bare.to_ascii_lowercase())
    } else {
        Err(VcsError::Command(format!(
            "'{host}' is not a server's host name or address"
        )))
    }
}

/// How `known_hosts` and `ssh-keyscan` name a host at a port: `host`, or
/// `[host]:port` for another port than 22.
fn host_field(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

/// The host keys in `ssh-keyscan`'s output for `host` at `port`: lines of
/// known key types for that host with well-formed keys (at most 16,
/// without repeats), and how many lines were dropped. Output larger than
/// 64 KiB is an error.
pub fn parse_keyscan(
    output: &[u8],
    host: &str,
    port: u16,
) -> Result<(Vec<HostKey>, usize), String> {
    if output.len() > MAX_OUTPUT {
        return Err(format!(
            "the server's answer is larger than {} KiB",
            MAX_OUTPUT / 1024
        ));
    }
    let expected = host_field(host, port);
    let text = String::from_utf8_lossy(output);
    let mut keys: Vec<HostKey> = Vec::new();
    let mut dropped = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let read = match fields.as_slice() {
            [name, kind, key, ..]
                if name.eq_ignore_ascii_case(&expected) && KEY_TYPES.contains(kind) =>
            {
                key_blob(kind, key).map(|blob| HostKey {
                    kind: (*kind).to_owned(),
                    key: base64(&blob, true),
                    fingerprint: fingerprint(&blob),
                })
            }
            _ => None,
        };
        match read {
            Some(key) if keys.len() < MAX_KEYS && !keys.contains(&key) => keys.push(key),
            _ => dropped += 1,
        }
    }
    Ok((keys, dropped))
}

/// The service whose published keys apply to `host`, and those keys'
/// fingerprints.
fn service_of(host: &str) -> Option<(&'static str, [&'static str; 3])> {
    PUBLISHED
        .iter()
        .find(|(name, _, _)| *name == host)
        .map(|(_, service, keys)| (*service, *keys))
}

/// `keys` compared with what `host`'s service publishes.
fn compare_published(host: &str, keys: &[HostKey]) -> (Option<String>, Option<Published>) {
    let Some((service, published)) = service_of(host) else {
        return (None, None);
    };
    let state = if keys.is_empty() {
        None
    } else if keys
        .iter()
        .all(|key| published.contains(&key.fingerprint.as_str()))
    {
        Some(Published::Verified)
    } else {
        Some(Published::Mismatch)
    };
    (Some(service.to_owned()), state)
}

/// SHA-1 (`known_hosts`' hashed host names).
fn sha1(data: &[u8]) -> Option<[u8; 20]> {
    let mut hasher = gix::hash::hasher(gix::hash::Kind::Sha1);
    hasher.update(data);
    let id = hasher.try_finalize().ok()?;
    id.as_bytes().try_into().ok()
}

/// HMAC-SHA1 of `message` with `key` (at most 64 bytes).
fn hmac_sha1(key: &[u8], message: &[u8]) -> Option<[u8; 20]> {
    if key.len() > 64 {
        return None;
    }
    let mut block = [0u8; 64];
    block[..key.len()].copy_from_slice(key);
    let mut inner: Vec<u8> = block.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(message);
    let mut outer: Vec<u8> = block.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&sha1(&inner)?);
    sha1(&outer)
}

/// Whether a `known_hosts` pattern (with `*` and `?`) matches `name`.
fn glob(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            mark = n;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            n = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

/// Whether the host field of a `known_hosts` line (patterns separated by
/// commas, `!` negating, `|1|salt|hash` hashed) names `name`.
fn hosts_match(patterns: &str, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let mut matched = false;
    for pattern in patterns.split(',') {
        let (negated, pattern) = match pattern.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, pattern),
        };
        let hit = match pattern.strip_prefix("|1|") {
            Some(hashed) => hashed.split_once('|').is_some_and(|(salt, hash)| {
                match (decode_base64(salt), decode_base64(hash)) {
                    (Some(salt), Some(hash)) => {
                        hmac_sha1(&salt, name.as_bytes()).is_some_and(|mac| mac[..] == hash[..])
                    }
                    _ => false,
                }
            }),
            None => glob(pattern.to_ascii_lowercase().as_bytes(), name.as_bytes()),
        };
        if hit {
            if negated {
                return false;
            }
            matched = true;
        }
    }
    matched
}

/// Whether `known_hosts` (its text) has one of `keys` for `name`, and
/// whether it has a key of one of their types for `name` that is none of
/// them.
fn known_state(text: &str, name: &str, keys: &[HostKey]) -> (bool, bool) {
    let mut known = false;
    let mut others = std::collections::BTreeSet::new();
    let mut types_known = std::collections::BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        // Certificate authorities and revoked keys are left to ssh.
        let [hosts, kind, key, ..] = fields.as_slice() else {
            continue;
        };
        if hosts.starts_with('@') || !hosts_match(hosts, name) {
            continue;
        }
        if keys.iter().any(|k| k.kind == *kind && k.key == *key) {
            known = true;
            types_known.insert((*kind).to_owned());
        } else if keys.iter().any(|k| k.kind == *kind) {
            others.insert((*kind).to_owned());
        }
    }
    let changed = others.iter().any(|kind| !types_known.contains(kind));
    (known, changed)
}

/// The user's home folder.
fn home(context: &Context) -> Option<PathBuf> {
    if let Some(home) = &context.home {
        return Some(home.clone());
    }
    let variable = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    variable("HOME")
        .or_else(|| {
            if cfg!(windows) {
                variable("USERPROFILE")
            } else {
                None
            }
        })
        .map(PathBuf::from)
}

fn known_hosts_path(context: &Context) -> Result<PathBuf, VcsError> {
    home(context)
        .map(|home| home.join(".ssh").join("known_hosts"))
        .ok_or_else(|| {
            failure(
                ErrorClass::Other,
                "no home folder (HOME) for ~/.ssh/known_hosts",
            )
        })
}

/// `known_hosts`' text; empty when it is missing or too large.
fn read_known_hosts(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() <= MAX_KNOWN_HOSTS => fs::read(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// `ssh-keyscan`: [`Context::keyscan`], `MITCAD_SSH_KEYSCAN`, the git
/// installation's, `PATH`'s, Windows' OpenSSH.
fn find_keyscan(context: &Context) -> Result<PathBuf, VcsError> {
    if let Some(program) = &context.keyscan {
        return Ok(program.clone());
    }
    if let Some(program) = std::env::var_os("MITCAD_SSH_KEYSCAN").filter(|v| !v.is_empty()) {
        let program = PathBuf::from(program);
        return if program.is_file() {
            Ok(program)
        } else {
            Err(failure(
                ErrorClass::Other,
                format!(
                    "the ssh-keyscan set for Mitcad (MITCAD_SSH_KEYSCAN) is not there: {}",
                    program.display()
                ),
            ))
        };
    }
    let name = if cfg!(windows) {
        "ssh-keyscan.exe"
    } else {
        "ssh-keyscan"
    };
    let mut candidates = Vec::new();
    if let Ok(git) = context.git()
        && let Some(folder) = git.program().parent()
    {
        // Git for Windows: <root>\cmd\git.exe and <root>\usr\bin.
        for above in folder.ancestors().take(4) {
            candidates.push(above.join("usr").join("bin").join(name));
            candidates.push(above.join(name));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(
            std::env::split_paths(&path)
                .filter(|folder| folder.is_absolute())
                .map(|folder| folder.join(name)),
        );
    }
    if cfg!(windows)
        && let Some(system) = std::env::var_os("SystemRoot")
    {
        candidates.push(
            PathBuf::from(system)
                .join("System32")
                .join("OpenSSH")
                .join(name),
        );
    }
    candidates
        .into_iter()
        .find(|program| program.is_file())
        .ok_or_else(|| {
            failure(
                ErrorClass::Other,
                "ssh-keyscan was not found: it comes with git (Git for Windows) and with \
                 OpenSSH; install one of them, or set MITCAD_SSH_KEYSCAN",
            )
        })
}

/// What a program printed (its output limited) and whether it succeeded.
struct Limited {
    success: bool,
    stdout: Vec<u8>,
    stderr: String,
}

enum Event {
    Data(bool, Vec<u8>),
    Closed,
}

/// Runs `program` without a terminal and with stdin closed for at most
/// `timeout`, its output read up to `limit` bytes (more stops it).
fn run_limited(
    program: &Path,
    args: &[&str],
    timeout: Duration,
    limit: usize,
) -> Result<Limited, RemoteError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: the closure runs in the child between fork and exec and
        // only calls setsid(), which is async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = command.spawn().map_err(|e| {
        RemoteError::new(
            ErrorClass::Other,
            format!("cannot run {}: {e}", program.display()),
        )
    })?;
    let (sender, events) = mpsc::channel();
    for (out, pipe) in [
        (
            true,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
        (
            false,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
    ] {
        let Some(mut pipe) = pipe else { continue };
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sender.send(Event::Data(out, buffer[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = sender.send(Event::Closed);
        });
    }
    drop(sender);
    let stop = |child: &mut std::process::Child| {
        #[cfg(unix)]
        if let Ok(group) = libc::pid_t::try_from(child.id()) {
            // SAFETY: kill() with the negative id of the group it leads.
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    };
    let start = Instant::now();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut open = 2;
    while open > 0 {
        match events.recv_timeout(Duration::from_millis(20)) {
            Ok(Event::Data(true, bytes)) => {
                stdout.extend_from_slice(&bytes);
                if stdout.len() > limit {
                    stop(&mut child);
                    return Err(RemoteError::new(
                        ErrorClass::Other,
                        format!(
                            "{} printed more than {} KiB and was stopped",
                            program.display(),
                            limit / 1024
                        ),
                    ));
                }
            }
            Ok(Event::Data(false, bytes)) => {
                if stderr.len() < limit {
                    stderr.extend_from_slice(&bytes);
                }
            }
            Ok(Event::Closed) => open -= 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => open = 0,
        }
        if start.elapsed() > timeout {
            stop(&mut child);
            return Err(RemoteError::new(
                ErrorClass::TimedOut,
                format!(
                    "{} did not finish within {} s and was stopped",
                    program.display(),
                    timeout.as_secs()
                ),
            ));
        }
    }
    // Its output is closed; it ends, or is stopped at the time limit.
    let status = loop {
        let waited = child.try_wait().map_err(|e| {
            RemoteError::new(
                ErrorClass::Other,
                format!("cannot wait for {}: {e}", program.display()),
            )
        })?;
        if let Some(status) = waited {
            break status;
        }
        if start.elapsed() > timeout {
            stop(&mut child);
            return Err(RemoteError::new(
                ErrorClass::TimedOut,
                format!(
                    "{} did not finish within {} s and was stopped",
                    program.display(),
                    timeout.as_secs()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(Limited {
        success: status.success(),
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// The host keys the SSH server `host` at `port` offers (`ssh-keyscan`),
/// compared with those its service publishes and with `known_hosts`. A
/// server that gives none (unreachable, not SSH) is the error `network`.
pub fn host_keys(
    host: &str,
    port: u16,
    context: &Context,
    log: &mut Vec<String>,
) -> Result<HostKeys, VcsError> {
    let host = check_host(host)?;
    let program = find_keyscan(context)?;
    let port_text = port.to_string();
    let args = [
        "-T",
        CONNECT_TIMEOUT,
        "-p",
        &port_text,
        "-t",
        "rsa,ecdsa,ed25519",
        &host,
    ];
    let result = run_limited(&program, &args, SCAN_TIMEOUT, MAX_OUTPUT);
    log.push(redact(&format!(
        "ssh-keyscan {}: {}",
        args.join(" "),
        match &result {
            Ok(output) if output.success => "ok".to_owned(),
            Ok(_) => "failed".to_owned(),
            Err(error) => error.message.clone(),
        }
    )));
    let output = result?;
    let (keys, dropped) = parse_keyscan(&output.stdout, &host, port)
        .map_err(|problem| failure(ErrorClass::Other, problem))?;
    if keys.is_empty() {
        let mut error = RemoteError::new(
            ErrorClass::Network,
            format!(
                "{} sent no SSH host key: the server cannot be reached, or it is not an SSH \
                 server. Check the host name and the network.",
                host_field(&host, port)
            ),
        );
        error.detail = super::clean_text(&output.stderr, 2000);
        return Err(error.into());
    }
    let (service, published) = compare_published(&host, &keys);
    let path = known_hosts_path(context).ok();
    let (known, changed) = match &path {
        Some(path) => known_state(&read_known_hosts(path), &host_field(&host, port), &keys),
        None => (false, false),
    };
    Ok(HostKeys {
        host,
        port,
        keys,
        service,
        published,
        known,
        changed,
        known_hosts: path.map(|p| p.to_string_lossy().into_owned()),
        dropped,
    })
}

/// Appends the host key `kind` `key` of `host` at `port` to
/// `~/.ssh/known_hosts` (made, readable by the user only, when missing),
/// only when a new scan still gives it and it is one its service publishes
/// (a mismatch is never trusted from Mitcad). `written` false: it was
/// there.
pub fn trust_host_key(
    host: &str,
    port: u16,
    kind: &str,
    key: &str,
    context: &Context,
    log: &mut Vec<String>,
) -> Result<Trusted, VcsError> {
    let host = check_host(host)?;
    let blob = KEY_TYPES
        .contains(&kind)
        .then(|| key_blob(kind, key.trim()))
        .flatten()
        .ok_or_else(|| {
            VcsError::Command(format!("'{kind}' with that key is not an SSH host key"))
        })?;
    let key = base64(&blob, true);
    let scan = host_keys(&host, port, context, log)?;
    let name = host_field(&host, port);
    let Some(found) = scan.keys.iter().find(|k| k.kind == kind && k.key == key) else {
        return Err(failure(
            ErrorClass::HostKeyUnknown,
            format!(
                "{name} does not offer this {kind} key now: look at its keys again and compare \
                 their fingerprints"
            ),
        ));
    };
    if let Some((service, published)) = service_of(&host)
        && !published.contains(&found.fingerprint.as_str())
    {
        return Err(failure(
            ErrorClass::HostKeyUnknown,
            format!(
                "the key {name} sent ({}) is not one {service} publishes, so Mitcad does not \
                 trust it: the network may be redirected. Check the connection (another \
                 network, no proxy) and compare with {service}'s published fingerprints",
                found.fingerprint
            ),
        ));
    }
    let path = known_hosts_path(context)?;
    let text = read_known_hosts(&path);
    let (known, _) = known_state(&text, &name, std::slice::from_ref(found));
    if known {
        return Ok(Trusted {
            path: path.to_string_lossy().into_owned(),
            written: false,
        });
    }
    append_line(&path, &format!("{name} {kind} {key}\n"))?;
    Ok(Trusted {
        path: path.to_string_lossy().into_owned(),
        written: true,
    })
}

/// [`trust_host_key`] for every key a new scan of `host` at `port` gives
/// (`mitcad-cli host-keys --trust`): none is written when one is not what
/// its service publishes. `written`: a key was added.
pub fn trust_host_keys(
    host: &str,
    port: u16,
    context: &Context,
    log: &mut Vec<String>,
) -> Result<Trusted, VcsError> {
    let scan = host_keys(host, port, context, log)?;
    if let (Some(service), Some(Published::Mismatch)) = (&scan.service, scan.published) {
        return Err(failure(
            ErrorClass::HostKeyUnknown,
            format!(
                "a key {} sent is not one {service} publishes, so Mitcad does not trust it: the \
                 network may be redirected. Check the connection (another network, no proxy)",
                host_field(&scan.host, port)
            ),
        ));
    }
    let path = known_hosts_path(context)?;
    let name = host_field(&scan.host, port);
    let mut written = false;
    for key in &scan.keys {
        let (known, _) = known_state(&read_known_hosts(&path), &name, std::slice::from_ref(key));
        if !known {
            append_line(&path, &format!("{name} {} {}\n", key.kind, key.key))?;
            written = true;
        }
    }
    Ok(Trusted {
        path: path.to_string_lossy().into_owned(),
        written,
    })
}

/// Appends `line` to the file at `path`, after a line break when the file
/// does not end with one; the folder (and the file) made for the user only
/// when missing.
fn append_line(path: &Path, line: &str) -> Result<(), VcsError> {
    let folder = path.parent().unwrap_or(Path::new("."));
    if !folder.is_dir() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(folder)
            .map_err(|e| io_error("make the folder", folder, e))?;
    }
    let ends_with_break = match fs::read(path) {
        Ok(bytes) => bytes.is_empty() || bytes.ends_with(b"\n"),
        Err(_) => true,
    };
    let mut options = fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| io_error("open", path, e))?;
    let mut text = String::new();
    if !ends_with_break {
        text.push('\n');
    }
    text.push_str(line);
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| io_error("write", path, e))
}

/// The user's public key: the first of `~/.ssh/id_ed25519.pub`,
/// `id_ecdsa.pub` and `id_rsa.pub` that holds one.
pub fn ssh_public_key(context: &Context) -> Option<PublicKey> {
    let folder = home(context)?.join(".ssh");
    for name in ["id_ed25519.pub", "id_ecdsa.pub", "id_rsa.pub"] {
        let path = folder.join(name);
        if fs::metadata(&path).map_or(true, |m| m.len() > MAX_PUBLIC_KEY) {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let Some(line) = text.lines().map(str::trim).find(|line| !line.is_empty()) else {
            continue;
        };
        let mut fields = line.split_whitespace();
        let (Some(kind), Some(key)) = (fields.next(), fields.next()) else {
            continue;
        };
        if USER_KEY_TYPES.contains(&kind) && key_blob(kind, key).is_some() {
            return Some(PublicKey {
                path: path.to_string_lossy().into_owned(),
                text: super::clean_text(line, 16 * 1024),
            });
        }
    }
    None
}

/// `host_keys`' answer as text.
pub(crate) fn describe(out: &mut String, answer: &Value) {
    let host = answer["host"].as_str().unwrap_or("");
    let port = answer["port"].as_u64().unwrap_or(22);
    let _ = writeln!(out, "SSH host keys of {host} (port {port}):");
    for key in answer["keys"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "  {} {}",
            key["type"].as_str().unwrap_or(""),
            key["fingerprint"].as_str().unwrap_or("")
        );
    }
    let service = answer["service"].as_str();
    let _ = match (service, answer["published"].as_str()) {
        (Some(service), Some("verified")) => {
            writeln!(out, "Verified: the keys {service} publishes")
        }
        (Some(service), Some(_)) => writeln!(
            out,
            "warning: a key is not one {service} publishes: do not trust it (the network may \
             be redirected)"
        ),
        _ => writeln!(
            out,
            "Compare the fingerprints with those your server's administrator gives"
        ),
    };
    if answer["changed"].as_bool() == Some(true) {
        let _ = writeln!(
            out,
            "warning: known_hosts has another key of {host}: the server's key changed, or \
             someone is in between"
        );
    } else if answer["known"].as_bool() == Some(true) {
        let _ = writeln!(out, "Trusted already (in known_hosts)");
    }
}

/// A host name as `known_hosts` keeps it hashed: `|1|salt|hash`.
#[cfg(test)]
pub(crate) fn hashed_host(salt: &[u8], name: &str) -> String {
    let mac = hmac_sha1(salt, name.as_bytes()).expect("a short salt");
    format!("|1|{}|{}", base64(salt, true), base64(&mac, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let text = base64(data, true);
            if !data.is_empty() {
                assert_eq!(decode_base64(&text).unwrap(), data, "{text}");
            }
        }
        assert_eq!(base64(b"foobar", true), "Zm9vYmFy");
        assert_eq!(base64(b"fo", true), "Zm8=");
        assert_eq!(base64(b"fo", false), "Zm8");
        for bad in ["", "Zm8", "Zm=8", "Z===", "Zm8=Zm8=", "Zm 8", "Zm8*"] {
            assert_eq!(decode_base64(bad), None, "{bad}");
        }
    }

    #[test]
    fn hmac_sha1_of_the_rfc_2202_case_2() {
        let mac = hmac_sha1(b"Jefe", b"what do ya want for nothing?").unwrap();
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79");
    }

    /// The host keys the three services' servers send (`ssh-keyscan`,
    /// 2026-10-08), as their documentation lists them.
    const SCANNED: &str = "\
github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCj7ndNxQowgcQnjshcLrqPEiiphnt+VTTvDP6mHBL9j1aNUkY4Ue1gvwnGLVlOhGeYrnZaMgRK6+PKCUXaDbC7qtbW8gIkhL7aGCsOr/C56SJMy/BCZfxd1nWzAOxSDPgVsmerOBYfNqltV9/hWCqBywINIR+5dIg6JTJ72pcEpEjcYgXkE2YEFXV1JHnsKgbLWNlhScqb2UmyRkQyytRLtL+38TGxkxCflmO+5Z8CSSNY7GidjMIZ7Q4zMjA2n1nGrlTDkzwDCsw+wqFPGQA179cnfGWOWRVruj16z6XyvxvjJwbz0wQZ75XK5tKSb7FNyeIEs4TT4jk+S4dhPeAUC5y+bDYirYgM4GC7uEnztnZyaVWQ7B381AK4Qdrwt51ZqExKbQpTUNn+EjqoTwvqNj4kqx5QUCI0ThS/YkOxJCXmPUWZbhjpCg56i+2aB6CmK2JGhn57K5mj0MNdBXA4/WnwH6XoPWJzK5Nyu2zB3nAZp+S5hpQs+p1vN1/wsjk=
github.com ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg=
github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
gitlab.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQCsj2bNKTBSpIYDEGk9KxsGh3mySTRgMtXL583qmBpzeQ+jqCMRgBqB98u3z++J1sKlXHWfM9dyhSevkMwSbhoR8XIq/U0tCNyokEi/ueaBMCvbcTHhO7FcwzY92WK4Yt0aGROY5qX2UKSeOvuP4D6TPqKF1onrSzH9bx9XUf2lEdWT/ia1NEKjunUqu1xOB/StKDHMoX4/OKyIzuS0q/T1zOATthvasJFoPrAjkohTyaDUz2LN5JoH839hViyEG82yB+MjcFV5MU3N1l1QL3cVUCh93xSaua1N85qivl+siMkPGbO5xR/En4iEY6K2XPASUEMaieWVNTRCtJ4S8H+9
gitlab.com ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBFSMqzJeV9rUzU4kWitGjeR4PWSa29SPqJ1fVkhtj3Hw9xjLVXVYrU9QlYWrOLXBpQ6KWjbjTDTdDkoohFzgbEY=
gitlab.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAfuCHKVTjquxvt6CM6tdG4SLp1Btn/nOeHHE5UOzRdf
codeberg.org ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC8hZi7K1/2E2uBX8gwPRJAHvRAob+3Sn+y2hxiEhN0buv1igjYFTgFO2qQD8vLfU/HT/P/rqvEeTvaDfY1y/vcvQ8+YuUYyTwE2UaVU5aJv89y6PEZBYycaJCPdGIfZlLMmjilh/Sk8IWSEK6dQr+g686lu5cSWrFW60ixWpHpEVB26eRWin3lKYWSQGMwwKv4LwmW3ouqqs4Z4vsqRFqXJ/eCi3yhpT+nOjljXvZKiYTpYajqUC48IHAxTWugrKe1vXWOPxVXXMQEPsaIRc2hpK+v1LmfB7GnEGvF1UAKnEZbUuiD9PBEeD5a1MZQIzcoPWCrTxipEpuXQ5Tni4mN
codeberg.org ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBL2pDxWr18SoiDJCGZ5LmxPygTlPu+cCKSkpqkvCyQzl5xmIMeKNdfdBpfbCGDPoZQghePzFZkKJNR/v9Win3Sc=
codeberg.org ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIVIC02vnjFyL+I4RHfvIGNtOgJMe769VTF1VR4EB3ZB
";

    #[test]
    fn the_services_keys_are_the_published_ones() {
        for (host, service, published) in PUBLISHED {
            let (keys, dropped) = parse_keyscan(SCANNED.as_bytes(), host, 22).unwrap();
            assert_eq!((keys.len(), dropped), (3, 6), "{host}");
            let fingerprints: Vec<&str> = keys.iter().map(|k| k.fingerprint.as_str()).collect();
            for fingerprint in published {
                assert!(fingerprints.contains(&fingerprint), "{host}: {fingerprint}");
            }
            assert_eq!(
                compare_published(host, &keys),
                (Some(service.to_owned()), Some(Published::Verified))
            );
            // Another service's keys under this name are a mismatch.
            let other = PUBLISHED.iter().find(|(h, _, _)| *h != host).unwrap().0;
            let (theirs, _) = parse_keyscan(SCANNED.as_bytes(), other, 22).unwrap();
            assert_eq!(
                compare_published(host, &theirs).1,
                Some(Published::Mismatch)
            );
        }
    }

    #[test]
    fn globs_match_like_ssh() {
        assert!(glob(b"*.example.com", b"git.example.com"));
        assert!(glob(b"git?.example.com", b"git1.example.com"));
        assert!(!glob(b"*.example.com", b"example.com"));
        assert!(glob(b"*", b"anything"));
        assert!(!glob(b"a", b"b"));
    }
}
