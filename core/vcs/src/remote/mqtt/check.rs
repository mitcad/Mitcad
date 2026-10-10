// SPDX-License-Identifier: MIT
//! Checks of the fields of live messages (mitcad#89, "Untrusted input"):
//! ids, times, paths and branch names, and the text people see, cleaned.
//!
//! Everything here reads text someone else wrote, so nothing panics and
//! nothing is guessed: a value that does not pass is refused.

use std::time::{SystemTime, UNIX_EPOCH};

/// Names (of people) longer than this many characters are refused.
pub const MAX_NAME: usize = 100;
/// A request's message longer than this many characters is refused.
pub const MAX_MESSAGE: usize = 200;
/// Paths longer than this many bytes are refused.
pub const MAX_PATH: usize = 1024;
/// Branch names longer than this many bytes are refused.
pub const MAX_BRANCH: usize = 200;
/// Times before this (2020-01-01T00:00:00Z) ...
pub const EARLIEST: i64 = 1_577_836_800;
/// ... and from this on (2100-01-01T00:00:00Z) are refused.
pub const LATEST: i64 = 4_102_444_800;

/// Text for people: control characters, bidirectional overrides and other
/// invisible format characters removed, runs of whitespace made one space,
/// no space at either end.
pub fn clean_text(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = !clean.is_empty();
        } else if c.is_control() || invisible(c) {
            continue;
        } else {
            if space {
                clean.push(' ');
                space = false;
            }
            clean.push(c);
        }
    }
    clean
}

/// Characters that change how text around them looks or reads but show
/// nothing themselves: Unicode's format characters (category Cf, which has
/// the bidirectional marks, embeddings, overrides and isolates and the
/// zero-width characters), the combining grapheme joiner and the Hangul
/// fillers, which render as blanks.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// Whether `text` holds a character [`clean_text`] would remove (whitespace
/// other than a space counts too).
pub fn has_hidden(text: &str) -> bool {
    text.chars()
        .any(|c| (c.is_whitespace() && c != ' ') || c.is_control() || invisible(c))
}

/// A name for people: cleaned, 1 to [`MAX_NAME`] characters.
pub fn name(text: &str) -> Result<String, &'static str> {
    let name = clean_text(text);
    match name.chars().count() {
        0 => Err("an empty name"),
        n if n > MAX_NAME => Err("a name longer than 100 characters"),
        _ => Ok(name),
    }
}

/// A request's message: cleaned, at most [`MAX_MESSAGE`] characters.
pub fn message(text: &str) -> Result<String, &'static str> {
    let message = clean_text(text);
    if message.chars().count() > MAX_MESSAGE {
        return Err("a message longer than 200 characters");
    }
    Ok(message)
}

/// Lowercase hexadecimal digits, exactly `length` of them.
pub fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A commit id: SHA-1 (40) or SHA-256 (64) in lowercase hexadecimal. A
/// project's id is its repository's first commit.
pub fn is_commit(text: &str) -> bool {
    is_hex(text, 40) || is_hex(text, 64)
}

/// A file's id: the SHA-256 of its path in the project, in lowercase
/// hexadecimal.
pub fn is_file_id(text: &str) -> bool {
    is_hex(text, 64)
}

/// A session id: a UUID in lowercase (`8-4-4-4-12` hexadecimal digits), as
/// the application makes them.
pub fn is_session(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(part, length)| is_hex(part, length))
}

/// A path relative to the project: `/`-separated, without empty, `.` or
/// `..` parts, a drive, a backslash or hidden characters, at most
/// [`MAX_PATH`] bytes.
pub fn is_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.contains(['\\', ':'])
        && !has_hidden(path)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// A branch name as git allows it, in a stricter form: letters, digits,
/// `.`, `_`, `-` and `/`, not starting with `-`, `.` or `/`, without `..`,
/// `//` or `/.`, not ending with `/`, `.` or `.lock`.
pub fn is_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= MAX_BRANCH
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
        && !branch.starts_with(['-', '.', '/'])
        && !branch.ends_with(['/', '.'])
        && !branch.ends_with(".lock")
        && !branch.contains("..")
        && !branch.contains("//")
        && !branch.contains("/.")
}

/// Reads an RFC 3339 time (`2026-10-08T13:40:00Z`, a fraction and an
/// offset allowed) as Unix seconds, within [`EARLIEST`] and [`LATEST`].
pub fn parse_time(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 20 || b.len() > 40 {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let digits = b.get(range)?;
        digits.iter().try_fold(0i64, |n, &d| {
            d.is_ascii_digit().then(|| n * 10 + i64::from(d - b'0'))
        })
    };
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    if b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut rest = &b[19..];
    if let Some(fraction) = rest.strip_prefix(b".") {
        let digits = fraction.iter().take_while(|d| d.is_ascii_digit()).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        [b'Z' | b'z'] => 0,
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
    // A leap second is read as the second before it.
    let seconds =
        days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second.min(59)
            - offset;
    (EARLIEST..LATEST).contains(&seconds).then_some(seconds)
}

/// A time as RFC 3339 in UTC, to the second (`2026-10-08T13:40:00Z`).
pub fn format_time(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time / 60 % 60,
        time % 60
    )
}

/// An RFC 3339 time read and written again in UTC, to the second.
pub fn time(text: &str) -> Result<String, &'static str> {
    parse_time(text)
        .map(format_time)
        .ok_or("a time that is not RFC 3339 between 2020 and 2100")
}

/// The time now in Unix seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a date of the proleptic Gregorian calendar
/// (H. Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a number of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
