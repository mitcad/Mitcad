// SPDX-License-Identifier: MIT
//! The command reference in parts (mitcad#714). The tool `model_reference`
//! answers with the reference's contents and the names MCP permits, one
//! section, the rows of a query or a command, a feature type's section or
//! the lines that contain a text, each with at most [`LIMIT`] bytes of
//! text, where the resource holds the whole reference (about 1 MB, more
//! than an agent's context). Errors give the permitted names, and a
//! refused command that misses fields gets the reference of its feature
//! type or command.

use std::borrow::Cow;
use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::COMMANDS_REFERENCE;

/// The most text one answer holds, in bytes.
pub const LIMIT: usize = 64 * 1024;
/// The most text of a feature type's or a command's reference that a
/// refused command's result holds.
const ERROR_LIMIT: usize = 12 * 1024;
/// The most search hits in one answer.
const MAX_HITS: usize = 200;
/// A search hit's excerpt, in characters.
const EXCERPT: usize = 240;

const USAGE: &str = "model_reference with section (a heading, or a name a heading lists), query, command, feature (a feature type) or search gives that part of the reference; offset continues a truncated answer from its next_offset (for search: the hits to skip).";

/// The names MCP permits, which the backend supplies
/// (`mitcad_model_api::classes`): the reference does not tell them.
#[derive(Clone, Debug, Default)]
pub struct Vocabulary {
    pub queries: Vec<String>,
    pub commands: Vec<String>,
    pub features: Vec<String>,
}

impl Vocabulary {
    fn is_empty(&self) -> bool {
        self.queries.is_empty() && self.commands.is_empty() && self.features.is_empty()
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
    let name = echoed(name);
    if names.is_empty() {
        return format!(
            "unknown {kind} '{name}'; model_reference without arguments lists the {plural}"
        );
    }
    format!(
        "unknown {kind} '{name}'; the {plural} MCP permits: {}. model_reference {{\"{}\": \"<name>\"}} gives one's fields",
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
        format!("its '{key}' is not a nonempty string")
    } else if fields.is_empty() {
        "this one is empty".to_owned()
    } else if count > fields.len() {
        format!(
            "this one has no '{key}' (its fields: {}, and {} more)",
            fields.join(", "),
            count - fields.len()
        )
    } else {
        format!(
            "this one has no '{key}' (its fields: {})",
            fields.join(", ")
        )
    };
    format!(
        "{form}, for example {example}; {found}. model_reference without arguments lists the {plural}"
    )
}

struct Heading {
    level: usize,
    title: String,
    /// Its line.
    line: usize,
    /// The line after its section, subsections included.
    end: usize,
}

struct Reference {
    lines: Vec<&'static str>,
    /// Whether each line is a code fence or inside a code block.
    code: Vec<bool>,
    headings: Vec<Heading>,
}

fn reference() -> &'static Reference {
    static PARSED: OnceLock<Reference> = OnceLock::new();
    PARSED.get_or_init(|| Reference::parse(COMMANDS_REFERENCE))
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
fn cut(text: &str, offset: usize, limit: usize) -> Result<(String, Option<usize>), String> {
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

/// Says in `answer` whether its text was cut, and where to continue.
fn mark_cut(answer: &mut Value, next: Option<usize>) {
    answer["truncated"] = json!(next.is_some());
    if let Some(next) = next {
        answer["next_offset"] = json!(next);
        answer["note"] = json!(format!(
            "cut at {} kB: call again with the same arguments and offset {next} for the rest",
            LIMIT / 1024
        ));
    }
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
    fn parse(text: &'static str) -> Self {
        let lines: Vec<&'static str> = text.lines().collect();
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
                headings.push(Heading {
                    level,
                    title: line[level..].trim().to_owned(),
                    line: i,
                    end: lines.len(),
                });
            }
        }
        // A section ends at the next heading of its level or above.
        let ends: Vec<usize> = headings
            .iter()
            .enumerate()
            .map(|(i, heading)| {
                headings[i + 1..]
                    .iter()
                    .find(|next| next.level <= heading.level)
                    .map_or(lines.len(), |next| next.line)
            })
            .collect();
        for (heading, end) in headings.iter_mut().zip(ends) {
            heading.end = end;
        }
        Self {
            lines,
            code,
            headings,
        }
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

    /// The sections a name finds, best first: the heading itself, then
    /// headings that list it, then headings that contain it.
    fn find_sections(&self, name: &str) -> Vec<&Heading> {
        let wanted = normal(name);
        let exact = |heading: &Heading| normal(&heading.title) == wanted;
        let lists = |heading: &Heading| listed(&heading.title).contains(&wanted);
        let contains = |heading: &Heading| normal(&heading.title).contains(&wanted);
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
        let text = self.lines[line];
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
        let first_cell = |line: &str| line.split('|').nth(1).unwrap_or_default().to_owned();
        let names = |cell: &str| {
            cell.split('`')
                .skip(1)
                .step_by(2)
                .any(|token| token.trim() == name)
        };
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
            if normal(&first_cell(self.lines[header])).contains(kind) {
                let rows: Vec<&str> = (header + 2..end)
                    .map(|row| self.lines[row])
                    .filter(|row| names(&first_cell(row)))
                    .collect();
                if !rows.is_empty() {
                    tables.push(format!(
                        "{}\n\n{}\n{}\n{}",
                        self.path(header),
                        self.lines[header],
                        self.lines.get(header + 1).copied().unwrap_or_default(),
                        rows.join("\n")
                    ));
                }
            }
            i = end;
        }
        tables
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
    fn section_answer(
        &self,
        heading: &Heading,
        offset: usize,
        limit: usize,
    ) -> Result<Value, String> {
        let (text, next) = cut(&self.text(heading), offset, limit)?;
        let mut answer = json!({
            "section": heading.title,
            "path": self.path(heading.line),
            "text": text,
        });
        if next.is_some() {
            let subsections: Vec<&str> = self
                .headings
                .iter()
                .filter(|sub| {
                    sub.line > heading.line
                        && sub.line < heading.end
                        && sub.level == heading.level + 1
                })
                .map(|sub| sub.title.as_str())
                .collect();
            if !subsections.is_empty() {
                answer["subsections"] = json!(subsections);
            }
        }
        mark_cut(&mut answer, next);
        Ok(answer)
    }
}

/// The answer of `model_reference` to its (validated) arguments.
pub(crate) fn answer(arguments: &Value, vocabulary: &Vocabulary) -> Result<Value, String> {
    let offset = arguments
        .get("offset")
        .and_then(Value::as_u64)
        .map_or(0, |offset| usize::try_from(offset).unwrap_or(usize::MAX));
    let given: Vec<&str> = ["section", "query", "command", "feature", "search"]
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
    match given.as_slice() {
        [] => contents(vocabulary, offset),
        ["section"] => section(&text("section"), offset),
        ["query"] => item(
            "query",
            &text("query"),
            &vocabulary.queries,
            vocabulary,
            offset,
        ),
        ["command"] => item(
            "command",
            &text("command"),
            &vocabulary.commands,
            vocabulary,
            offset,
        ),
        ["feature"] => feature(&text("feature"), vocabulary, offset),
        ["search"] => search(&text("search"), offset),
        _ => Err(format!(
            "give one of section, query, command, feature and search, not {}",
            given.join(" and ")
        )),
    }
}

fn contents(vocabulary: &Vocabulary, offset: usize) -> Result<Value, String> {
    let reference = reference();
    let outline: String = reference
        .sections()
        .filter(|heading| heading.level <= 3)
        .map(|heading| {
            let indent = if heading.level == 3 { "  " } else { "" };
            format!("{indent}{}\n", heading.title)
        })
        .collect();
    // The names count towards the limit too.
    let names: usize = [
        &vocabulary.queries,
        &vocabulary.commands,
        &vocabulary.features,
    ]
    .into_iter()
    .flatten()
    .map(|name| name.len() + 3)
    .sum();
    let (text, next) = cut(&outline, offset, LIMIT.saturating_sub(names).max(4096))?;
    let mut answer = json!({
        "contents": text,
        "queries": vocabulary.queries,
        "commands": vocabulary.commands,
        "features": vocabulary.features,
        "usage": USAGE,
    });
    mark_cut(&mut answer, next);
    Ok(answer)
}

fn section(name: &str, offset: usize) -> Result<Value, String> {
    let reference = reference();
    let found = reference.find_sections(name);
    let Some(heading) = found.first() else {
        return Err(format!(
            "no section '{name}' in the reference; model_reference without arguments lists the sections, and with search finds a text"
        ));
    };
    let mut answer = reference.section_answer(heading, offset, LIMIT)?;
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

/// A query's or a command's reference rows (`kind`), with the sections
/// whose headings list it; without rows, the lines that name it.
fn item(
    kind: &str,
    name: &str,
    names: &[String],
    vocabulary: &Vocabulary,
    offset: usize,
) -> Result<Value, String> {
    let reference = reference();
    let permitted = names.iter().any(|known| known == name);
    let wanted = normal(name);
    let sections: Vec<&str> = reference
        .sections()
        .filter(|heading| listed(&heading.title).contains(&wanted))
        .map(|heading| heading.title.as_str())
        .collect();
    let tables = reference.rows(kind, name);
    let text = if tables.is_empty() {
        reference
            .lines_with(&format!("`{name}`"))
            .into_iter()
            .map(|line| {
                format!(
                    "{}: {}",
                    reference.path(line),
                    excerpt(reference.lines[line], &format!("`{name}`").to_lowercase())
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        tables.join("\n\n")
    };
    if text.is_empty() && sections.is_empty() {
        let plural = if kind == "query" {
            "queries"
        } else {
            "commands"
        };
        return Err(unknown(kind, plural, name, names));
    }
    let (text, next) = cut(&text, offset, LIMIT)?;
    let mut answer = json!({kind: name, "text": text, "sections": sections});
    if !vocabulary.is_empty() {
        answer["permitted"] = json!(permitted);
        if !permitted {
            answer["note"] = json!(format!("this {kind} is not available through MCP"));
        }
    }
    mark_cut(&mut answer, next);
    Ok(answer)
}

fn feature(name: &str, vocabulary: &Vocabulary, offset: usize) -> Result<Value, String> {
    let reference = reference();
    let Some(heading) = reference.feature_section(name) else {
        return Err(unknown(
            "feature type",
            "feature types",
            name,
            &vocabulary.features,
        ));
    };
    let mut answer = reference.section_answer(heading, offset, LIMIT)?;
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
fn search(text: &str, skip: usize) -> Result<Value, String> {
    let reference = reference();
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
            "excerpt": excerpt(reference.lines[line], &wanted),
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

/// Adds the reference to a refused `model_command`'s result (`value`)
/// whose error is a missing field: the section of the feature type its
/// definition names, with an example definition, or the rows of its
/// command when the missing field is one of the command's own. The model's
/// error names only the first missing field; the reference gives them all
/// in one answer.
pub(crate) fn explain_refusal(arguments: &Value, value: &mut Value) {
    let Some(error) = value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return;
    };
    let Some(field) = error
        .split("missing field `")
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .map(str::to_owned)
    else {
        return;
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
    let reference = reference();
    let name = command
        .get("cmd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let rows = if name.is_empty() {
        String::new()
    } else {
        reference.rows("command", name).join("\n\n")
    };
    let of_command = rows.contains(&format!("`{field}`"));
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
        (
            format!(
                "the fields of the feature type '{kind}' and an example are in reference; model_reference {{\"feature\": \"{kind}\"}} gives its whole section"
            ),
            entry,
            reference.text(heading),
        )
    } else if !rows.is_empty() {
        (
            format!(
                "the fields of '{name}' are in reference; model_reference {{\"command\": \"{name}\"}} gives them"
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
