// SPDX-License-Identifier: MIT
//! The command reference in parts (mitcad#714). The tool `model_reference`
//! answers with the reference's contents and the names MCP permits, one
//! section, the rows of a query or a command, a feature type's section or
//! the lines that contain a text, each with at most [`LIMIT`] bytes of
//! text, where the resource holds the whole reference (about 1 MB, more
//! than an agent's context). Errors give the permitted names, and a
//! refused command that misses fields gets the reference of its feature
//! type or command.
//!
//! Agents see the reference without what they cannot use (mitcad#1509):
//! the sections whose command and query tables name only names MCP does
//! not permit, the sections marked [`INTERNAL_MARKER`] (how the code works
//! inside) and the issue numbers of headings.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde_json::{Value, json};

use crate::COMMANDS_REFERENCE;

/// The line under a heading of the reference that hides its section from
/// agents (mitcad#1509): it is about how the code works inside, not the
/// API. A Markdown comment, so the developers' document reads as before.
pub const INTERNAL_MARKER: &str = "<!-- mcp: internal -->";

/// The most text one answer holds, in bytes.
pub const LIMIT: usize = 64 * 1024;
/// The most text of a feature type's or a command's reference that a
/// refused command's result holds.
const ERROR_LIMIT: usize = 12 * 1024;
/// The most search hits in one answer.
const MAX_HITS: usize = 200;
/// A search hit's excerpt, in characters.
const EXCERPT: usize = 240;
/// The most of one paragraph or list item a brief answer keeps, in bytes
/// (mitcad#1587).
const BRIEF_ITEM: usize = 800;
/// The largest section a command or query described in prose (without
/// table rows) is answered with whole (mitcad#1897); a larger one gives the
/// paragraphs, examples and lists of it that describe the name.
const ENTRY_WHOLE: usize = 16 * 1024;
/// The most other sections an entry lists in `also_in`.
const ALSO_IN: usize = 20;

const USAGE: &str = "model_reference with section (a heading, or a name a heading lists), query, command, feature (a feature type) or search gives that part of the reference; a section or a feature type comes in brief (the example, the tables, each field's text, the subsections' headings), and extended: true gives its whole text; a query or a command gives its entry (its table rows, or the section that describes it in prose, with section and path) and a command its JSON Schema (schema) where one exists, and extended: true adds what other sections say of it; brief: true says an answer was shortened on purpose (extended: true gives the rest), truncated: true that it was cut for size (pass next_offset as offset); with guide, a guide on how to model well here (guides lists them, \"index\" says when to read each; \"bug-reporting\" when a result is unexpected); offset continues a truncated answer from its next_offset (for search: the hits to skip).";

/// The names MCP permits, which the backend supplies
/// (`mitcad_model_api::classes`): the reference does not tell them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vocabulary {
    pub queries: Vec<String>,
    pub commands: Vec<String>,
    pub features: Vec<String>,
}

impl Vocabulary {
    fn is_empty(&self) -> bool {
        self.queries.is_empty() && self.commands.is_empty() && self.features.is_empty()
    }

    /// Whether a query or a command is permitted.
    fn permits(&self, name: &str) -> bool {
        self.queries
            .iter()
            .chain(&self.commands)
            .any(|known| known == name)
    }

    /// Whether a feature type is permitted.
    fn permits_feature(&self, name: &str) -> bool {
        self.features.iter().any(|known| known == name)
    }
}

/// The error for a query the API does not have: the queries MCP permits.
/// A query the API has but MCP refuses keeps its own error.
pub fn unknown_query(name: &str, vocabulary: &Vocabulary) -> String {
    unknown("query", "queries", name, &vocabulary.queries)
}

/// The error for a command the API does not have: the commands MCP
/// permits.
pub fn unknown_command(name: &str, vocabulary: &Vocabulary) -> String {
    unknown("command", "commands", name, &vocabulary.commands)
}

/// The most characters of a client's own text that an error repeats
/// (mitcad#935): refusals are logged, and a client's text can be
/// megabytes long.
pub const ECHO_CHARS: usize = 200;
/// The most field names of a client's object that an error lists.
const ECHO_FIELDS: usize = 20;

/// A client's text as an error repeats it (mitcad#935): whole when it has
/// at most [`ECHO_CHARS`] characters, else its first [`ECHO_CHARS`] with
/// `…` and how many bytes were left out.
pub fn echoed(text: &str) -> Cow<'_, str> {
    match text.char_indices().nth(ECHO_CHARS) {
        None => Cow::Borrowed(text),
        Some((cut, _)) => Cow::Owned(format!(
            "{}… ({} more bytes)",
            &text[..cut],
            text.len() - cut
        )),
    }
}

fn unknown(kind: &str, plural: &str, name: &str, names: &[String]) -> String {
    choices(
        &format!("unknown {kind} {}", echoed(name)),
        kind,
        plural,
        names,
    )
}

/// The error for a query or a command that the reference describes but
/// MCP does not permit, whose sections agents do not see (mitcad#1509).
fn unavailable(kind: &str, plural: &str, name: &str, names: &[String]) -> String {
    choices(
        &format!("the {kind} {} is not available through MCP", echoed(name)),
        kind,
        plural,
        names,
    )
}

/// `found`, and the names to choose from instead.
fn choices(found: &str, kind: &str, plural: &str, names: &[String]) -> String {
    if names.is_empty() {
        return format!("{found}; model_reference without arguments lists the {plural}");
    }
    format!(
        "{found}; the {plural} MCP permits: {}. model_reference {{\"{}\": \"<name>\"}} gives one's fields",
        names.join(", "),
        kind.split(' ').next().unwrap_or(kind)
    )
}

/// The error for a command (`key` `cmd`) or a query (`key` `query`)
/// object without its name: the form it takes, with an example.
pub fn missing_name(request: &Value, key: &str) -> String {
    let (form, example, plural) = if key == "cmd" {
        (
            "a command is {\"cmd\": \"<name>\", ...its fields}",
            "{\"cmd\": \"add_feature\", \"def\": {...}}",
            "commands",
        )
    } else {
        (
            "a query is {\"query\": \"<name>\", ...its fields}",
            "{\"query\": \"bodies\", \"properties\": true}",
            "queries",
        )
    };
    // At most ECHO_FIELDS of the client's names, each cut (mitcad#935).
    let count = request.as_object().map_or(0, serde_json::Map::len);
    let fields: Vec<Cow<'_, str>> = request
        .as_object()
        .map(|object| {
            object
                .keys()
                .take(ECHO_FIELDS)
                .map(|field| echoed(field.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let found = if request.get(key).is_some() {
        format!("its {key} is not a nonempty string")
    } else if fields.is_empty() {
        "this one is empty".to_owned()
    } else if count > fields.len() {
        format!(
            "this one has no {key} (its fields: {}, and {} more)",
            fields.join(", "),
            count - fields.len()
        )
    } else {
        format!("this one has no {key} (its fields: {})", fields.join(", "))
    };
    format!(
        "{form}, for example {example}; {found}. model_reference without arguments lists the {plural}"
    )
}

struct Heading {
    level: usize,
    /// As it is shown: in the agents' view without its issue numbers.
    title: String,
    /// As the reference writes it, which finds it too.
    full: String,
    /// Its line.
    line: usize,
    /// The line after its section, subsections included.
    end: usize,
}

struct Reference {
    lines: Vec<String>,
    /// Whether each line is a code fence or inside a code block.
    code: Vec<bool>,
    headings: Vec<Heading>,
    /// The paragraphs, list items, tables and code blocks, in order
    /// (mitcad#1897): what a command's or a query's entry is made of.
    blocks: Vec<Block>,
}

/// What a [`Block`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Paragraph,
    /// A top-level list item with its continuation and nested items.
    Item,
    Table,
    /// A fenced code block, its fences included.
    Code,
}

/// Lines of the reference that belong together (mitcad#1897), never
/// across a heading.
struct Block {
    lines: Range<usize>,
    kind: BlockKind,
}

/// Whether a line, without its indentation, starts a list item: `- `,
/// `* ` or `1. `.
fn is_bullet(trimmed: &str) -> bool {
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return true;
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && trimmed[digits..].starts_with(". ")
}

/// The blocks of a reference's lines (mitcad#1897).
fn split_blocks(lines: &[String], code: &[bool], headings: &[Heading]) -> Vec<Block> {
    let heading_lines: std::collections::BTreeSet<usize> =
        headings.iter().map(|heading| heading.line).collect();
    let fence = |i: usize| code[i] && lines[i].trim_start().starts_with("```");
    // A list item at the left margin; nested ones continue it.
    let bullet = |i: usize| {
        let line = &lines[i];
        let trimmed = line.trim_start();
        line.len() - trimmed.len() < 2 && is_bullet(trimmed)
    };
    // A line that continues a paragraph or a list item.
    let continues = |i: usize| {
        !code[i]
            && !heading_lines.contains(&i)
            && !lines[i].trim().is_empty()
            && !lines[i].starts_with('|')
            && !bullet(i)
    };
    let mut blocks = Vec::new();
    let count = lines.len();
    let mut i = 0;
    while i < count {
        if heading_lines.contains(&i) || (!code[i] && lines[i].trim().is_empty()) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let kind = if code[start] {
            if fence(start) {
                while i < count && code[i] && !fence(i) {
                    i += 1;
                }
                if i < count && fence(i) {
                    i += 1;
                }
            } else {
                while i < count && code[i] && !fence(i) {
                    i += 1;
                }
            }
            BlockKind::Code
        } else if lines[start].starts_with('|') {
            while i < count && !code[i] && lines[i].starts_with('|') {
                i += 1;
            }
            BlockKind::Table
        } else {
            while i < count && continues(i) {
                i += 1;
            }
            if bullet(start) {
                BlockKind::Item
            } else {
                BlockKind::Paragraph
            }
        };
        blocks.push(Block {
            lines: start..i,
            kind,
        });
    }
    blocks
}

/// A query's or a command's entry (mitcad#1897), as
/// [`Reference::item_entry`] makes it.
struct ItemEntry {
    /// The section that describes it: its heading as shown, and its path.
    section: Option<(String, String)>,
    /// What a lookup answers: its table rows; else the section that
    /// describes it in prose, whole up to [`ENTRY_WHOLE`], else that
    /// section's paragraphs, examples and lists about it.
    text: String,
    /// What a lookup with `extended: true` answers: the rows and every
    /// other paragraph and example that names it; else the whole section
    /// and what the other sections say of it.
    extended: String,
    /// Whether `text` is that section's extract rather than its whole
    /// text.
    extract: bool,
    /// Whether it has table rows.
    rows: bool,
    /// The other sections that name it, which `text` leaves out.
    also_in: Vec<String>,
}

/// The whole reference, as the developers' document has it.
fn reference() -> &'static Reference {
    static PARSED: OnceLock<Reference> = OnceLock::new();
    PARSED.get_or_init(|| Reference::parse(COMMANDS_REFERENCE))
}

/// The reference as the agents of a vocabulary see it (mitcad#1509),
/// made once per vocabulary.
fn view(vocabulary: &Vocabulary) -> Arc<Reference> {
    static VIEWS: Mutex<Vec<(Vocabulary, Arc<Reference>)>> = Mutex::new(Vec::new());
    let mut views = VIEWS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, view)) = views.iter().find(|(known, _)| known == vocabulary) {
        return Arc::clone(view);
    }
    let view = Arc::new(reference().for_agents(vocabulary));
    views.push((vocabulary.clone(), Arc::clone(&view)));
    view
}

/// The reference as agents see it (mitcad#1509), the text of the resource
/// `mitcad://reference/commands`: without the sections whose command and
/// query tables name only names the vocabulary does not permit (an empty
/// vocabulary, which does not say, keeps them), the sections marked
/// [`INTERNAL_MARKER`], and the issue numbers of headings.
pub fn reference_text(vocabulary: &Vocabulary) -> String {
    let mut text = view(vocabulary).lines.join("\n");
    text.push('\n');
    text
}

/// The permitted names whose reference agents no longer find
/// (mitcad#1509), each as `query <name>`, `command <name>` or
/// `feature <name>`: where the whole reference has a query's or a
/// command's table rows, the agents' view has them all; else the sections
/// whose headings list it, or else the lines that name it; and a feature
/// type's section. Empty when the agents' view keeps all they may use; a
/// test with the names the backend permits checks it.
pub fn reference_losses(vocabulary: &Vocabulary) -> Vec<String> {
    let whole = reference();
    let view = view(vocabulary);
    let view: &Reference = &view;
    let mut lost = Vec::new();
    for (kind, names) in [
        ("query", &vocabulary.queries),
        ("command", &vocabulary.commands),
    ] {
        for name in names {
            let rows = |reference: &Reference| -> usize {
                reference
                    .row_lines(kind, name)
                    .iter()
                    .map(|(_, rows)| rows.len())
                    .sum()
            };
            let listed = |reference: &Reference| !reference.listing(name).is_empty();
            // Named in prose or in a JSON example (mitcad#1897).
            let named = |reference: &Reference| !reference.naming(kind, name).is_empty();
            let kept = if rows(whole) > 0 {
                rows(view) == rows(whole)
            } else if listed(whole) {
                listed(view)
            } else {
                !named(whole) || named(view)
            };
            if !kept {
                lost.push(format!("{kind} {name}"));
            }
        }
    }
    for name in &vocabulary.features {
        if whole.feature_section(name).is_some() && view.feature_section(name).is_none() {
            lost.push(format!("feature {name}"));
        }
    }
    lost
}

/// A heading as agents see it (mitcad#1509): without its issue numbers,
/// `(mitcad#1416)` or `(#12, #14)`; the whole heading when nothing else
/// is left.
pub(crate) fn shown_title(title: &str) -> String {
    let issues = |text: &str| {
        text.split(',').all(|part| {
            part.trim().split_once('#').is_some_and(|(repo, number)| {
                repo.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    && !number.is_empty()
                    && number.chars().all(|c| c.is_ascii_digit())
            })
        })
    };
    let mut shown = String::new();
    let mut rest = title;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')').map(|close| open + close) else {
            break;
        };
        if issues(&rest[open + 1..close]) {
            shown.push_str(rest[..open].trim_end());
        } else {
            shown.push_str(&rest[..=close]);
        }
        rest = &rest[close + 1..];
    }
    shown.push_str(rest);
    let shown = shown.trim();
    if shown.is_empty() {
        title.to_owned()
    } else {
        shown.to_owned()
    }
}

/// Whether a token of a table's first cell is a query's or a command's
/// name: `bodies`, `sketch.add_line`.
fn is_name(token: &str) -> bool {
    token.starts_with(|c: char| c.is_ascii_lowercase())
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// A table row's first cell.
fn first_cell(line: &str) -> &str {
    line.split('|').nth(1).unwrap_or_default()
}

/// Whether a table's first header cell is that of the API's commands or
/// queries: `Command`, `Query`, `Command / query`; not `Command (Rust,
/// Document)`, whose rows are the code's.
fn api_header(cell: &str) -> bool {
    let cell = normal(cell);
    let words: Vec<&str> = cell
        .split(' ')
        .filter(|word| !matches!(*word, "/" | "or" | "and"))
        .collect();
    !words.is_empty()
        && words
            .iter()
            .all(|word| matches!(*word, "command" | "commands" | "query" | "queries"))
}

/// Gives each heading the line after its section: the next heading of its
/// level or above.
fn set_ends(headings: &mut [Heading], lines: usize) {
    let ends: Vec<usize> = headings
        .iter()
        .enumerate()
        .map(|(i, heading)| {
            headings[i + 1..]
                .iter()
                .find(|next| next.level <= heading.level)
                .map_or(lines, |next| next.line)
        })
        .collect();
    for (heading, end) in headings.iter_mut().zip(ends) {
        heading.end = end;
    }
}

/// Lower case, without backticks, single spaces.
fn normal(text: &str) -> String {
    text.replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The names a heading lists: `Face features: shell, draft` lists `shell`
/// and `draft`, `move, align, scale` its three, `rib, web` two.
fn listed(title: &str) -> Vec<String> {
    let names = title.rsplit_once(':').map_or(title, |(_, names)| names);
    names
        .split(',')
        .flat_map(|part| part.split(" and "))
        .map(normal)
        .filter(|name| !name.is_empty())
        .collect()
}

/// `text` from `offset` on, at most `limit` bytes, cut after a line when
/// one ends in the second half; the offset to continue from when cut.
pub(crate) fn cut(
    text: &str,
    offset: usize,
    limit: usize,
) -> Result<(String, Option<usize>), String> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(format!(
            "offset {offset} is not a place in this text of {} bytes; pass the next_offset of the previous answer",
            text.len()
        ));
    }
    let rest = &text[offset..];
    if rest.len() <= limit {
        return Ok((rest.to_owned(), None));
    }
    let mut end = limit;
    while !rest.is_char_boundary(end) {
        end -= 1;
    }
    if let Some(newline) = rest[..end].rfind('\n')
        && newline >= limit / 2
    {
        end = newline + 1;
    }
    Ok((rest[..end].to_owned(), Some(offset + end)))
}

/// Says in `answer` whether its text was cut, and where to continue. A
/// cut for size is said before any other note (mitcad#1897: a brief
/// answer's note stays beside it).
pub(crate) fn mark_cut(answer: &mut Value, next: Option<usize>) {
    answer["truncated"] = json!(next.is_some());
    if let Some(next) = next {
        answer["next_offset"] = json!(next);
        let cut = format!(
            "cut at {} kB: call again with the same arguments and offset {next} for the rest",
            LIMIT / 1024
        );
        let note = match answer.get("note").and_then(Value::as_str) {
            Some(other) => format!("{cut}. {other}"),
            None => cut,
        };
        answer["note"] = json!(note);
    }
}

/// Adds a sentence to `answer`'s note.
fn add_note(answer: &mut Value, text: &str) {
    let note = match answer.get("note").and_then(Value::as_str) {
        Some(other) => format!("{other}. {text}"),
        None => text.to_owned(),
    };
    answer["note"] = json!(note);
}

/// The search excerpt of `line` around the first place of `wanted` (in
/// lower case).
fn excerpt(line: &str, wanted: &str) -> String {
    let line = line.trim();
    let count = line.chars().count();
    if count <= EXCERPT {
        return line.to_owned();
    }
    let wanted: Vec<char> = wanted.chars().collect();
    let at = line
        .char_indices()
        .position(|(i, _)| {
            line[i..]
                .chars()
                .flat_map(char::to_lowercase)
                .take(wanted.len())
                .eq(wanted.iter().copied())
        })
        .unwrap_or(0);
    let start = at.saturating_sub(EXCERPT / 3);
    let mut text = String::new();
    if start > 0 {
        text.push('…');
    }
    text.extend(line.chars().skip(start).take(EXCERPT));
    if start + EXCERPT < count {
        text.push('…');
    }
    text
}

impl Reference {
    fn parse(text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut code = Vec::with_capacity(lines.len());
        let mut headings = Vec::new();
        let mut fenced = false;
        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                code.push(true);
                continue;
            }
            code.push(fenced);
            if fenced {
                continue;
            }
            let level = line.bytes().take_while(|byte| *byte == b'#').count();
            if (1..=6).contains(&level) && line[level..].starts_with(' ') {
                let title = line[level..].trim().to_owned();
                headings.push(Heading {
                    level,
                    full: title.clone(),
                    title,
                    line: i,
                    end: lines.len(),
                });
            }
        }
        set_ends(&mut headings, lines.len());
        let blocks = split_blocks(&lines, &code, &headings);
        Self {
            lines,
            code,
            headings,
            blocks,
        }
    }

    /// The reference as the agents of a vocabulary see it (mitcad#1509).
    ///
    /// A section's own text (before its first subsection) is left out when
    /// its first line is [`INTERNAL_MARKER`], when its command and query
    /// tables name only names the vocabulary does not permit and it has no
    /// permitted feature type, or when the section it is in is left out and
    /// it has no permitted name of its own. Its heading stays while one of
    /// its subsections does, so that the subsection keeps its place. The
    /// headings lose their issue numbers.
    fn for_agents(&self, vocabulary: &Vocabulary) -> Self {
        let count = self.headings.len();
        let mut cut = vec![false; count];
        // The sections the heading is in, innermost last.
        let mut outer: Vec<usize> = Vec::new();
        for (i, heading) in self.headings.iter().enumerate() {
            while outer
                .last()
                .is_some_and(|&j| self.headings[j].level >= heading.level)
            {
                outer.pop();
            }
            let own = heading.line + 1..self.own_end(i);
            let names = self.table_names(own.clone());
            let permitted = names.iter().any(|name| vocabulary.permits(name))
                || self
                    .feature_types(own.clone())
                    .iter()
                    .any(|name| vocabulary.permits_feature(name))
                || listed(&heading.title)
                    .iter()
                    .any(|name| vocabulary.permits(name) || vocabulary.permits_feature(name));
            let refused = !vocabulary.is_empty() && !names.is_empty() && !permitted;
            let with_outer = outer.last().is_some_and(|&j| cut[j]) && !permitted;
            // The document's title is never left out.
            cut[i] = heading.level > 1 && (self.internal(own) || refused || with_outer);
            outer.push(i);
        }
        // A heading stays while its own text or one of its subsections does.
        let mut shown: Vec<bool> = cut.iter().map(|&left_out| !left_out).collect();
        for i in (0..count).rev() {
            if !shown[i] {
                let end = self.headings[i].end;
                shown[i] = (i + 1..count)
                    .take_while(|&j| self.headings[j].line < end)
                    .any(|j| shown[j]);
            }
        }
        let mut lines = Vec::new();
        let mut code = Vec::new();
        let mut headings = Vec::new();
        // The innermost heading of the line, and the next heading.
        let mut current: Option<usize> = None;
        let mut next = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if let Some(heading) = self.headings.get(next).filter(|heading| heading.line == i) {
                if shown[next] {
                    let title = shown_title(&heading.title);
                    headings.push(Heading {
                        level: heading.level,
                        title: title.clone(),
                        full: heading.full.clone(),
                        line: lines.len(),
                        end: 0,
                    });
                    lines.push(format!("{} {title}", "#".repeat(heading.level)));
                    code.push(false);
                }
                current = Some(next);
                next += 1;
                continue;
            }
            if current.is_some_and(|h| cut[h]) {
                continue;
            }
            lines.push(line.clone());
            code.push(self.code[i]);
        }
        set_ends(&mut headings, lines.len());
        let blocks = split_blocks(&lines, &code, &headings);
        Self {
            lines,
            code,
            headings,
            blocks,
        }
    }

    /// The line after a heading's own text: its next heading's.
    fn own_end(&self, heading: usize) -> usize {
        self.headings
            .get(heading + 1)
            .map_or(self.lines.len(), |next| next.line)
    }

    /// Whether the first line of a section's own text (`own`) that is not
    /// empty is [`INTERNAL_MARKER`].
    fn internal(&self, mut own: Range<usize>) -> bool {
        own.find(|&i| !self.lines[i].trim().is_empty())
            .is_some_and(|i| !self.code[i] && self.lines[i].trim() == INTERNAL_MARKER)
    }

    /// The names in the first cells of the API's command and query tables
    /// ([`api_header`]) of these lines.
    fn table_names(&self, lines: Range<usize>) -> Vec<String> {
        let mut names = Vec::new();
        // The table's header line and whether it is of commands or queries.
        let mut header: Option<(usize, bool)> = None;
        for i in lines {
            let line = self.lines[i].as_str();
            if self.code[i] || !line.starts_with('|') {
                header = None;
                continue;
            }
            match header {
                None => header = Some((i, api_header(first_cell(line)))),
                Some((start, true)) if i >= start + 2 => names.extend(
                    first_cell(line)
                        .split('`')
                        .skip(1)
                        .step_by(2)
                        .map(str::trim)
                        .filter(|token| is_name(token))
                        .map(str::to_owned),
                ),
                Some(_) => {}
            }
        }
        names
    }

    /// The `"type"` values of the code in these lines, which name feature
    /// types in example definitions.
    fn feature_types(&self, lines: Range<usize>) -> Vec<String> {
        let mut types = Vec::new();
        for i in lines.filter(|&i| self.code[i]) {
            let mut rest = self.lines[i].as_str();
            while let Some(at) = rest.find("\"type\":") {
                rest = rest[at + "\"type\":".len()..].trim_start();
                if let Some(value) = rest.strip_prefix('"')
                    && let Some(end) = value.find('"')
                {
                    types.push(value[..end].to_owned());
                }
            }
        }
        types
    }

    /// The sections whose headings list a name.
    fn listing(&self, name: &str) -> Vec<&Heading> {
        let wanted = normal(name);
        self.sections()
            .filter(|heading| listed(&heading.title).contains(&wanted))
            .collect()
    }

    /// The headings of the sections (not the document's title).
    fn sections(&self) -> impl Iterator<Item = &Heading> {
        self.headings.iter().filter(|heading| heading.level >= 2)
    }

    /// The innermost section that holds a line.
    fn heading_of(&self, line: usize) -> Option<&Heading> {
        let after = self
            .headings
            .partition_point(|heading| heading.line <= line);
        after.checked_sub(1).map(|i| &self.headings[i])
    }

    /// The sections that hold a line, outermost first:
    /// `Feature definitions > extrude`.
    fn path(&self, line: usize) -> String {
        self.sections()
            .filter(|heading| heading.line <= line && line < heading.end)
            .map(|heading| heading.title.as_str())
            .collect::<Vec<_>>()
            .join(" > ")
    }

    fn text(&self, heading: &Heading) -> String {
        self.lines[heading.line..heading.end].join("\n")
    }

    /// The sections a name finds, best first: the heading itself (as shown
    /// or in full, with its issue numbers), then headings that list it,
    /// then headings that contain it.
    fn find_sections(&self, name: &str) -> Vec<&Heading> {
        let wanted = normal(name);
        let exact =
            |heading: &Heading| normal(&heading.title) == wanted || normal(&heading.full) == wanted;
        let lists = |heading: &Heading| listed(&heading.title).contains(&wanted);
        let contains = |heading: &Heading| {
            normal(&heading.title).contains(&wanted) || normal(&heading.full).contains(&wanted)
        };
        let tiers: [&dyn Fn(&Heading) -> bool; 3] = [&exact, &lists, &contains];
        for tier in tiers {
            let found: Vec<&Heading> = self.sections().filter(|heading| tier(heading)).collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// The section of a feature type: the heading that lists it, else the
    /// section of the first example definition of that type.
    fn feature_section(&self, name: &str) -> Option<&Heading> {
        let wanted = normal(name);
        if let Some(heading) = self
            .sections()
            .find(|heading| listed(&heading.title).contains(&wanted))
        {
            return Some(heading);
        }
        let line = (0..self.lines.len()).find(|&i| self.code[i] && self.defines(i, name))?;
        self.heading_of(line)
    }

    /// Whether a line of code is a definition of the feature type.
    fn defines(&self, line: usize, name: &str) -> bool {
        let text = &self.lines[line];
        text.contains(&format!("\"type\": \"{name}\""))
            || text.contains(&format!("\"type\":\"{name}\""))
    }

    /// The first code block in a section with a definition of the feature
    /// type, without its fences.
    fn example(&self, heading: &Heading, name: &str) -> Option<String> {
        let line = (heading.line..heading.end).find(|&i| {
            self.code[i] && !self.lines[i].trim_start().starts_with("```") && self.defines(i, name)
        })?;
        let fence = |i: &usize| self.lines[*i].trim_start().starts_with("```");
        let start = (heading.line..line)
            .rev()
            .find(fence)
            .map_or(line, |i| i + 1);
        let end = (line..heading.end).find(fence).unwrap_or(heading.end);
        Some(self.lines[start..end].join("\n"))
    }

    /// The table rows of a query or a command (`kind` `query` or
    /// `command`), each table with its header and the sections it is in.
    fn rows(&self, kind: &str, name: &str) -> Vec<String> {
        self.row_lines(kind, name)
            .into_iter()
            .map(|(header, rows)| {
                format!(
                    "{}\n\n{}\n{}\n{}",
                    self.path(header),
                    self.lines[header],
                    self.lines
                        .get(header + 1)
                        .map(String::as_str)
                        .unwrap_or_default(),
                    rows.iter()
                        .map(|&row| self.lines[row].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            })
            .collect()
    }

    /// The lines of [`Self::rows`]: each table's header line and its rows'
    /// lines. A row is the name's in a table whose first header cell names
    /// the kind (`Command`, `Query`), or in any table when its first cell
    /// is the kind and the name (`` command `set_table` ``, mitcad#1897).
    fn row_lines(&self, kind: &str, name: &str) -> Vec<(usize, Vec<usize>)> {
        let names = |cell: &str| {
            cell.split('`')
                .skip(1)
                .step_by(2)
                .any(|token| token.trim() == name)
        };
        let labelled = format!("{kind} {name}");
        let labels = |cell: &str| normal(cell) == labelled;
        let mut tables = Vec::new();
        let mut i = 0;
        while i < self.lines.len() {
            let starts = self.lines[i].starts_with('|')
                && !self.code[i]
                && (i == 0 || !self.lines[i - 1].starts_with('|'));
            if !starts {
                i += 1;
                continue;
            }
            let header = i;
            let mut end = i + 1;
            while end < self.lines.len() && self.lines[end].starts_with('|') {
                end += 1;
            }
            let of_kind = normal(first_cell(&self.lines[header])).contains(kind);
            let rows: Vec<usize> = (header + 2..end)
                .filter(|&row| {
                    let cell = first_cell(&self.lines[row]);
                    (of_kind && names(cell)) || labels(cell)
                })
                .collect();
            if !rows.is_empty() {
                tables.push((header, rows));
            }
            i = end;
        }
        tables
    }

    /// The JSON key that names a query or a command (`kind`) in an
    /// example: `"query": "bodies"`, `"cmd": "add_feature"`.
    fn key(kind: &str) -> &'static str {
        if kind == "query" { "query" } else { "cmd" }
    }

    /// Whether a block names a query or a command (mitcad#1897): prose and
    /// tables with it in backticks, code with it as its key's value.
    fn block_names(&self, block: &Block, kind: &str, name: &str) -> bool {
        let lines = &self.lines[block.lines.clone()];
        if block.kind == BlockKind::Code {
            let key = Self::key(kind);
            let spaced = format!("\"{key}\": \"{name}\"");
            let tight = format!("\"{key}\":\"{name}\"");
            lines
                .iter()
                .any(|line| line.contains(&spaced) || line.contains(&tight))
        } else {
            let quoted = format!("`{name}`");
            lines.iter().any(|line| line.contains(&quoted))
        }
    }

    /// The blocks that name a query or a command, in order.
    fn naming(&self, kind: &str, name: &str) -> Vec<usize> {
        (0..self.blocks.len())
            .filter(|&k| self.block_names(&self.blocks[k], kind, name))
            .collect()
    }

    /// The innermost heading of a block, by its index.
    fn owner(&self, block: usize) -> Option<usize> {
        let line = self.blocks[block].lines.start;
        self.headings
            .partition_point(|heading| heading.line <= line)
            .checked_sub(1)
    }

    /// The heading of the section that describes what these blocks name:
    /// the one with most of them, a code example counting three (the
    /// first on a tie).
    fn home(&self, naming: &[usize]) -> Option<usize> {
        let mut weights: Vec<(usize, usize)> = Vec::new();
        for &block in naming {
            let Some(heading) = self.owner(block) else {
                continue;
            };
            let weight = if self.blocks[block].kind == BlockKind::Code {
                3
            } else {
                1
            };
            match weights.iter_mut().find(|(known, _)| *known == heading) {
                Some(entry) => entry.1 += weight,
                None => weights.push((heading, weight)),
            }
        }
        let best = weights.iter().map(|(_, weight)| *weight).max()?;
        weights
            .iter()
            .find(|(_, weight)| *weight == best)
            .map(|(heading, _)| *heading)
    }

    /// Whether a block's last line ends with a colon: it introduces what
    /// follows.
    fn introduces(&self, block: usize) -> bool {
        self.lines[self.blocks[block].lines.clone()]
            .last()
            .is_some_and(|line| line.trim_end().ends_with(':'))
    }

    /// The blocks that name something with what belongs to them
    /// (mitcad#1897), in order: the paragraph that introduces a code
    /// block, a table or a list (ending with a colon), the code blocks,
    /// tables and list items a paragraph introduces, the code block a list
    /// item introduces, and the paragraph that follows a code example.
    fn entry_blocks(&self, naming: &[usize]) -> Vec<usize> {
        let count = self.blocks.len();
        // Whether block `b` follows block `a` in the same section.
        let next = |a: usize, b: usize| b == a + 1 && b < count && self.owner(a) == self.owner(b);
        let kind = |block: usize| self.blocks[block].kind;
        let mut picked = std::collections::BTreeSet::new();
        for &block in naming {
            picked.insert(block);
            if kind(block) != BlockKind::Paragraph {
                let mut first = block;
                while first > 0 && next(first - 1, first) && kind(first - 1) != BlockKind::Paragraph
                {
                    first -= 1;
                }
                if first > 0
                    && next(first - 1, first)
                    && kind(first - 1) == BlockKind::Paragraph
                    && self.introduces(first - 1)
                {
                    picked.insert(first - 1);
                }
            }
            match kind(block) {
                BlockKind::Paragraph if self.introduces(block) => {
                    let mut after = block;
                    while next(after, after + 1) && kind(after + 1) != BlockKind::Paragraph {
                        after += 1;
                        picked.insert(after);
                    }
                }
                BlockKind::Item if self.introduces(block) => {
                    let mut after = block;
                    while next(after, after + 1) && kind(after + 1) == BlockKind::Code {
                        after += 1;
                        picked.insert(after);
                    }
                }
                BlockKind::Code
                    if next(block, block + 1) && kind(block + 1) == BlockKind::Paragraph =>
                {
                    picked.insert(block + 1);
                }
                _ => {}
            }
        }
        picked.into_iter().collect()
    }

    /// The blocks as text, each section's under a heading line with its
    /// path.
    fn render(&self, blocks: &[usize]) -> String {
        let mut out = String::new();
        let mut section = None;
        for &block in blocks {
            let owner = self.owner(block);
            if owner != section {
                if let Some(heading) = owner.map(|h| &self.headings[h]) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    let path = self.path(heading.line);
                    let label = if path.is_empty() {
                        heading.title.as_str()
                    } else {
                        path.as_str()
                    };
                    out.push_str(&"#".repeat(heading.level));
                    out.push(' ');
                    out.push_str(label);
                    out.push('\n');
                }
                section = owner;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&self.lines[self.blocks[block].lines.clone()].join("\n"));
            out.push('\n');
        }
        out
    }

    /// The sections that hold these blocks, by their paths, in order and
    /// once each, at most [`ALSO_IN`].
    fn paths_of(&self, blocks: &[usize]) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        for &block in blocks {
            let path = self.path(self.blocks[block].lines.start);
            if !path.is_empty() && !paths.contains(&path) {
                paths.push(path);
            }
            if paths.len() == ALSO_IN {
                break;
            }
        }
        paths
    }

    /// A query's or a command's entry (mitcad#1897): its table rows, else
    /// the section that describes it in prose; None when the reference
    /// names it nowhere.
    fn item_entry(&self, kind: &str, name: &str) -> Option<ItemEntry> {
        let tables = self.row_lines(kind, name);
        let naming = self.naming(kind, name);
        if let Some((header, _)) = tables.first() {
            let rows: Vec<usize> = tables
                .iter()
                .flat_map(|(_, rows)| rows.iter().copied())
                .collect();
            // The rows' own tables are in the text already.
            let others: Vec<usize> = naming
                .into_iter()
                .filter(|&block| {
                    !self.blocks[block]
                        .lines
                        .clone()
                        .any(|line| rows.contains(&line))
                })
                .collect();
            let text = self.rows(kind, name).join("\n\n");
            let more = self.render(&self.entry_blocks(&others));
            let extended = if more.is_empty() {
                text.clone()
            } else {
                format!("{text}\n\n{more}")
            };
            return Some(ItemEntry {
                section: self
                    .heading_of(*header)
                    .map(|heading| (heading.title.clone(), self.path(*header))),
                text,
                extended,
                extract: false,
                rows: true,
                also_in: self.paths_of(&others),
            });
        }
        let home = &self.headings[self.home(&naming)?];
        let inside = home.line..home.end;
        let (own, others): (Vec<usize>, Vec<usize>) = naming
            .into_iter()
            .partition(|&block| inside.contains(&self.blocks[block].lines.start));
        let mut whole = self.text(home);
        whole.push('\n');
        let extract = whole.len() > ENTRY_WHOLE;
        let text = if extract {
            self.render(&self.entry_blocks(&own))
        } else {
            whole.clone()
        };
        let more = self.render(&self.entry_blocks(&others));
        let extended = if more.is_empty() {
            whole
        } else {
            format!("{whole}\n{more}")
        };
        Some(ItemEntry {
            section: Some((home.title.clone(), self.path(home.line))),
            text,
            extended,
            extract,
            rows: false,
            also_in: self.paths_of(&others),
        })
    }

    /// The lines that contain a text, in any case.
    fn lines_with(&self, text: &str) -> Vec<usize> {
        let wanted = text.to_lowercase();
        (0..self.lines.len())
            .filter(|&i| self.lines[i].to_lowercase().contains(&wanted))
            .collect()
    }

    /// A section as an answer: its text from `offset`, at most `limit`
    /// bytes; when cut, its subsections, which are smaller to ask for.
    ///
    /// Without `extended` the section comes in brief (mitcad#1587, see
    /// [`Reference::brief`]) when that is shorter, with `brief: true`, its
    /// subsections and a note of the whole text's size.
    fn section_answer(
        &self,
        heading: &Heading,
        offset: usize,
        limit: usize,
        extended: bool,
    ) -> Result<Value, String> {
        let whole = self.text(heading);
        let brief = (!extended)
            .then(|| self.brief(heading))
            .filter(|brief| brief.len() < whole.len());
        let (text, next) = cut(brief.as_deref().unwrap_or(&whole), offset, limit)?;
        let mut answer = json!({
            "section": heading.title,
            "path": self.path(heading.line),
            "text": text,
        });
        let subsections = self.subsections(heading);
        if (brief.is_some() || next.is_some()) && !subsections.is_empty() {
            answer["subsections"] = json!(subsections);
        }
        if brief.is_some() {
            answer["brief"] = json!(true);
            answer["note"] = json!(format!(
                "in brief: the example, the tables and each field's text up to {BRIEF_ITEM} bytes; nested details, later examples and the subsections' texts are left out. The same call with extended: true gives the whole section ({} kB), and section with a subsection's heading gives that one",
                whole.len().div_ceil(1024)
            ));
        }
        mark_cut(&mut answer, next);
        Ok(answer)
    }

    /// The headings one level below a section's.
    fn subsections(&self, heading: &Heading) -> Vec<&str> {
        self.headings
            .iter()
            .filter(|sub| {
                sub.line > heading.line && sub.line < heading.end && sub.level == heading.level + 1
            })
            .map(|sub| sub.title.as_str())
            .collect()
    }

    /// A section in brief (mitcad#1587): its heading and its own text (not
    /// its subsections') with the first code block, the tables, and each
    /// paragraph and top-level list item up to [`BRIEF_ITEM`] bytes, cut
    /// after a sentence; nested list items and later code blocks are left
    /// out, and so are parenthesized issue numbers.
    fn brief(&self, heading: &Heading) -> String {
        let index = self
            .headings
            .iter()
            .position(|other| other.line == heading.line)
            .unwrap_or_default();
        let own = heading.line..self.own_end(index).min(heading.end);
        let mut out = String::new();
        // The paragraph or top-level list item being read.
        let mut item = String::new();
        let mut nested = false;
        let mut blocks = 0;
        let mut fenced = false;
        let flush = |item: &mut String, out: &mut String| {
            if !item.is_empty() {
                out.push_str(&shorten(&without_issue_numbers(item)));
                out.push('\n');
                item.clear();
            }
        };
        for i in own {
            let line = &self.lines[i];
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if self.code[i] {
                flush(&mut item, &mut out);
                if trimmed.starts_with("```") {
                    if !fenced {
                        blocks += 1;
                    }
                    fenced = !fenced;
                }
                if blocks == 1 {
                    out.push_str(line);
                    out.push('\n');
                }
                continue;
            }
            if i == heading.line {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if trimmed.is_empty() {
                flush(&mut item, &mut out);
                if !out.ends_with("\n\n") {
                    out.push('\n');
                }
                continue;
            }
            if trimmed.starts_with('|') {
                flush(&mut item, &mut out);
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let bullet = trimmed.starts_with("- ") || trimmed.starts_with("* ");
            if bullet && indent >= 2 {
                flush(&mut item, &mut out);
                nested = true;
                continue;
            }
            if bullet {
                flush(&mut item, &mut out);
                nested = false;
                item.push_str(trimmed);
                continue;
            }
            if nested && indent >= 3 {
                continue;
            }
            nested = false;
            if !item.is_empty() {
                item.push(' ');
            }
            item.push_str(trimmed);
        }
        flush(&mut item, &mut out);
        let subsections = self.subsections(heading);
        if !subsections.is_empty() {
            out.push_str("\nSubsections (ask for one by its heading with section): ");
            out.push_str(&subsections.join("; "));
            out.push('\n');
        }
        out.trim_end().to_owned() + "\n"
    }
}

/// A paragraph or list item cut to [`BRIEF_ITEM`] bytes, after the last
/// sentence that ends before that (else after a clause or a word), with
/// an ellipsis.
fn shorten(item: &str) -> String {
    if item.len() <= BRIEF_ITEM {
        return item.to_owned();
    }
    let mut end = BRIEF_ITEM;
    while !item.is_char_boundary(end) {
        end -= 1;
    }
    let head = &item[..end];
    let mut sentence = None;
    let mut clause = None;
    let mut code = false;
    let mut depth = 0i32;
    let bytes = head.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        match byte {
            b'`' => code = !code,
            b'(' | b'[' | b'{' if !code => depth += 1,
            b')' | b']' | b'}' if !code => depth -= 1,
            b'.' | b':' if !code && depth <= 0 => {
                let next = bytes.get(i + 1).copied();
                let before = &head[..i];
                let abbreviation = before.ends_with("e.g") || before.ends_with("i.e");
                if next == Some(b' ') && !abbreviation {
                    sentence = Some(i + 1);
                }
            }
            b';' | b',' if !code && depth <= 0 => clause = Some(i + 1),
            _ => {}
        }
    }
    let at = sentence
        .filter(|&at| at >= BRIEF_ITEM / 3)
        .or(clause.filter(|&at| at >= BRIEF_ITEM / 2))
        .unwrap_or_else(|| head.rfind(' ').unwrap_or(end));
    format!("{} …", head[..at].trim_end())
}

/// Text without its parenthesized issue numbers: `(mitcad#714)`,
/// `(mitcad#547; mitcad#539)`.
fn without_issue_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')').map(|close| open + close) else {
            break;
        };
        let inner = &rest[open + 1..close];
        let only_issues = inner.contains("mitcad#")
            && inner
                .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
                .filter(|word| !word.is_empty() && *word != "and")
                .all(|word| {
                    word.strip_prefix("mitcad#")
                        .is_some_and(|number| number.chars().all(|c| c.is_ascii_digit()))
                });
        if only_issues {
            out.push_str(rest[..open].trim_end_matches(' '));
            rest = &rest[close + 1..];
        } else {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
        }
    }
    out.push_str(rest);
    out
}

/// The answer of `model_reference` to its (validated) arguments, in the
/// headless server.
#[cfg(test)]
pub(crate) fn answer(arguments: &Value, vocabulary: &Vocabulary) -> Result<Value, String> {
    answer_in(arguments, vocabulary, false)
}

/// The answer of `model_reference` to its (validated) arguments; `live`
/// for the live server, whose commands' schemas say its restrictions
/// (mitcad#1897).
pub(crate) fn answer_in(
    arguments: &Value,
    vocabulary: &Vocabulary,
    live: bool,
) -> Result<Value, String> {
    let offset = arguments
        .get("offset")
        .and_then(Value::as_u64)
        .map_or(0, |offset| usize::try_from(offset).unwrap_or(usize::MAX));
    // A section or a feature type in brief unless asked whole (mitcad#1587).
    let extended = arguments
        .get("extended")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let given: Vec<&str> = ["section", "query", "command", "feature", "search", "guide"]
        .into_iter()
        .filter(|key| arguments.get(*key).is_some())
        .collect();
    let text = |key: &str| {
        arguments[key]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let reference = view(vocabulary);
    let reference: &Reference = &reference;
    match given.as_slice() {
        [] => contents(reference, vocabulary, offset),
        ["section"] => section(reference, &text("section"), offset, extended),
        ["query"] => item(
            reference,
            Lookup {
                kind: "query",
                name: &text("query"),
                names: &vocabulary.queries,
                offset,
                extended,
                live,
            },
            vocabulary,
        ),
        ["command"] => item(
            reference,
            Lookup {
                kind: "command",
                name: &text("command"),
                names: &vocabulary.commands,
                offset,
                extended,
                live,
            },
            vocabulary,
        ),
        ["feature"] => feature(reference, &text("feature"), vocabulary, offset, extended),
        ["search"] => search(reference, &text("search"), offset),
        // How to model well rather than what exists (mitcad#1508).
        ["guide"] => crate::guide::answer(&text("guide"), offset),
        _ => Err(format!(
            "give one of section, query, command, feature, search and guide, not {}",
            given.join(" and ")
        )),
    }
}

fn contents(
    reference: &Reference,
    vocabulary: &Vocabulary,
    offset: usize,
) -> Result<Value, String> {
    let outline: String = reference
        .sections()
        .filter(|heading| heading.level <= 3)
        .map(|heading| {
            let indent = if heading.level == 3 { "  " } else { "" };
            format!("{indent}{}\n", heading.title)
        })
        .collect();
    // The guides on how to model well (mitcad#1508).
    let guides = crate::guide::guides();
    // The names and the guides count towards the limit too.
    let names: usize = [
        &vocabulary.queries,
        &vocabulary.commands,
        &vocabulary.features,
    ]
    .into_iter()
    .flatten()
    .map(|name| name.len() + 3)
    .sum();
    // The commands with a JSON Schema (mitcad#1897): their names only.
    let schemas = crate::schemas::schema_commands(vocabulary);
    let others = names + guides.to_string().len() + schemas.len() * 24;
    let (text, next) = cut(&outline, offset, LIMIT.saturating_sub(others).max(4096))?;
    let mut answer = json!({
        "contents": text,
        "guides": guides,
        "queries": vocabulary.queries,
        "commands": vocabulary.commands,
        "features": vocabulary.features,
        "schemas": schemas,
        "usage": USAGE,
    });
    mark_cut(&mut answer, next);
    Ok(answer)
}

fn section(
    reference: &Reference,
    name: &str,
    offset: usize,
    extended: bool,
) -> Result<Value, String> {
    let found = reference.find_sections(name);
    let Some(heading) = found.first() else {
        return Err(format!(
            "no section {name} in the reference; model_reference without arguments lists the sections, and with search finds a text"
        ));
    };
    let mut answer = reference.section_answer(heading, offset, LIMIT, extended)?;
    if found.len() > 1 {
        let others: Vec<&str> = found[1..]
            .iter()
            .take(20)
            .map(|heading| heading.title.as_str())
            .collect();
        answer["other_sections"] = json!(others);
    }
    Ok(answer)
}

/// A query or a command looked up.
struct Lookup<'a> {
    /// `query` or `command`.
    kind: &'a str,
    name: &'a str,
    /// The names of its kind MCP permits.
    names: &'a [String],
    offset: usize,
    extended: bool,
    /// The live server's lookup (a command's schema says its
    /// restrictions).
    live: bool,
}

/// A query's or a command's entry (mitcad#1897, [`ItemEntry`]): its
/// reference rows, else the section that describes it in prose, with that
/// section and its path, the sections whose headings list it and the other
/// sections that name it; a command's JSON Schema where one exists.
/// `brief` says that `extended: true` gives more, `truncated` that the
/// text was cut for size.
fn item(
    reference: &Reference,
    lookup: Lookup<'_>,
    vocabulary: &Vocabulary,
) -> Result<Value, String> {
    let Lookup {
        kind,
        name,
        names,
        offset,
        extended,
        live,
    } = lookup;
    let permitted = names.iter().any(|known| known == name);
    let sections: Vec<&str> = reference
        .listing(name)
        .into_iter()
        .map(|heading| heading.title.as_str())
        .collect();
    let Some(entry) = reference.item_entry(kind, name).or_else(|| {
        // A name only a heading lists: no text of its own.
        (!sections.is_empty()).then(|| ItemEntry {
            section: None,
            text: String::new(),
            extended: String::new(),
            extract: false,
            rows: false,
            also_in: Vec::new(),
        })
    }) else {
        let plural = if kind == "query" {
            "queries"
        } else {
            "commands"
        };
        // One the agents' view leaves out (mitcad#1509).
        if !permitted && self::reference().item_entry(kind, name).is_some() {
            return Err(unavailable(kind, plural, name, names));
        }
        return Err(unknown(kind, plural, name, names));
    };
    let brief = !extended && entry.extended.len() > entry.text.len();
    let whole = if extended {
        &entry.extended
    } else {
        &entry.text
    };
    let (text, next) = cut(whole, offset, LIMIT)?;
    let mut answer = json!({kind: name, "text": text, "sections": sections, "brief": brief});
    if let Some((title, path)) = &entry.section {
        answer["section"] = json!(title);
        answer["path"] = json!(path);
    }
    if !extended && !entry.also_in.is_empty() {
        answer["also_in"] = json!(entry.also_in);
    }
    if !vocabulary.is_empty() {
        answer["permitted"] = json!(permitted);
        if !permitted {
            add_note(
                &mut answer,
                &format!("this {kind} is not available through MCP"),
            );
        }
    }
    if brief {
        let size = entry.extended.len().div_ceil(1024);
        let what = if entry.rows {
            "its table rows; the same call with extended: true adds the paragraphs and examples elsewhere in the reference that name it"
        } else if entry.extract {
            "the paragraphs, examples and lists of its section that describe it; the same call with extended: true gives the whole section and what other sections say of it"
        } else {
            "the whole section that describes it; the same call with extended: true adds what other sections (also_in) say of it"
        };
        add_note(&mut answer, &format!("in brief: {what} ({size} kB in all)"));
    }
    // Its JSON Schema, on demand only (mitcad#1897).
    if kind == "command"
        && let Some(schema) = crate::schemas::command_schema(name, vocabulary, live)
    {
        answer["schema"] = schema;
    }
    mark_cut(&mut answer, next);
    Ok(answer)
}

fn feature(
    reference: &Reference,
    name: &str,
    vocabulary: &Vocabulary,
    offset: usize,
    extended: bool,
) -> Result<Value, String> {
    let Some(heading) = reference.feature_section(name) else {
        return Err(unknown(
            "feature type",
            "feature types",
            name,
            &vocabulary.features,
        ));
    };
    let mut answer = reference.section_answer(heading, offset, LIMIT, extended)?;
    answer["feature"] = json!(name);
    if let Some(example) = reference.example(heading, name) {
        answer["example"] = json!(example);
    }
    if !vocabulary.features.is_empty() {
        answer["permitted"] = json!(vocabulary.features.iter().any(|known| known == name));
    }
    Ok(answer)
}

/// The sections whose headings contain a text, and the lines that do, with
/// their sections; `skip` hits left out (a later page).
fn search(reference: &Reference, text: &str, skip: usize) -> Result<Value, String> {
    let wanted = text.to_lowercase();
    let headings: Vec<&str> = reference
        .sections()
        .filter(|heading| heading.title.to_lowercase().contains(&wanted))
        .map(|heading| heading.title.as_str())
        .collect();
    let lines = reference.lines_with(text);
    let mut hits = Vec::new();
    let mut size = 0;
    let mut next = None;
    for (n, &line) in lines.iter().enumerate().skip(skip) {
        let hit = json!({
            "section": reference.path(line),
            "excerpt": excerpt(&reference.lines[line], &wanted),
        });
        let length = hit.to_string().len();
        if hits.len() == MAX_HITS || size + length > LIMIT {
            next = Some(n);
            break;
        }
        size += length;
        hits.push(hit);
    }
    let mut answer = json!({
        "search": text,
        "total": lines.len(),
        "headings": headings,
        "hits": hits,
        "truncated": next.is_some(),
    });
    if let Some(next) = next {
        answer["next_offset"] = json!(next);
        answer["note"] = json!(format!(
            "{} of {} hits: call again with offset {next} for the next ones, or search a longer text",
            next - skip.min(next),
            lines.len()
        ));
    }
    Ok(answer)
}

/// The name of a refused parameter that is a unit, a function or a
/// constant, from the model's error (`Round is a unit, function or
/// constant and cannot name a parameter`).
fn reserved_parameter_name(error: &str) -> Option<&str> {
    let (before, _) =
        error.split_once(" is a unit, function or constant and cannot name a parameter")?;
    let name = before
        .rsplit(|c: char| c.is_whitespace() || c == ':')
        .next()?;
    (!name.is_empty()).then_some(name)
}

/// Adds the reference to a refused `model_command`'s result (`value`)
/// whose error is a missing or an unknown field (mitcad#1587: unknown ones
/// too): the section of the feature type its definition names, in brief,
/// with an example definition, or the rows of its command when the field
/// is one of the command's own. The model's error names only the first
/// such field; the reference gives them all in one answer. From the
/// reference as the agents of `vocabulary` see it. A few refusals say
/// where the thing asked for is instead (a definition's `name`, a
/// parameter named like a function).
pub(crate) fn explain_refusal(arguments: &Value, value: &mut Value, vocabulary: &Vocabulary) {
    let Some(error) = value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return;
    };
    if let Some(name) = reserved_parameter_name(&error) {
        value["error"] = json!(format!(
            "{error}; parameter names may not be units, functions or constants (round, min, mm, pi, …): choose another, e.g. {name}Value"
        ));
        return;
    }
    let field = |marker: &str| {
        error
            .split(marker)
            .nth(1)
            .and_then(|rest| rest.split('`').next())
            .map(str::to_owned)
    };
    let (field, unknown) = match field("missing field `") {
        Some(field) => (field, false),
        None => match field("unknown field `") {
            Some(field) => (field, true),
            None => return,
        },
    };
    let command = match &arguments["command"] {
        Value::Array(entries) => value
            .get("entry")
            .and_then(Value::as_u64)
            .and_then(|i| entries.get(usize::try_from(i).ok()?)),
        command => Some(command),
    };
    let Some(command) = command.filter(|command| command.is_object()) else {
        return;
    };
    let reference = view(vocabulary);
    let name = command
        .get("cmd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Its rows, or the section that describes it in prose (mitcad#1897).
    let rows = if name.is_empty() {
        String::new()
    } else {
        reference
            .item_entry("command", name)
            .map(|entry| entry.text)
            .unwrap_or_default()
    };
    // A missing field the command's rows name is the command's; an unknown
    // one is the command's when the command itself has it.
    let of_command = if unknown {
        command.get(&field).is_some()
    } else {
        rows.contains(&format!("`{field}`"))
    };
    let feature = command
        .pointer("/def/type")
        .and_then(Value::as_str)
        .filter(|_| !of_command)
        .and_then(|kind| Some((kind, reference.feature_section(kind)?)));
    let (hint, mut entry, text) = if let Some((kind, heading)) = feature {
        let mut entry = json!({"feature": kind, "section": heading.title});
        if let Some(example) = reference.example(heading, kind) {
            entry["example"] = json!(example);
        }
        let mut hint = format!(
            "the fields of the feature type {kind} and an example are in reference; model_reference {{\"feature\": \"{kind}\"}} gives its section"
        );
        if unknown && field == "name" {
            hint = format!(
                "a feature's name is not part of its definition: give it beside def ({{\"cmd\": \"add_feature\", \"name\": \"…\", \"def\": {{…}}}}), {}or later with rename_feature; {hint}",
                if kind == "sketch" {
                    "with sketch.create's name, "
                } else {
                    ""
                }
            );
        }
        (hint, entry, reference.brief(heading))
    } else if !rows.is_empty() {
        (
            format!(
                "the fields of {name} are in reference; model_reference {{\"command\": \"{name}\"}} gives them"
            ),
            json!({"command": name}),
            rows,
        )
    } else {
        return;
    };
    let (text, next) = cut(&text, 0, ERROR_LIMIT).unwrap_or_default();
    entry["text"] = json!(text);
    entry["truncated"] = json!(next.is_some());
    value["error"] = json!(format!("{error}; {hint}"));
    value["reference"] = entry;
}
