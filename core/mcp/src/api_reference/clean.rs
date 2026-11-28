// SPDX-License-Identifier: MIT
//! The developers' text made the users' (mitcad#2606): the command
//! reference (`core/model-api/src/commands.md`) carries ticket references,
//! history, test and file names and notes on the code inside, which the
//! generated API reference leaves out.
//!
//! The text is cleaned a sentence at a time: ticket references are taken
//! out where they stand alone or in parentheses (`(mitcad#714)`, `(#12,
//! #14)`, `(mitcad#689: …)` keeps its note), and so are the file and test
//! names in parentheses. A sentence that is history (`Before, …`,
//! `since mitcad#…`), about tests (`Tests: …`), about the code inside
//! (OCCT, its patches, Rust paths), or that still names a ticket, a file or
//! a test after that, is left out; each one is reported
//! ([`Dropped`]), so that its source can be fixed in the reference (or its
//! section marked internal, [`crate::reference::INTERNAL_MARKER`]).

/// A sentence left out, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dropped {
    pub(crate) reason: &'static str,
    pub(crate) text: String,
}

pub(crate) const HISTORY: &str = "history";
pub(crate) const TESTS: &str = "tests or files";
pub(crate) const INTERNAL: &str = "the code inside";
pub(crate) const UNCLEAN: &str = "could not be cleaned";

/// Where a ticket reference was taken out, before the parentheses round
/// it are tidied.
const TICKET: char = '\u{1}';
/// Where a file's or a test's name was taken out.
const NAME: char = '\u{2}';

/// The repositories whose tickets the reference names.
const REPOSITORIES: &[&str] = &["mitcad", "mitcad_private", "mitcad-private", "github"];

/// The text split into its code spans (`true`) and the prose between
/// them; an unclosed backtick leaves the rest prose.
pub(crate) fn spans(text: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let Some(open) = rest.find('`') else {
            if !rest.is_empty() {
                out.push((false, rest));
            }
            break;
        };
        let Some(close) = rest[open + 1..].find('`').map(|close| open + 1 + close) else {
            out.push((false, rest));
            break;
        };
        if open > 0 {
            out.push((false, &rest[..open]));
        }
        out.push((true, &rest[open + 1..close]));
        rest = &rest[close + 1..];
    }
    out
}

/// The prose of a text, its code spans left out.
pub(crate) fn prose(text: &str) -> String {
    spans(text)
        .into_iter()
        .filter(|(code, _)| !code)
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ticket references in prose: the byte ranges of `mitcad#714`,
/// `GitHub #557`, `#12` (after a space, an opening parenthesis or the
/// start).
fn tickets(prose: &str) -> Vec<(usize, usize)> {
    let bytes = prose.as_bytes();
    let mut found = Vec::new();
    for (at, _) in prose.match_indices('#') {
        let digits = prose[at + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits == 0
            || prose[at + 1 + digits..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let end = at + 1 + digits;
        let word = prose[..at]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .last()
            .map_or(at, |(i, _)| i);
        let prefix = prose[word..at].to_lowercase();
        if !prefix.is_empty() {
            if REPOSITORIES.contains(&prefix.as_str()) {
                found.push((word, end));
            }
            continue;
        }
        // `GitHub #557`.
        let github = at.checked_sub("github ".len()).filter(|&start| {
            prose.is_char_boundary(start) && prose[start..at].eq_ignore_ascii_case("github ")
        });
        if let Some(start) = github {
            found.push((start, end));
            continue;
        }
        let before = at.checked_sub(1).map(|i| bytes[i]);
        if matches!(before, None | Some(b' ' | b'(' | b',' | b';')) {
            found.push((at, end));
        }
    }
    found
}

/// Whether a text names a ticket outside its code spans (or a link to a
/// heading that does: `#…-mitcad1632`).
pub(crate) fn has_ticket(text: &str) -> bool {
    spans(text)
        .iter()
        .any(|(code, part)| !code && !tickets(part).is_empty())
        || (text.contains("](")
            && text.match_indices("-mitcad").any(|(at, found)| {
                text[at + found.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            }))
}

/// Whether code names a ticket with its repository (`mitcad#714`,
/// `GitHub #557`): a bare `#1` is code's own (a G-code variable, a face's
/// piece).
pub(crate) fn names_ticket(code: &str) -> bool {
    tickets(code)
        .iter()
        .any(|&(start, _)| !code[start..].starts_with('#'))
}

/// Whether a code span names a file, a test or the code inside rather
/// than something of the API: `core/model/src/features/renamed.rs`,
/// `profile_selection_tests.rs`, `api::face_lists_tests`,
/// `cli.fillet_exact_reported`, `Kernel::face_plane`.
pub(crate) fn internal_name(span: &str) -> bool {
    let span = span.trim();
    if span.contains(' ') && !span.contains("::") {
        return false;
    }
    const EXTENSIONS: &[&str] = &[
        ".rs", ".cpp", ".h", ".hpp", ".py", ".sh", ".ps1", ".cmake", ".tcl", ".mm", ".brep",
    ];
    const FOLDERS: &[&str] = &[
        "core/",
        "app/",
        "geometry/",
        "tools/",
        "docs/",
        "cmake/",
        "third_party/",
        "packaging/",
        "resources/",
        "release/",
        "private/",
    ];
    const TEST_GROUPS: &[&str] = &[
        "core.",
        "cli.",
        "app.",
        "geometry.",
        "repo.",
        "tools.",
        "occt.",
        "ui.",
    ];
    let lower = span.to_lowercase();
    let word = |text: &str| {
        text.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '*'))
    };
    (span.contains("::") && !span.contains('"'))
        || EXTENSIONS
            .iter()
            .any(|extension| lower.ends_with(extension))
        || FOLDERS.iter().any(|folder| lower.starts_with(folder))
        || (TEST_GROUPS.iter().any(|group| lower.starts_with(group)) && word(&lower))
        || ((lower.ends_with("_tests") || lower.ends_with("_test") || lower.starts_with("test_"))
            && word(&lower))
}

/// A history phrase in prose, as found (lower case).
pub(crate) fn history(prose: &str) -> Option<&'static str> {
    let lower = format!(" {}", prose.to_lowercase());
    const PHRASES: &[&str] = &[
        "previously",
        "formerly",
        " since mitcad#",
        " until mitcad#",
        " before mitcad#",
        " after mitcad#",
        " since #",
        " before #",
        " until 0.",
        " since 0.",
        " before 0.",
        " now no longer",
    ];
    if let Some(phrase) = PHRASES.iter().find(|phrase| lower.contains(*phrase)) {
        return Some(phrase);
    }
    let start = lower.trim_start();
    for opening in [
        "before,",
        "before mitcad",
        "until 0.",
        "until mitcad",
        "up to mitcad",
        "since mitcad",
    ] {
        if start.starts_with(opening) {
            return Some(opening);
        }
    }
    // A ticket taken out right after a word of time.
    for word in ["since", "before", "until", "after"] {
        if lower.contains(&format!(" {word} {TICKET}")) {
            return Some("a ticket's time");
        }
    }
    // A date: `12.10.2026`.
    let bytes = lower.as_bytes();
    for (at, _) in lower.match_indices(".20") {
        let year = &bytes[at + 1..];
        let day_month = &bytes[..at];
        if year.len() >= 4
            && year[..4].iter().all(u8::is_ascii_digit)
            && day_month.len() >= 3
            && day_month[day_month.len() - 1].is_ascii_digit()
            && day_month[day_month.len() - 3..].contains(&b'.')
        {
            return Some("a date");
        }
    }
    None
}

/// A note on the code inside in prose: the geometry kernel's library and
/// its patches.
pub(crate) fn internal_note(prose: &str) -> Option<&'static str> {
    let lower = format!(" {} ", prose.to_lowercase());
    const PHRASES: &[&str] = &[
        "occt",
        "open cascade",
        "port patch",
        "port's patch",
        " patch 0",
        " patch 1",
        " patch 2",
        "mitcad_private",
        "the mock kernel",
        " corpus",
    ];
    PHRASES
        .iter()
        .find(|phrase| lower.contains(*phrase))
        .copied()
}

/// Whether a sentence is about tests: `Tests: …`, `Test: …`.
fn about_tests(prose: &str) -> bool {
    let lower = prose.trim_start().to_lowercase();
    [
        "tests:",
        "test:",
        "tests ",
        "tested ",
        "the test ",
        "the tests ",
    ]
    .iter()
    .any(|opening| lower.starts_with(opening))
}

/// The text's sentences: ends after `.`, `!` or `?` outside code spans and
/// parentheses when a capital, a backtick or a quote follows a space.
pub(crate) fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut code = false;
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (k, &(i, c)) in chars.iter().enumerate() {
        match c {
            '`' => code = !code,
            '(' | '[' if !code => depth += 1,
            ')' | ']' if !code => depth = (depth - 1).max(0),
            '.' | '!' | '?' if !code && depth == 0 => {
                let space = chars.get(k + 1).is_some_and(|(_, next)| *next == ' ');
                let next = chars.get(k + 2).is_some_and(|(_, next)| {
                    next.is_uppercase() || matches!(next, '`' | '"' | '*')
                });
                // Not after `e.g.`, `i.e.`, `etc.` or `vs.`.
                let word: String = text[start..i]
                    .rsplit(' ')
                    .next()
                    .unwrap_or_default()
                    .to_lowercase();
                let abbreviation = matches!(word.as_str(), "e.g" | "i.e" | "vs" | "cf" | "no");
                if space && next && !abbreviation {
                    out.push(text[start..=i].trim());
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// A sentence with its ticket references and the names of files and tests
/// in code spans replaced by [`TICKET`] and [`NAME`].
fn marked(sentence: &str) -> String {
    let mut out = String::new();
    for (code, part) in spans(sentence) {
        if code {
            if internal_name(part) {
                out.push(NAME);
            } else {
                out.push('`');
                out.push_str(part);
                out.push('`');
            }
            continue;
        }
        let mut last = 0;
        for (start, end) in tickets(part) {
            out.push_str(&part[last..start]);
            out.push(TICKET);
            last = end;
        }
        out.push_str(&part[last..]);
    }
    out
}

/// A parenthesis's text split at its `,` and `;` outside code, brackets
/// and inner parentheses: each part and the separator after it.
pub(crate) fn parts(text: &str) -> Vec<(String, Option<char>)> {
    let mut out = Vec::new();
    let mut part = String::new();
    let mut code = false;
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '`' => code = !code,
            '(' | '[' | '{' if !code => depth += 1,
            ')' | ']' | '}' if !code => depth -= 1,
            ',' | ';' if !code && depth == 0 => {
                out.push((std::mem::take(&mut part), Some(c)));
                continue;
            }
            _ => {}
        }
        part.push(c);
    }
    out.push((part, None));
    out
}

/// Whether a part of a parenthesis is only taken-out references: nothing
/// is left but marks, spaces and joining words.
fn only_marks(part: &str) -> bool {
    part.replace([TICKET, NAME], " ")
        .split_whitespace()
        .all(|word| {
            matches!(
                word,
                "and" | "or" | "see" | "also" | "in" | "as" | "with" | ":"
            )
        })
}

/// A parenthesis's text without its taken-out references; None when
/// nothing is left of it.
fn tidy_parenthesis(inner: &str) -> Option<String> {
    let inner = inner
        .replace(&format!("{TICKET}: "), "")
        .replace(&format!("{NAME}: "), "");
    let kept: Vec<(String, Option<char>)> = parts(&inner)
        .into_iter()
        .filter(|(part, _)| !only_marks(part))
        .collect();
    if kept.is_empty() {
        return None;
    }
    let mut out = String::new();
    for (k, (part, separator)) in kept.iter().enumerate() {
        let part = if k == 0 {
            part.trim_start()
        } else {
            part.as_str()
        };
        out.push_str(part);
        if k + 1 < kept.len() {
            out.push(separator.unwrap_or(','));
        }
    }
    Some(out.trim_end().to_owned())
}

/// The sentence with each parenthesis (outside code) tidied, innermost
/// first; a parenthesis with nothing left goes with the space before it.
fn tidy(sentence: &str) -> String {
    let chars: Vec<char> = sentence.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    // Where each open parenthesis is in `out`.
    let mut open: Vec<usize> = Vec::new();
    let mut code = false;
    for &c in &chars {
        match c {
            '`' => {
                code = !code;
                out.push(c);
            }
            '(' if !code => {
                open.push(out.len());
                out.push(c);
            }
            ')' if !code && !open.is_empty() => {
                let start = open.pop().unwrap_or_default();
                let inner: String = out[start + 1..].iter().collect();
                out.truncate(start);
                if inner.contains([TICKET, NAME]) {
                    match tidy_parenthesis(&inner) {
                        Some(kept) => {
                            out.push('(');
                            out.extend(kept.chars());
                            out.push(')');
                        }
                        None => {
                            while out.last() == Some(&' ') {
                                out.pop();
                            }
                        }
                    }
                } else {
                    out.push('(');
                    out.extend(inner.chars());
                    out.push(')');
                }
            }
            _ => out.push(c),
        }
    }
    out.into_iter().collect()
}

/// A sentence's ticket references outside parentheses taken out where
/// they stand alone (`…, mitcad#455.`); None when one is part of the
/// sentence (`see mitcad#12`, `mitcad#557's design`).
fn without_loose_tickets(sentence: &str) -> Option<String> {
    if !sentence.contains(TICKET) {
        return Some(sentence.to_owned());
    }
    let lower = sentence.to_lowercase();
    for word in [
        "see", "in", "of", "by", "per", "from", "as", "for", "and", "or", "the", "to", "with",
    ] {
        if lower.contains(&format!(" {word} {TICKET}"))
            || lower.starts_with(&format!("{word} {TICKET}"))
        {
            return None;
        }
    }
    if lower.contains(&format!("{TICKET}'")) || lower.contains(&format!("{TICKET}’")) {
        return None;
    }
    let mut text = sentence.to_owned();
    for separator in [", ", "; ", " "] {
        text = text.replace(&format!("{separator}{TICKET}"), &TICKET.to_string());
    }
    for after in ['.', ':', ';', ','] {
        text = text.replace(&format!("{TICKET}{after}"), &after.to_string());
    }
    if text.ends_with(TICKET) {
        text.pop();
    }
    (!text.contains(TICKET)).then(|| {
        text.replace(" ,", ",")
            .replace(" .", ".")
            .replace("  ", " ")
    })
}

/// A text (a paragraph, a list item, a table cell) cleaned, and the
/// sentences left out.
pub(crate) fn clean_text(text: &str, dropped: &mut Vec<Dropped>) -> String {
    let mut kept: Vec<String> = Vec::new();
    for sentence in sentences(text) {
        let mut leave_out = |reason: &'static str| {
            dropped.push(Dropped {
                reason,
                text: sentence.to_owned(),
            });
        };
        // A code span that runs on over a ticket reference (an unclosed
        // span in the source) cannot be cleaned: code is kept as it is.
        if spans(sentence)
            .iter()
            .any(|(code, part)| *code && names_ticket(part))
        {
            leave_out(UNCLEAN);
            continue;
        }
        let marked = marked(sentence);
        let marked_prose = prose(&marked);
        if about_tests(&marked_prose) {
            leave_out(TESTS);
            continue;
        }
        if history(&prose(sentence)).is_some() || history(&marked_prose).is_some() {
            leave_out(HISTORY);
            continue;
        }
        if internal_note(&marked_prose).is_some() {
            leave_out(INTERNAL);
            continue;
        }
        let mut tidied = tidy(&marked);
        // `…; before, it was …` ends where the history begins.
        if let Some(at) = tidied
            .find("; before,")
            .or_else(|| tidied.find("; Before,"))
        {
            tidied.truncate(at);
            tidied.push('.');
        }
        if tidied.contains(NAME) {
            leave_out(TESTS);
            continue;
        }
        let Some(tidied) = without_loose_tickets(&tidied).filter(|text| !has_ticket(text)) else {
            leave_out(UNCLEAN);
            continue;
        };
        let tidied = tidied.trim();
        if !tidied.is_empty() {
            kept.push(tidied.to_owned());
        }
    }
    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(text: &str) -> (String, Vec<&'static str>) {
        let mut dropped = Vec::new();
        let text = clean_text(text, &mut dropped);
        (text, dropped.iter().map(|dropped| dropped.reason).collect())
    }

    #[test]
    fn ticket_references_are_taken_out() {
        assert_eq!(
            clean("It says so in its result (mitcad#1729): `warnings`."),
            ("It says so in its result: `warnings`.".to_owned(), vec![])
        );
        assert_eq!(
            clean("A surface ([Surface modelling](#surface-modelling), mitcad#455)."),
            (
                "A surface ([Surface modelling](#surface-modelling)).".to_owned(),
                vec![]
            )
        );
        assert_eq!(
            clean("The box (mitcad#714: the boxes in MCP's `changes`) is measured."),
            (
                "The box (the boxes in MCP's `changes`) is measured.".to_owned(),
                vec![]
            )
        );
        assert_eq!(
            clean(
                "Faces are found again (mitcad#272, `core/model/src/features/renamed.rs`), wherever. Next (#12, #14)."
            ),
            ("Faces are found again, wherever. Next.".to_owned(), vec![])
        );
        // Face numbers and anchors in code stay.
        assert_eq!(
            clean("Use `F3:end(r{…})#1` and `#k`."),
            ("Use `F3:end(r{…})#1` and `#k`.".to_owned(), vec![])
        );
        // A code span that runs on over a ticket is left out whole.
        assert_eq!(
            clean("A bar `{\"size\" (mitcad#2550, with ` only. It works."),
            ("It works.".to_owned(), vec![UNCLEAN])
        );
    }

    #[test]
    fn history_tests_and_notes_on_the_code_are_left_out() {
        assert_eq!(
            clean("A cut keeps its pieces. Before, every piece was a body. It works."),
            (
                "A cut keeps its pieces. It works.".to_owned(),
                vec![HISTORY]
            )
        );
        assert_eq!(
            clean("It works. Tests: `core.model.sketch-refs` (`profile_selection_tests.rs`)."),
            ("It works.".to_owned(), vec![TESTS])
        );
        assert_eq!(
            clean("Each type is a module `core/model/src/features/<type>.rs`. Lengths in mm."),
            ("Lengths in mm.".to_owned(), vec![TESTS])
        );
        assert_eq!(
            clean("The face is rebuilt (port patch 1100: a pocket's walls). It holds."),
            ("It holds.".to_owned(), vec![INTERNAL])
        );
        assert_eq!(
            clean("This works since mitcad#1200 for every body."),
            (String::new(), vec![HISTORY])
        );
        assert_eq!(
            clean("GitHub #557's design shows it."),
            (String::new(), vec![UNCLEAN])
        );
        assert_eq!(
            clean("The pieces are new bodies; before, every piece was kept."),
            ("The pieces are new bodies.".to_owned(), vec![])
        );
        assert!(history("the owner said so on 12.10.2026").is_some());
        assert!(history("a value of 0.20 mm").is_none());
    }

    #[test]
    fn names_of_files_and_tests_are_told_from_the_api() {
        for name in [
            "core/model/src/features/renamed.rs",
            "profile_selection_tests.rs",
            "api::face_lists_tests",
            "cli.fillet_exact_reported",
            "Kernel::face_plane",
            "inch_entry_tests",
        ] {
            assert!(internal_name(name), "{name}");
        }
        for name in [
            "F2:side(c1)",
            "\"Body1/F2:end(r{c5})\"",
            "O1/O4",
            "sketch.add_line",
            "add_feature",
            "{\"query\": \"bodies\"}",
            "mm/s",
        ] {
            assert!(!internal_name(name), "{name}");
        }
        assert!(has_ticket("as mitcad#12 says"));
        assert!(has_ticket("see (#12)"));
        assert!(!has_ticket("the face `#2` and F3#1"));
    }

    #[test]
    fn sentences_end_outside_code_and_parentheses() {
        assert_eq!(
            sentences("One (a. B) here. Two `x. Y` there, e.g. `z`. Three"),
            vec!["One (a. B) here.", "Two `x. Y` there, e.g. `z`.", "Three"]
        );
    }
}
