// SPDX-License-Identifier: MIT
//! The contents of edit locks and requests (`lock.json`, `request.json`),
//! the names of their refs, and the checks of everything read from them.
//!
//! Lock and request refs are written by anyone who can push to the remote,
//! so what they hold is untrusted: a file larger than [`MAX_FILE_SIZE`] is
//! not parsed, the JSON is read into typed structures with a known
//! `format` and `version` (an unknown version is not guessed at), and every
//! field is checked: ids and commit ids as hex of the right length,
//! sessions as UUIDs, times as RFC 3339 between 2000 and 2200, paths
//! relative to the project without `..`, names and messages cleaned
//! ([`clean_text`]) and limited, the idle time and the poll interval
//! clamped to the bounds of the project's settings (a lock cannot make
//! others wait longer). Nothing here panics on any input; a seeded random
//! run in the tests and the fuzz targets in `core/vcs/fuzz` feed it
//! mutated files.

use std::collections::BTreeMap;
use std::fmt;

use mitcad_model::Sha256;
use mitcad_model::file::settings::{IDLE_MINUTES, POLL_SECONDS};
use serde::{Deserialize, Serialize};

/// The `format` of a lock's `lock.json`.
pub const LOCK_FORMAT: &str = "mitcad-lock";
/// The `format` of a request's `request.json`.
pub const REQUEST_FORMAT: &str = "mitcad-lock-request";
/// The version of both formats that this Mitcad reads and writes.
pub const FORMAT_VERSION: u64 = 1;
/// The file in a lock commit's tree.
pub const LOCK_FILE: &str = "lock.json";
/// The file in a request commit's tree.
pub const REQUEST_FILE: &str = "request.json";
/// `lock.json` and `request.json` larger than this are not read.
pub const MAX_FILE_SIZE: usize = 16 * 1024;
/// A name (of a holder or a requester) has at most this many characters.
pub const MAX_NAME: usize = 100;
/// An email address has at most this many characters.
pub const MAX_EMAIL: usize = 254;
/// A request's or an answer's message has at most this many characters.
pub const MAX_MESSAGE: usize = 200;
/// A path in the project has at most this many bytes.
pub const MAX_PATH: usize = 1024;
/// The version of the application that wrote a lock: at most this many
/// characters.
pub const MAX_APPLICATION_VERSION: usize = 40;
/// A lock lists at most this many request ids it has seen, and answers.
pub const MAX_REQUEST_IDS: usize = 64;
/// The minutes of a "keep" answer: at most the longest idle time.
pub const MAX_KEEP_MINUTES: u32 = IDLE_MINUTES.1;

/// Where the edit locks are on the remote: `refs/mitcad/locks/<id>`.
pub const LOCKS_PREFIX: &str = "refs/mitcad/locks/";
/// Where the requests are: `refs/mitcad/lock-requests/<id>/<session>`.
pub const REQUESTS_PREFIX: &str = "refs/mitcad/lock-requests/";
/// Where a poll keeps its copies of the remote's lock and request refs:
/// `refs/mitcad/remote-locks/locks/<id>` and
/// `refs/mitcad/remote-locks/lock-requests/<id>/<session>`.
pub const LOCAL_PREFIX: &str = "refs/mitcad/remote-locks/";

/// The earliest and latest time a lock may name (2000-01-01 and
/// 2200-01-01, UTC).
const TIME_RANGE: (i64, i64) = (946_684_800, 7_258_118_400);

/// The id of a file's lock: the SHA-256 (64 lowercase hex digits) of its
/// path in the project (`/`-separated).
pub fn lock_id(path: &str) -> String {
    Sha256::of(path.as_bytes()).to_string()
}

/// The remote's ref of a lock.
pub fn lock_ref(id: &str) -> String {
    format!("{LOCKS_PREFIX}{id}")
}

/// The remote's ref of a request.
pub fn request_ref(id: &str, session: &str) -> String {
    format!("{REQUESTS_PREFIX}{id}/{session}")
}

/// The local copy of a remote's ref under [`LOCAL_PREFIX`]
/// (`refs/mitcad/locks/x` → `refs/mitcad/remote-locks/locks/x`).
pub fn local_ref(remote_ref: &str) -> String {
    let rest = remote_ref
        .strip_prefix("refs/mitcad/")
        .unwrap_or(remote_ref);
    format!("{LOCAL_PREFIX}{rest}")
}

/// What a ref of the lock namespaces names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockRefName {
    /// `refs/mitcad/locks/<id>`.
    Lock(String),
    /// `refs/mitcad/lock-requests/<id>/<session>`.
    Request(String, String),
}

impl LockRefName {
    /// Reads a remote's ref name; None for another ref or one whose id or
    /// session is not of the right form (a probe's, someone else's).
    pub fn parse(name: &str) -> Option<Self> {
        if let Some(id) = name.strip_prefix(LOCKS_PREFIX) {
            return is_lock_id(id).then(|| Self::Lock(id.to_owned()));
        }
        let rest = name.strip_prefix(REQUESTS_PREFIX)?;
        let (id, session) = rest.split_once('/')?;
        (is_lock_id(id) && is_session(session))
            .then(|| Self::Request(id.to_owned(), session.to_owned()))
    }

    /// The ref on the remote.
    pub fn remote(&self) -> String {
        match self {
            Self::Lock(id) => lock_ref(id),
            Self::Request(id, session) => request_ref(id, session),
        }
    }
}

/// Whether `text` is `len` lowercase hexadecimal digits.
fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A lock's id: 64 lowercase hex digits.
pub fn is_lock_id(text: &str) -> bool {
    is_hex(text, 64)
}

/// A commit's id (the repositories are SHA-1): 40 lowercase hex digits.
pub fn is_commit_id(text: &str) -> bool {
    is_hex(text, 40)
}

/// A session: a UUID in lowercase (`8-4-4-4-12` hex digits).
pub fn is_session(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => matches!(b, b'0'..=b'9' | b'a'..=b'f'),
        })
}

/// A session id as an application gives it (case and surrounding spaces
/// do not matter), or None when it is not a UUID.
pub fn session_of(text: &str) -> Option<String> {
    let session = text.trim().to_ascii_lowercase();
    is_session(&session).then_some(session)
}

/// A session of the command line for an author's email: a UUID made from
/// its SHA-256, the same in every run, so that `mitcad-cli lock release`
/// releases what `mitcad-cli lock take` took.
pub fn command_line_session(email: &str) -> String {
    let digest = Sha256::of(format!("mitcad-cli\0{}", email.trim().to_lowercase()).as_bytes());
    let hex = digest.to_string();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Whether `path` is a path relative to the project: `/`-separated parts,
/// none empty, `.` or `..`, no backslash, no control character, no drive,
/// at most [`MAX_PATH`] bytes.
pub fn is_project_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && !path
            .split('/')
            .next()
            .is_some_and(|first| first.ends_with(':'))
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Whether a character is invisible: a format character (bidirectional
/// overrides and isolates, zero-width characters, the byte order mark, tag
/// characters), a variation selector or a filler that shows as nothing.
fn is_invisible(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x034F
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

/// Text for people as Mitcad shows it: control characters, bidirectional
/// overrides and other invisible characters removed, whitespace collapsed
/// to single spaces and trimmed. The application shows it as plain text
/// only.
pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(4096));
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = true;
        } else if !c.is_control() && !is_invisible(c) {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// `text` cleaned and cut to at most `max` characters (what Mitcad writes).
pub fn clean_limited(text: &str, max: usize) -> String {
    let cleaned = clean_text(text);
    match cleaned.char_indices().nth(max) {
        Some((end, _)) => cleaned[..end].trim_end().to_owned(),
        None => cleaned,
    }
}

/// Text read from a lock or request: cleaned, at most `max` characters,
/// and not empty unless `empty` allows it.
fn read_text(text: &str, max: usize, what: &str, empty: bool) -> Result<String, String> {
    let cleaned = clean_text(text);
    if cleaned.is_empty() && !empty {
        return Err(format!("{what} is empty"));
    }
    if cleaned.chars().count() > max {
        return Err(format!("{what} is longer than {max} characters"));
    }
    Ok(cleaned)
}

// Times: RFC 3339, written in UTC with seconds.

/// Days since 1970-01-01 of a date of the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = i64::from((month + 9) % 12);
    let day_of_year = (153 * month_from_march + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date of a day since 1970-01-01: (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_from_march + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A time (seconds since 1970, UTC) as Mitcad writes it:
/// `2026-10-08T13:40:00Z`.
pub fn format_time(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// Reads an RFC 3339 time (`2026-10-08T13:40:00Z`, `…T15:40:00.123+02:00`)
/// as seconds since 1970 (UTC); None when it is not one or is outside the
/// years 2000 to 2200.
pub fn parse_time(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || bytes.len() > 40 {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = bytes.get(range)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    let year = i64::from(number(0..4)?);
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut at = 19;
    if bytes[at] == b'.' {
        at += 1;
        let start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if at == start || at - start > 9 {
            return None;
        }
    }
    let offset = match bytes.get(at..)? {
        b"Z" | b"z" => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let digits = [*h1, *h2, *m1, *m2];
            if !digits.iter().all(u8::is_ascii_digit) {
                return None;
            }
            let hours = i64::from((h1 - b'0') * 10 + (h2 - b'0'));
            let minutes = i64::from((m1 - b'0') * 10 + (m2 - b'0'));
            if hours > 23 || minutes > 59 {
                return None;
            }
            let offset = hours * 3600 + minutes * 60;
            if *sign == b'-' { -offset } else { offset }
        }
        _ => return None,
    };
    // A leap second counts as the second before it.
    let second = i64::from(second.min(59));
    let seconds = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + second
        - offset;
    (TIME_RANGE.0..=TIME_RANGE.1)
        .contains(&seconds)
        .then_some(seconds)
}

fn read_time(text: &str, what: &str) -> Result<i64, String> {
    parse_time(text).ok_or_else(|| format!("{what} is not a time (RFC 3339, 2000 to 2200)"))
}

/// A number clamped to `bounds`.
fn clamp(value: u64, bounds: (u32, u32)) -> u32 {
    u32::try_from(value.clamp(u64::from(bounds.0), u64::from(bounds.1))).unwrap_or(bounds.1)
}

// The contents.

/// Who holds a lock or asks for it: the project's author, as for versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub email: String,
}

impl Person {
    /// A person as Mitcad writes one: cleaned and cut to the limits.
    pub fn new(name: &str, email: &str) -> Self {
        Self {
            name: clean_limited(name, MAX_NAME),
            email: clean_limited(email, MAX_EMAIL),
        }
    }

    fn read(raw: &RawPerson, what: &str) -> Result<Self, String> {
        Ok(Self {
            name: read_text(&raw.name, MAX_NAME, &format!("{what}'s name"), false)?,
            email: read_text(&raw.email, MAX_EMAIL, &format!("{what}'s email"), false)?,
        })
    }
}

/// Whether the holder is working on the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    Active,
    /// No activity for the idle time, with unsaved changes kept (autosave
    /// off): a request is granted at once.
    Idle,
}

/// The holder's answer to a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerKind {
    /// The requester stays read-only and may ask again.
    Declined,
    /// The holder keeps the lock until `until`, then answers again.
    Keep,
}

/// An answer to a request, in the holder's lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub answer: AnswerKind,
    /// The end of a "keep" (the holder's clock: only shown, and compared
    /// only with the same lock's `refreshed_at`).
    pub until: Option<i64>,
    pub message: Option<String>,
}

/// Why a lock was taken from its previous holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakenBecause {
    /// The lock stayed unchanged for its idle time and two polls.
    Unchanged,
    /// A request got no receipt in time.
    NoReceipt,
    /// A request with a receipt got no answer in time.
    Unanswered,
    /// Taken over after a confirmation: the same person's other session,
    /// or a holder whose application went offline.
    TakeOver,
}

impl TakenBecause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::NoReceipt => "no_receipt",
            Self::Unanswered => "unanswered",
            Self::TakeOver => "take_over",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "unchanged" => Some(Self::Unchanged),
            "no_receipt" => Some(Self::NoReceipt),
            "unanswered" => Some(Self::Unanswered),
            "take_over" => Some(Self::TakeOver),
            _ => None,
        }
    }
}

/// The previous holder of a lock: who handed it over, or whom it was taken
/// from (and why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Previous {
    pub person: Person,
    pub session: String,
    /// Set when the lock was taken rather than handed over.
    pub because: Option<TakenBecause>,
}

/// A file's edit lock: `lock.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    /// The file, relative to the project (`/`-separated).
    pub path: String,
    pub owner: Person,
    /// The holder's session (a UUID per run of the application).
    pub session: String,
    pub application_version: String,
    /// Times on the holder's clock (UTC), only shown, never compared with
    /// another computer's clock.
    pub taken_at: i64,
    pub refreshed_at: i64,
    /// The holder's last activity.
    pub active_at: i64,
    /// Clamped to the bounds of the project's settings.
    pub idle_minutes: u32,
    /// The holder's poll interval of git (longer while connected to the
    /// broker), clamped.
    pub poll_seconds: u32,
    /// Whether the holder listens to the project's broker.
    pub mqtt: bool,
    /// The commit of the file's version the holder edits.
    pub base: Option<String>,
    pub state: LockState,
    pub idle_since: Option<i64>,
    /// The ids of the requests the holder has seen (the receipts).
    pub requests_seen: Vec<String>,
    /// The holder's answers by request id.
    pub answers: BTreeMap<String, Answer>,
    pub handed_over_from: Option<Previous>,
    pub taken_from: Option<Previous>,
}

/// A request for a file's lock: `request.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub path: String,
    pub requester: Person,
    pub session: String,
    /// The requester's clock: only shown.
    pub asked_at: i64,
    pub message: Option<String>,
    /// A refresh of a waiting request (mitcad#89): the request's id, the
    /// commit it was first written as, so that its receipt and answer
    /// stay its own. None for the first write, whose commit is the id.
    pub refresh_of: Option<String>,
    /// When it was refreshed (the requester's clock: only shown).
    pub refreshed_at: Option<i64>,
    /// The requester's git poll interval (clamped): a request not refreshed
    /// for the project's idle time and two of these is stale.
    pub poll_seconds: Option<u32>,
}

/// Why a `lock.json` or `request.json` is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// Larger than [`MAX_FILE_SIZE`] (its size).
    TooLarge(u64),
    /// A format version this Mitcad does not know: ignored, not counted as
    /// malformed.
    UnknownVersion(u64),
    /// Anything else: dropped and counted.
    Malformed(String),
}

impl Unreadable {
    /// Whether it counts as malformed (dropped and counted).
    pub fn is_malformed(&self) -> bool {
        !matches!(self, Self::UnknownVersion(_))
    }
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(size) => write!(f, "{size} bytes, more than {MAX_FILE_SIZE}"),
            Self::UnknownVersion(version) => {
                write!(
                    f,
                    "format version {version}, which this Mitcad does not read"
                )
            }
            Self::Malformed(why) => f.write_str(why),
        }
    }
}

// What the JSON holds before it is checked.

#[derive(Deserialize)]
struct Header {
    format: Option<String>,
    version: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawPerson {
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct RawAnswer {
    answer: String,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct RawPrevious {
    name: String,
    email: String,
    session: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize)]
struct RawLock {
    path: String,
    owner: RawPerson,
    session: String,
    #[serde(default)]
    application_version: Option<String>,
    taken_at: String,
    refreshed_at: String,
    active_at: String,
    idle_minutes: u64,
    poll_seconds: u64,
    #[serde(default)]
    mqtt: bool,
    #[serde(default)]
    base: Option<String>,
    state: String,
    #[serde(default)]
    idle_since: Option<String>,
    #[serde(default)]
    requests_seen: Vec<String>,
    #[serde(default)]
    answers: BTreeMap<String, RawAnswer>,
    #[serde(default)]
    handed_over_from: Option<RawPrevious>,
    #[serde(default)]
    taken_from: Option<RawPrevious>,
}

#[derive(Deserialize)]
struct RawRequest {
    path: String,
    requester: RawPerson,
    session: String,
    asked_at: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    refresh_of: Option<String>,
    #[serde(default)]
    refreshed_at: Option<String>,
    #[serde(default)]
    poll_seconds: Option<u64>,
}

/// The size limit, then the `format` and `version`.
fn check_header(bytes: &[u8], format: &str) -> Result<(), Unreadable> {
    if bytes.len() > MAX_FILE_SIZE {
        return Err(Unreadable::TooLarge(bytes.len() as u64));
    }
    let header: Header = serde_json::from_slice(bytes)
        .map_err(|e| Unreadable::Malformed(format!("not JSON of the expected form: {e}")))?;
    if header.format.as_deref() != Some(format) {
        return Err(Unreadable::Malformed(format!("its format is not {format}")));
    }
    match header.version.as_ref().and_then(serde_json::Value::as_u64) {
        Some(FORMAT_VERSION) => Ok(()),
        Some(version) => Err(Unreadable::UnknownVersion(version)),
        None => Err(Unreadable::Malformed(
            "its version is not a whole number".to_owned(),
        )),
    }
}

fn read_session(text: &str, what: &str) -> Result<String, String> {
    if is_session(text) {
        Ok(text.to_owned())
    } else {
        Err(format!("{what} is not a session id (a UUID)"))
    }
}

fn read_previous(raw: &RawPrevious, what: &str) -> Result<Previous, String> {
    let because = match &raw.reason {
        None => None,
        Some(reason) => Some(
            TakenBecause::parse(reason).ok_or_else(|| format!("{what} has an unknown reason"))?,
        ),
    };
    Ok(Previous {
        person: Person::read(
            &RawPerson {
                name: raw.name.clone(),
                email: raw.email.clone(),
            },
            what,
        )?,
        session: read_session(&raw.session, what)?,
        because,
    })
}

fn read_message(message: Option<&String>, what: &str) -> Result<Option<String>, String> {
    match message {
        None => Ok(None),
        Some(text) => {
            let text = read_text(text, MAX_MESSAGE, what, true)?;
            Ok((!text.is_empty()).then_some(text))
        }
    }
}

/// Reads a `lock.json`: the size limit, the format and version, then each
/// field checked.
pub fn read_lock(bytes: &[u8]) -> Result<Lock, Unreadable> {
    check_header(bytes, LOCK_FORMAT)?;
    let raw: RawLock = serde_json::from_slice(bytes)
        .map_err(|e| Unreadable::Malformed(format!("not a lock: {e}")))?;
    check_lock(raw).map_err(Unreadable::Malformed)
}

fn check_lock(raw: RawLock) -> Result<Lock, String> {
    if !is_project_path(&raw.path) {
        return Err("its path is not a path in the project".to_owned());
    }
    let state = match raw.state.as_str() {
        "active" => LockState::Active,
        "idle" => LockState::Idle,
        _ => return Err("its state is neither active nor idle".to_owned()),
    };
    let base = match raw.base {
        Some(base) if is_commit_id(&base) => Some(base),
        Some(_) => return Err("its base is not a commit id".to_owned()),
        None => None,
    };
    if raw.requests_seen.len() > MAX_REQUEST_IDS || raw.answers.len() > MAX_REQUEST_IDS {
        return Err(format!(
            "it lists more than {MAX_REQUEST_IDS} requests or answers"
        ));
    }
    if !raw.requests_seen.iter().all(|id| is_commit_id(id)) {
        return Err("a request id it has seen is not a commit id".to_owned());
    }
    let mut answers = BTreeMap::new();
    for (id, answer) in &raw.answers {
        if !is_commit_id(id) {
            return Err("an answer's request id is not a commit id".to_owned());
        }
        let kind = match answer.answer.as_str() {
            "declined" => AnswerKind::Declined,
            "keep" => AnswerKind::Keep,
            _ => return Err("an answer is neither declined nor keep".to_owned()),
        };
        let until = match (&answer.until, kind) {
            (Some(until), _) => Some(read_time(until, "an answer's until")?),
            (None, AnswerKind::Keep) => return Err("a keep answer has no until".to_owned()),
            (None, AnswerKind::Declined) => None,
        };
        answers.insert(
            id.clone(),
            Answer {
                answer: kind,
                until,
                message: read_message(answer.message.as_ref(), "an answer's message")?,
            },
        );
    }
    let application_version = match &raw.application_version {
        Some(text) => read_text(
            text,
            MAX_APPLICATION_VERSION,
            "the application version",
            true,
        )?,
        None => String::new(),
    };
    Ok(Lock {
        owner: Person::read(&raw.owner, "the owner")?,
        session: read_session(&raw.session, "the session")?,
        application_version,
        taken_at: read_time(&raw.taken_at, "taken_at")?,
        refreshed_at: read_time(&raw.refreshed_at, "refreshed_at")?,
        active_at: read_time(&raw.active_at, "active_at")?,
        idle_minutes: clamp(raw.idle_minutes, IDLE_MINUTES),
        poll_seconds: clamp(raw.poll_seconds, POLL_SECONDS),
        mqtt: raw.mqtt,
        base,
        state,
        idle_since: raw
            .idle_since
            .as_deref()
            .map(|t| read_time(t, "idle_since"))
            .transpose()?,
        requests_seen: raw.requests_seen,
        answers,
        handed_over_from: raw
            .handed_over_from
            .as_ref()
            .map(|p| read_previous(p, "handed_over_from"))
            .transpose()?,
        taken_from: raw
            .taken_from
            .as_ref()
            .map(|p| read_previous(p, "taken_from"))
            .transpose()?,
        path: raw.path,
    })
}

/// Reads a `request.json` as [`read_lock`] reads a lock.
pub fn read_request(bytes: &[u8]) -> Result<Request, Unreadable> {
    check_header(bytes, REQUEST_FORMAT)?;
    let raw: RawRequest = serde_json::from_slice(bytes)
        .map_err(|e| Unreadable::Malformed(format!("not a request: {e}")))?;
    let check = || -> Result<Request, String> {
        if !is_project_path(&raw.path) {
            return Err("its path is not a path in the project".to_owned());
        }
        let refresh_of = match &raw.refresh_of {
            Some(id) if is_commit_id(id) => Some(id.clone()),
            Some(_) => return Err("refresh_of is not a request's id".to_owned()),
            None => None,
        };
        Ok(Request {
            requester: Person::read(&raw.requester, "the requester")?,
            session: read_session(&raw.session, "the session")?,
            asked_at: read_time(&raw.asked_at, "asked_at")?,
            message: read_message(raw.message.as_ref(), "the message")?,
            path: raw.path.clone(),
            refresh_of,
            refreshed_at: raw
                .refreshed_at
                .as_deref()
                .map(|t| read_time(t, "refreshed_at"))
                .transpose()?,
            poll_seconds: raw.poll_seconds.map(|s| clamp(s, POLL_SECONDS)),
        })
    };
    check().map_err(Unreadable::Malformed)
}

// Writing.

fn previous_json(previous: &Previous) -> serde_json::Value {
    let mut value = serde_json::json!({
        "name": previous.person.name,
        "email": previous.person.email,
        "session": previous.session,
    });
    if let Some(because) = previous.because {
        value["reason"] = serde_json::json!(because.as_str());
    }
    value
}

impl Lock {
    /// `lock.json` as Mitcad writes it (keys in a fixed order), at most
    /// [`MAX_FILE_SIZE`] bytes, so that others read it: request ids and
    /// answers beyond [`MAX_REQUEST_IDS`] are left out, and while it is too
    /// large the answers and then the request ids, the first ones first.
    pub fn to_json(&self) -> Vec<u8> {
        let mut seen = self.requests_seen.len().saturating_sub(MAX_REQUEST_IDS);
        let mut answers = self.answers.len().saturating_sub(MAX_REQUEST_IDS);
        loop {
            let value = self.json_value(seen, answers);
            let mut text = serde_json::to_vec_pretty(&value).expect("serializable");
            text.push(b'\n');
            if text.len() <= MAX_FILE_SIZE {
                return text;
            }
            let compact = serde_json::to_vec(&value).expect("serializable");
            if compact.len() <= MAX_FILE_SIZE
                || (answers >= self.answers.len() && seen >= self.requests_seen.len())
            {
                return compact;
            }
            if answers < self.answers.len() {
                answers += 1;
            } else {
                seen += 1;
            }
        }
    }

    /// The JSON of the lock without its first `skip_seen` request ids and
    /// `skip_answers` answers.
    fn json_value(&self, skip_seen: usize, skip_answers: usize) -> serde_json::Value {
        let answers: serde_json::Map<String, serde_json::Value> = self
            .answers
            .iter()
            .skip(skip_answers)
            .map(|(id, answer)| {
                let kind = match answer.answer {
                    AnswerKind::Declined => "declined",
                    AnswerKind::Keep => "keep",
                };
                (
                    id.clone(),
                    serde_json::json!({
                        "answer": kind,
                        "until": answer.until.map(format_time),
                        "message": answer.message,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "format": LOCK_FORMAT,
            "version": FORMAT_VERSION,
            "path": self.path,
            "owner": self.owner,
            "session": self.session,
            "application_version": self.application_version,
            "taken_at": format_time(self.taken_at),
            "refreshed_at": format_time(self.refreshed_at),
            "active_at": format_time(self.active_at),
            "idle_minutes": self.idle_minutes,
            "poll_seconds": self.poll_seconds,
            "mqtt": self.mqtt,
            "base": self.base,
            "state": self.state,
            "idle_since": self.idle_since.map(format_time),
            "requests_seen": &self.requests_seen[skip_seen.min(self.requests_seen.len())..],
            "answers": answers,
            "handed_over_from": self.handed_over_from.as_ref().map(previous_json),
            "taken_from": self.taken_from.as_ref().map(previous_json),
        })
    }
}

impl Request {
    /// `request.json` as Mitcad writes it.
    pub fn to_json(&self) -> Vec<u8> {
        let value = serde_json::json!({
            "format": REQUEST_FORMAT,
            "version": FORMAT_VERSION,
            "path": self.path,
            "requester": self.requester,
            "session": self.session,
            "asked_at": format_time(self.asked_at),
            "message": self.message,
            "refresh_of": self.refresh_of,
            "refreshed_at": self.refreshed_at.map(format_time),
            "poll_seconds": self.poll_seconds,
        });
        let mut text = serde_json::to_vec_pretty(&value).expect("serializable");
        text.push(b'\n');
        text
    }
}
