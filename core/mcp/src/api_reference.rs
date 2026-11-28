// SPDX-License-Identifier: MIT
//! The generated API reference for users and integrators (mitcad#2606):
//! static HTML and Markdown pages of the model API, made from the same
//! sources as MCP's `model_reference`, so that `commands.md` stays the one
//! source of truth.
//!
//! The sources: the command reference (`core/model-api/src/commands.md`)
//! as the agents of a vocabulary see it (without the sections marked
//! internal, [`crate::reference::INTERNAL_MARKER`]), the commands' JSON
//! Schemas (`crate::schemas`) and the vocabulary itself: the queries,
//! commands and feature types `model_reference` lists. Every text is
//! cleaned (`clean`): no ticket references, no history, no test or file
//! names, no notes on the code inside; what is left out is reported
//! ([`ApiReference::report_text`]) so that its source can be fixed.
//!
//! The pages: an index with a search, an overview (commands, queries,
//! preview, undo, values and units, identities and topological names,
//! errors), one page per command, query and feature type (its purpose, its
//! fields, an example, its result and its refusals, then the reference's
//! own text of it) and one per section of the reference (`topics/`).
//! `tools/api-reference/build.sh` writes them (the example
//! `core/mcp/examples/api_reference.rs`); the tests in
//! `api_reference_tests.rs` are the check: every name has a page, and no
//! page names a ticket, history, a file or a test.

pub(crate) mod clean;
mod markdown;

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use serde_json::{Value, json};

use crate::reference::{self, Vocabulary};
use clean::Dropped;
use markdown::{Block, Ids, Item};

/// What a page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageKind {
    Index,
    Overview,
    Command,
    Query,
    Feature,
    Topic,
}

impl PageKind {
    /// The word for it in the index and the search.
    pub fn word(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Overview => "overview",
            Self::Command => "command",
            Self::Query => "query",
            Self::Feature => "feature type",
            Self::Topic => "topic",
        }
    }
}

/// A page of the reference.
pub struct Page {
    /// Its path without an extension, from the reference's folder:
    /// `commands/add_feature`, `topics/values`.
    pub path: String,
    /// Its title, as Markdown (`` `add_feature` ``).
    pub title: String,
    pub kind: PageKind,
    /// A sentence of what it is, for the index and the search.
    pub summary: String,
    /// The section it belongs to in the index: a command's or a query's
    /// section of the reference.
    pub group: String,
    blocks: Vec<Block>,
}

impl Page {
    /// From the page's folder to the reference's folder.
    fn root(&self) -> &'static str {
        if self.path.contains('/') { "../" } else { "" }
    }

    /// The page as Markdown.
    pub fn markdown(&self) -> String {
        markdown::to_markdown(&self.blocks, self.root())
    }

    /// The page as HTML, with the version of the sources.
    pub fn html(&self, version: &str) -> String {
        let root = self.root();
        let body = markdown::to_html(&self.blocks, root, &mut Ids::default());
        let search = if self.kind == PageKind::Index {
            format!(
                "<script src=\"{root}search-index.js\"></script>\n<script src=\"{root}search.js\"></script>\n"
            )
        } else {
            String::new()
        };
        format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{} – MitCad API reference</title>\n<link rel=\"stylesheet\" href=\"{root}style.css\">\n</head>\n<body>\n<header><a href=\"{root}index.html\">MitCad API reference</a> <span class=\"version\">{}</span></header>\n<main>\n{body}</main>\n<footer>Generated from the sources of MitCad {}.</footer>\n{search}</body>\n</html>\n",
            markdown::escape(&self.title.replace('`', "")),
            markdown::escape(version),
            markdown::escape(version),
        )
    }
}

/// A sentence the cleaning left out, on a page.
pub struct Omission {
    pub page: String,
    pub reason: &'static str,
    pub text: String,
}

/// The generated reference: its pages and what it left out.
pub struct ApiReference {
    /// The version of the sources (`0.48.0`).
    pub version: String,
    pub pages: Vec<Page>,
    pub omissions: Vec<Omission>,
    /// Names without what a page should have: `command x: no example`.
    pub gaps: Vec<String>,
}

/// The sections of the overview, in its order: how commands, queries,
/// preview and undo work, values and units, identities and names, and
/// errors.
pub const OVERVIEW: &[&str] = &[
    "Commands",
    "Queries",
    "Preview",
    "Undo, revisions and the saved state",
    "Values",
    "Identities and names",
    "References to geometry",
    "Errors: message, places and detail",
    "Errors",
];

/// The style sheet of the HTML pages.
const STYLE: &str = "body{font-family:system-ui,-apple-system,\"Segoe UI\",sans-serif;margin:0;color:#1d1d1f;background:#fff;line-height:1.5}\nheader{padding:.6rem 1.5rem;border-bottom:1px solid #ddd;background:#f6f6f7}\nheader a{font-weight:600;color:#1d1d1f;text-decoration:none}\n.version{color:#666;margin-left:.5rem}\nmain{max-width:60rem;margin:0 auto;padding:1rem 1.5rem 3rem}\nfooter{padding:1rem 1.5rem;color:#666;border-top:1px solid #ddd;font-size:.9rem}\ncode{font-family:ui-monospace,\"SF Mono\",Consolas,monospace;font-size:.92em;background:#f2f2f4;padding:0 .2em;border-radius:3px}\npre{background:#f2f2f4;padding:.8rem;overflow-x:auto;border-radius:4px}\npre code{background:none;padding:0}\n.table{overflow-x:auto}\ntable{border-collapse:collapse;margin:.5rem 0}\nth,td{border:1px solid #ddd;padding:.3rem .5rem;vertical-align:top;text-align:left}\nth{background:#f6f6f7}\na{color:#0a58ca}\n#search{width:100%;max-width:30rem;padding:.4rem;font-size:1rem}\n#results li{margin:.2rem 0}\n.kind{color:#666;font-size:.85rem}\n";

/// The search of the index page: the pages whose title and summary hold
/// every word typed.
const SEARCH: &str = "(function () {\n  var input = document.getElementById('search');\n  var results = document.getElementById('results');\n  var lists = document.getElementById('lists');\n  if (!input || !results || typeof MITCAD_API_INDEX === 'undefined') { return; }\n  function show() {\n    var words = input.value.toLowerCase().split(/\\s+/).filter(function (w) { return w; });\n    results.innerHTML = '';\n    lists.style.display = words.length ? 'none' : '';\n    if (!words.length) { return; }\n    var found = MITCAD_API_INDEX.filter(function (page) {\n      var text = (page.t + ' ' + page.s + ' ' + page.k).toLowerCase();\n      return words.every(function (w) { return text.indexOf(w) >= 0; });\n    });\n    found.sort(function (a, b) {\n      var at = a.t.toLowerCase().indexOf(words[0]) >= 0 ? 0 : 1;\n      var bt = b.t.toLowerCase().indexOf(words[0]) >= 0 ? 0 : 1;\n      return at - bt;\n    });\n    found.slice(0, 100).forEach(function (page) {\n      var li = document.createElement('li');\n      var a = document.createElement('a');\n      a.href = page.u;\n      a.textContent = page.t;\n      var kind = document.createElement('span');\n      kind.className = 'kind';\n      kind.textContent = ' ' + page.k + (page.s ? ': ' + page.s : '');\n      li.appendChild(a);\n      li.appendChild(kind);\n      results.appendChild(li);\n    });\n    if (!found.length) {\n      var none = document.createElement('li');\n      none.textContent = 'Nothing found.';\n      results.appendChild(none);\n    }\n  }\n  input.addEventListener('input', show);\n  show();\n})();\n";

/// The links of the reference's own headings (`#values`,
/// `#fitting-the-face-to-the-thread-mitcad1632`) and of its sections'
/// paths, to the pages that hold them.
#[derive(Default)]
struct Links {
    /// A heading's anchor in the developers' document, to `page:…`.
    anchors: BTreeMap<String, String>,
    /// A section's path (`Commands > Timeline and parameters`), to
    /// `page:…`.
    paths: BTreeMap<String, String>,
}

/// Cleans texts and collects what was left out, page by page.
struct Cleaner<'a> {
    links: &'a Links,
    page: String,
    omissions: Vec<Omission>,
}

impl Cleaner<'_> {
    fn note(&mut self, dropped: Vec<Dropped>) {
        for Dropped { reason, text } in dropped {
            self.omissions.push(Omission {
                page: self.page.clone(),
                reason,
                text,
            });
        }
    }

    /// Text with its links to the reference's headings made links to the
    /// pages (a link to a file or an unknown heading keeps its words),
    /// cleaned.
    fn text(&mut self, text: &str) -> String {
        let linked = self.relink(text);
        let mut dropped = Vec::new();
        let cleaned = clean::clean_text(&linked, &mut dropped);
        self.note(dropped);
        cleaned
    }

    fn relink(&self, text: &str) -> String {
        let mut out = String::new();
        let mut code = false;
        let mut i = 0;
        while i < text.len() {
            let c = text[i..].chars().next().unwrap_or(' ');
            if c == '`' {
                code = !code;
            }
            if !code
                && c == '['
                && let Some((label, url, end)) = markdown::link_at(text, i)
            {
                let target = if let Some(anchor) = url.strip_prefix('#') {
                    self.links.anchors.get(anchor).cloned()
                } else if url.starts_with("https://") || url.starts_with("page:") {
                    Some(url.to_owned())
                } else {
                    None
                };
                match target {
                    Some(target) => out.push_str(&format!("[{label}]({target})")),
                    None => out.push_str(label),
                }
                i = end;
                continue;
            }
            out.push(c);
            i += c.len_utf8();
        }
        out
    }

    /// Markdown made blocks and cleaned: empty paragraphs, items and
    /// rows go, a code block that names a ticket goes (reported).
    fn blocks(&mut self, text: &str) -> Vec<Block> {
        let mut out = Vec::new();
        for block in markdown::parse(text) {
            match block {
                Block::Heading { level, text } => {
                    let text = self.text(&text);
                    if !text.is_empty() {
                        out.push(Block::Heading { level, text });
                    }
                }
                Block::Paragraph(text) => {
                    let text = self.text(&text);
                    if !text.is_empty() {
                        out.push(Block::Paragraph(text));
                    }
                }
                Block::List(items) => {
                    let items: Vec<Item> = items
                        .into_iter()
                        .filter_map(|item| {
                            let text = self.text(&item.text);
                            (!text.is_empty()).then_some(Item { text, ..item })
                        })
                        .collect();
                    if !items.is_empty() {
                        out.push(Block::List(items));
                    }
                }
                Block::Table { header, rows } => {
                    let header: Vec<String> = header.iter().map(|cell| self.text(cell)).collect();
                    let rows: Vec<Vec<String>> = rows
                        .iter()
                        .map(|row| row.iter().map(|cell| self.text(cell)).collect::<Vec<_>>())
                        .filter(|row| row.first().is_some_and(|cell| !cell.is_empty()))
                        .collect();
                    if !rows.is_empty() {
                        out.push(Block::Table { header, rows });
                    }
                }
                Block::Code { lang, text } => {
                    if clean::names_ticket(&text) {
                        self.omissions.push(Omission {
                            page: self.page.clone(),
                            reason: clean::UNCLEAN,
                            text,
                        });
                    } else {
                        out.push(Block::Code { lang, text });
                    }
                }
            }
        }
        out
    }

    /// A section's path as shown (`Commands › Timeline and parameters`),
    /// linked to its page when it has one.
    fn path_link(&mut self, path: &str) -> String {
        let shown = self.text(&path.replace(" > ", " › "));
        match self.links.paths.get(path) {
            Some(target) => format!("[{shown}]({target})"),
            None => shown,
        }
    }
}

/// A page's path for a name: `commands/sketch.add_line`.
fn item_path(kind: PageKind, name: &str) -> String {
    let folder = match kind {
        PageKind::Command => "commands",
        PageKind::Query => "queries",
        PageKind::Feature => "features",
        _ => "topics",
    };
    format!("{folder}/{name}")
}

/// The first sentence of a text, at most about 200 characters.
fn first_sentence(text: &str) -> String {
    let sentence = clean::sentences(text)
        .into_iter()
        .next()
        .unwrap_or_default()
        .to_owned();
    if sentence.chars().count() <= 220 {
        return sentence;
    }
    // Cut, its links as their words, so that no link is cut in two, and
    // a code span cut in two closed.
    let sentence = unlinked(&sentence);
    let cut: String = sentence.chars().take(200).collect();
    let mut cut = match cut.rfind(' ') {
        Some(space) => cut[..space].to_owned(),
        None => cut,
    };
    if cut.matches('`').count() % 2 == 1 {
        cut.push('`');
    }
    cut.push('…');
    cut
}

/// Text with its links replaced by their words.
fn unlinked(text: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap_or(' ');
        if c == '`' {
            code = !code;
        }
        if !code
            && c == '['
            && let Some((label, _, end)) = markdown::link_at(text, i)
        {
            out.push_str(label);
            i = end;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// A JSON value as compact text without the quotes of a string.
fn plain(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// A field schema's type in words: `string`, `number or string`, `one of
/// `a`, `b``, `array of string`.
fn type_of(schema: &Value) -> String {
    if let Some(value) = schema.get("const") {
        return format!("`{}`", plain(value));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let values: Vec<String> = values
            .iter()
            .map(|value| format!("`{}`", plain(value)))
            .collect();
        return format!("one of {}", values.join(", "));
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(options) = schema.get(key).and_then(Value::as_array) {
            let mut kinds: Vec<String> = Vec::new();
            for kind in options.iter().map(type_of) {
                if !kind.is_empty() && !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
            return kinds.join(" or ");
        }
    }
    match schema.get("type") {
        Some(Value::String(kind)) if kind == "array" => match schema.get("items") {
            Some(items) if !type_of(items).is_empty() => format!("array of {}", type_of(items)),
            _ => "array".to_owned(),
        },
        Some(Value::String(kind)) => kind.clone(),
        Some(Value::Array(kinds)) => kinds.iter().map(plain).collect::<Vec<_>>().join(" or "),
        _ if schema.as_object().is_some_and(serde_json::Map::is_empty) => {
            "feature definition".to_owned()
        }
        _ => String::new(),
    }
}

/// The unit a field's text names: `mm`, `rad`, `mm or rad`, `°`, `mm²`.
fn unit_of(text: &str) -> String {
    let lower = text.to_lowercase();
    let word = |unit: &str| {
        lower.match_indices(unit).any(|(at, _)| {
            let before = lower[..at].chars().next_back();
            let after = lower[at + unit.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphabetic)
        })
    };
    for unit in ["mm³", "mm²", "g/cm³", "mm/s", "mm/min"] {
        if lower.contains(unit) {
            return unit.to_owned();
        }
    }
    let length = word("mm") || word("millimetres");
    let angle = word("radians") || word("rad");
    let degrees = word("degrees") || lower.contains('°');
    match (length, angle, degrees) {
        (true, true, _) => "mm or rad".to_owned(),
        (true, false, _) => "mm".to_owned(),
        (false, true, _) => "rad".to_owned(),
        (false, false, true) => "°".to_owned(),
        _ => String::new(),
    }
}

/// The default a field's text gives: `(default false)`, `default 100`.
fn default_of(text: &str) -> String {
    let Some(at) = text.find("default ") else {
        return String::new();
    };
    let rest = &text[at + "default ".len()..];
    let end = rest
        .find([')', ',', ';', ':'])
        .unwrap_or(rest.len())
        .min(rest.find(". ").unwrap_or(rest.len()));
    let value = rest[..end].trim().trim_end_matches('.');
    if value.split_whitespace().count() > 4 {
        String::new()
    } else {
        value.to_owned()
    }
}

/// A field's type guessed from its text, for fields described in words.
fn guessed_type(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains("default true") || lower.contains("default false") {
        "boolean".to_owned()
    } else if !unit_of(text).is_empty() {
        "value".to_owned()
    } else {
        String::new()
    }
}

/// A row of a fields table.
struct FieldRow {
    names: Vec<String>,
    kind: String,
    unit: String,
    default: String,
    required: String,
    text: String,
}

impl FieldRow {
    /// A field described in words: its names, then its text, of which
    /// the table keeps the first sentence.
    fn described(names: Vec<String>, text: String) -> Self {
        let lower = text.to_lowercase();
        let optional = lower.contains("optional") || lower.contains("default");
        Self {
            names,
            kind: guessed_type(&text),
            unit: unit_of(&text),
            default: default_of(&text),
            required: if optional { "no" } else { "" }.to_owned(),
            text: first_sentence(&text),
        }
    }
}

/// The fields table of a page.
fn fields_table(rows: Vec<FieldRow>) -> Block {
    Block::Table {
        header: [
            "Field",
            "Type",
            "Unit",
            "Default",
            "Required",
            "Description",
        ]
        .iter()
        .map(|cell| (*cell).to_owned())
        .collect(),
        rows: rows
            .into_iter()
            .map(|row| {
                let names: Vec<String> = row.names.iter().map(|name| format!("`{name}`")).collect();
                let default = if row.default.is_empty() || row.default.contains('`') {
                    row.default
                } else {
                    format!("`{}`", row.default)
                };
                vec![
                    names.join(", "),
                    row.kind,
                    row.unit,
                    default,
                    row.required,
                    row.text,
                ]
            })
            .collect(),
    }
}

/// The names a text starts with in code spans, and the text after them:
/// `` `a`, `b` (optional): two points `` gives `a`, `b` and `(optional):
/// two points`. None when it does not start with a name.
fn leading_names(text: &str) -> Option<(Vec<String>, String)> {
    let mut names = Vec::new();
    let mut rest = text.trim_start();
    while let Some(after) = rest.strip_prefix('`') {
        let end = after.find('`')?;
        let name = &after[..end];
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            break;
        }
        names.push(name.to_owned());
        rest = after[end + 1..].trim_start();
        let Some(next) = rest
            .strip_prefix(',')
            .or_else(|| rest.strip_prefix("and "))
            .or_else(|| rest.strip_prefix("or "))
        else {
            break;
        };
        let next = next.trim_start();
        if !next.starts_with('`') {
            break;
        }
        rest = next;
    }
    if names.is_empty() {
        return None;
    }
    let rest = rest.trim_start_matches(':').trim();
    Some((names, rest.to_owned()))
}

/// The fields of a reference table's cell (`` `a`, `b` (optional)
/// selections ``): one row per field, a part that does not start with a
/// field's name goes with the field before.
fn cell_fields(cell: &str) -> Vec<FieldRow> {
    let mut fields: Vec<(Vec<String>, String)> = Vec::new();
    for (part, _) in clean::parts(cell) {
        let part = part.trim();
        let starts = part.starts_with('`')
            && part[1..].find('`').is_some_and(|end| {
                let after = part[end + 2..].trim_start();
                after.is_empty()
                    || after.starts_with('(')
                    || after.starts_with(':')
                    || after.starts_with('{')
                    || after.starts_with("with ")
            });
        let named = if starts || fields.is_empty() {
            leading_names(part)
        } else {
            None
        };
        if let Some(field) = named {
            fields.push(field);
        } else if let Some((_, text)) = fields.last_mut() {
            if !text.is_empty() {
                text.push_str(", ");
            }
            text.push_str(part);
        } else {
            return Vec::new();
        }
    }
    fields
        .into_iter()
        .map(|(names, text)| FieldRow::described(names, text))
        .collect()
}

/// A JSON example pretty, when it is JSON.
fn pretty(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// The examples of a query or a command found in the reference that are
/// JSON: whole code blocks, else their lines that are its objects.
fn json_examples(found: &[String], key: &str, name: &str) -> Vec<String> {
    let names = |value: &Value| -> bool {
        match value {
            Value::Object(object) => object.get(key).and_then(Value::as_str) == Some(name),
            Value::Array(entries) => entries
                .iter()
                .any(|entry| entry.get(key).and_then(Value::as_str) == Some(name)),
            _ => false,
        }
    };
    // The objects of the item first, then the arrays it is in.
    let mut objects: Vec<String> = Vec::new();
    let mut arrays: Vec<String> = Vec::new();
    for text in found {
        let mut add = |value: Value| {
            let list = if value.is_array() {
                &mut arrays
            } else {
                &mut objects
            };
            if let Ok(text) = serde_json::to_string_pretty(&value)
                && !list.contains(&text)
            {
                list.push(text);
            }
        };
        if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
            if names(&value) {
                add(value);
            }
            continue;
        }
        for line in text.lines() {
            let line = line.trim().trim_end_matches(',');
            if let Ok(value) = serde_json::from_str::<Value>(line)
                && names(&value)
            {
                add(value);
            }
        }
    }
    objects.extend(arrays);
    objects
}

/// The sentences of texts that say when it is refused or fails, once each.
fn refusals(texts: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for text in texts {
        for sentence in clean::sentences(text) {
            let lower = sentence.to_lowercase();
            let refusal = lower.contains("refus")
                || lower.contains(" fails")
                || lower.contains("is an error")
                || lower.contains("is not allowed");
            if refusal && !out.iter().any(|known| known == sentence) {
                out.push(sentence.to_owned());
            }
        }
    }
    out.truncate(12);
    out
}

/// Headings of a text made at least `level` deep; the first block goes when
/// it is a heading (the section's own, which the page's title replaces).
fn nested(mut blocks: Vec<Block>, level: usize) -> Vec<Block> {
    if matches!(blocks.first(), Some(Block::Heading { .. })) {
        blocks.remove(0);
    }
    for block in &mut blocks {
        if let Block::Heading { level: own, .. } = block {
            *own = (*own).max(level).min(6);
        }
    }
    blocks
}

/// The generated reference for the names of a vocabulary (those
/// `model_reference` lists) and the version of the sources.
pub fn generate(vocabulary: &Vocabulary, version: &str) -> ApiReference {
    let sections = reference::section_sources(vocabulary);
    let mut links = Links::default();
    let mut omissions = Vec::new();
    let mut gaps = Vec::new();

    // The topics: a page per section of the reference's second level, its
    // subsections as its headings. First where each heading goes, so that
    // the links of any section's text find it.
    let mut placed: Vec<Option<(String, String)>> = Vec::new();
    {
        let empty = Links::default();
        let mut cleaner = Cleaner {
            links: &empty,
            page: "topics".to_owned(),
            omissions: Vec::new(),
        };
        let mut slugs = Ids::default();
        let mut anchors = Ids::default();
        // The path and the headings' ids of the topic being placed.
        let mut topic: Option<String> = None;
        let mut ids = Ids::default();
        for section in &sections {
            let anchor = anchors.next(&section.full.replace('`', ""));
            let title = cleaner.text(&section.title);
            let title = if title.is_empty() {
                section.title.replace('`', "")
            } else {
                title
            };
            let target = if section.level == 2 {
                let path = format!("topics/{}", slugs.next(&title.replace('`', "")));
                ids = Ids::default();
                // The page's title is its first heading.
                ids.next(&title.replace('`', ""));
                topic = Some(path.clone());
                format!("page:{path}")
            } else if let Some(path) = &topic {
                format!("page:{path}#{}", ids.next(&title.replace('`', "")))
            } else {
                placed.push(None);
                continue;
            };
            links.anchors.insert(anchor, target.clone());
            links.paths.insert(section.path.clone(), target);
            placed.push(topic.clone().map(|path| (path, title)));
        }
        omissions.append(&mut cleaner.omissions);
    }
    let mut topics: Vec<Page> = Vec::new();
    for (section, placed) in sections.iter().zip(&placed) {
        let Some((path, title)) = placed else {
            continue;
        };
        if section.level == 2 {
            topics.push(Page {
                path: path.clone(),
                title: title.clone(),
                kind: PageKind::Topic,
                summary: String::new(),
                group: String::new(),
                blocks: vec![Block::Heading {
                    level: 1,
                    text: title.clone(),
                }],
            });
        }
        let Some(page) = topics.last_mut() else {
            continue;
        };
        if section.level > 2 {
            page.blocks.push(Block::Heading {
                level: section.level - 1,
                text: title.clone(),
            });
        }
        let mut cleaner = Cleaner {
            links: &links,
            page: path.clone(),
            omissions: Vec::new(),
        };
        // A section's own text has no headings of its own.
        page.blocks.extend(
            cleaner
                .blocks(&section.own)
                .into_iter()
                .filter(|block| !matches!(block, Block::Heading { .. })),
        );
        omissions.append(&mut cleaner.omissions);
    }
    for topic in &mut topics {
        topic.summary = topic
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Paragraph(text) => Some(first_sentence(text)),
                _ => None,
            })
            .unwrap_or_default();
    }

    let mut items: Vec<Page> = Vec::new();
    for (kind, names) in [
        (PageKind::Command, &vocabulary.commands),
        (PageKind::Query, &vocabulary.queries),
    ] {
        for name in names {
            let (page, mut lost, gap) = item_page(vocabulary, &links, kind, name);
            omissions.append(&mut lost);
            gaps.extend(gap);
            items.push(page);
        }
    }
    for name in &vocabulary.features {
        let (page, mut lost, gap) = feature_page(vocabulary, &links, name);
        omissions.append(&mut lost);
        gaps.extend(gap);
        items.push(page);
    }

    let overview = overview_page(&sections, &links, &mut omissions);
    let mut pages = vec![index_page(&items, &topics, version), overview];
    pages.extend(items);
    pages.extend(topics);
    ApiReference {
        version: version.to_owned(),
        pages,
        omissions,
        gaps,
    }
}

/// A command's or a query's page, what its cleaning left out and its gaps.
fn item_page(
    vocabulary: &Vocabulary,
    links: &Links,
    kind: PageKind,
    name: &str,
) -> (Page, Vec<Omission>, Vec<String>) {
    let word = kind.word();
    let path = item_path(kind, name);
    let mut cleaner = Cleaner {
        links,
        page: path.clone(),
        omissions: Vec::new(),
    };
    let mut gaps = Vec::new();
    let source_kind = if kind == PageKind::Command {
        "command"
    } else {
        "query"
    };
    let source = reference::item_source(vocabulary, source_kind, name);
    let spec = (kind == PageKind::Command)
        .then(|| crate::schemas::spec(name))
        .flatten();
    let mut blocks = vec![Block::Heading {
        level: 1,
        text: format!("`{name}`"),
    }];

    // The rows' cells by their headers: `Fields`, `Result`, ….
    let rows: Vec<Vec<(String, String)>> = source
        .as_ref()
        .map(|source| {
            source
                .rows
                .iter()
                .map(|(header, row)| {
                    let header = markdown::cells(header);
                    let row = markdown::cells(row);
                    header
                        .into_iter()
                        .zip(row)
                        .skip(1)
                        .map(|(header, cell)| (cleaner.text(&header), cleaner.text(&cell)))
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default();
    let prose = source
        .as_ref()
        .and_then(|source| source.prose.as_deref())
        .map(|text| cleaner.blocks(text))
        .map(|blocks| nested(blocks, 3))
        .unwrap_or_default();
    let nearby = source
        .as_ref()
        .map(|source| nested(cleaner.blocks(&source.nearby), 3))
        .unwrap_or_default();
    let block_texts = |blocks: &[Block]| -> Vec<String> {
        blocks
            .iter()
            .flat_map(|block| match block {
                Block::Paragraph(text) => vec![text.clone()],
                Block::List(items) => items.iter().map(|item| item.text.clone()).collect(),
                _ => Vec::new(),
            })
            .collect()
    };
    let texts: Vec<String> = block_texts(&prose)
        .into_iter()
        .chain(block_texts(&nearby))
        .collect();

    // Its purpose: a command's schema's description or a query's sentence
    // in the query catalog (mitcad#2636), else the first sentence of the
    // reference that names it.
    let description = if kind == PageKind::Query {
        let purpose = crate::query_purposes::query_purpose(name);
        if purpose.is_none() {
            gaps.push(format!(
                "{word} {name}: no purpose in core/mcp/src/query_purposes.rs"
            ));
        }
        purpose.map(|text| cleaner.text(text)).unwrap_or_default()
    } else {
        spec.as_ref()
            .and_then(|spec| spec["description"].as_str())
            .map(|text| cleaner.text(text))
            .unwrap_or_default()
    };
    let (purpose, schema_result) = match description.split_once(" Result: ") {
        Some((purpose, result)) => (purpose.to_owned(), Some(result.to_owned())),
        None => (description.clone(), None),
    };
    let quoted = format!("`{name}`");
    let section = source
        .as_ref()
        .and_then(|source| source.path.clone())
        .unwrap_or_default();
    let section_link = if section.is_empty() {
        String::new()
    } else {
        cleaner.path_link(&section)
    };
    let purpose = if !purpose.is_empty() {
        purpose
    } else if let Some(sentence) = texts
        .iter()
        .flat_map(|text| clean::sentences(text))
        .find(|sentence| sentence.contains(&quoted))
    {
        sentence.to_owned()
    } else if !section.is_empty() {
        format!(
            "A {word} of the section {}.",
            cleaner.text(section.rsplit(" > ").next().unwrap_or(&section))
        )
    } else {
        gaps.push(format!("{word} {name}: the reference does not describe it"));
        format!("A {word} of the model API.")
    };
    blocks.push(Block::Paragraph(purpose.clone()));
    let mut about = format!("A {word}");
    if !section_link.is_empty() {
        about.push_str(&format!(" of {section_link}"));
    }
    about.push('.');
    if kind == PageKind::Command {
        about.push_str(&format!(" Run as `{{\"cmd\": \"{name}\", …}}`."));
    } else {
        about.push_str(&format!(" Asked as `{{\"query\": \"{name}\", …}}`."));
    }
    blocks.push(Block::Paragraph(about));

    // Its fields.
    let mut fields_cell: Option<String> = None;
    for row in &rows {
        if let Some((_, cell)) = row
            .iter()
            .find(|(header, _)| header.to_lowercase().contains("field"))
        {
            fields_cell = Some(cell.clone());
            break;
        }
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Fields".to_owned(),
    });
    if let Some(spec) = &spec {
        let required: BTreeSet<&str> = spec["required"]
            .as_array()
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut properties: Vec<(&String, &Value)> = spec["properties"]
            .as_object()
            .map(|properties| properties.iter().collect())
            .unwrap_or_default();
        properties.sort_by_key(|(field, _)| !required.contains(field.as_str()));
        let mut rows = Vec::new();
        for (field, schema) in properties {
            let feature = schema.as_object().is_some_and(serde_json::Map::is_empty);
            let text = if feature {
                "A feature definition: its `type` and that type's fields; see the [feature types](page:index#feature-types).".to_owned()
            } else {
                cleaner.text(schema["description"].as_str().unwrap_or_default())
            };
            rows.push(FieldRow {
                names: vec![field.clone()],
                kind: type_of(schema),
                unit: unit_of(&text),
                default: schema.get("default").map(plain).unwrap_or_default(),
                required: if required.contains(field.as_str()) {
                    "yes"
                } else {
                    "no"
                }
                .to_owned(),
                text,
            });
        }
        if rows.is_empty() {
            blocks.push(Block::Paragraph("No fields besides `cmd`.".to_owned()));
        } else {
            blocks.push(fields_table(rows));
        }
        if let Some(restrictions) = spec["restrictions"].as_array() {
            let items: Vec<Item> = restrictions
                .iter()
                .filter_map(Value::as_str)
                .map(|text| cleaner.text(text))
                .filter(|text| !text.is_empty())
                .map(|text| Item {
                    depth: 0,
                    ordered: false,
                    text,
                })
                .collect();
            if !items.is_empty() {
                blocks.push(Block::List(items));
            }
        }
    } else {
        match &fields_cell {
            Some(cell) if cell.is_empty() => {
                blocks.push(Block::Paragraph("No fields.".to_owned()));
            }
            Some(cell) => {
                let fields = cell_fields(cell);
                if fields.is_empty() {
                    blocks.push(Block::Paragraph(cell.clone()));
                } else {
                    blocks.push(fields_table(fields));
                    blocks.push(Block::Paragraph(
                        "The fields as the reference lists them; a blank Required means it does not say."
                            .to_owned(),
                    ));
                }
            }
            None => {
                blocks.push(Block::Paragraph(
                    "The reference's text below describes its fields.".to_owned(),
                ));
            }
        }
    }

    // An example.
    let key = if kind == PageKind::Command {
        "cmd"
    } else {
        "query"
    };
    let mut examples: Vec<String> = spec
        .as_ref()
        .and_then(|spec| spec["examples"].as_array())
        .map(|examples| {
            examples
                .iter()
                .filter_map(|example| serde_json::to_string_pretty(example).ok())
                .collect()
        })
        .unwrap_or_default();
    if examples.is_empty() {
        let found = reference::item_examples(vocabulary, source_kind, name);
        examples = json_examples(&found, key, name);
    }
    // Without fields, the name alone is the whole request.
    if examples.is_empty() && fields_cell.as_deref() == Some("") {
        let mut bare = serde_json::Map::new();
        bare.insert(key.to_owned(), json!(name));
        examples.push(Value::Object(bare).to_string());
    }
    examples.truncate(3);
    blocks.push(Block::Heading {
        level: 2,
        text: "Example".to_owned(),
    });
    if examples.is_empty() {
        gaps.push(format!("{word} {name}: no example"));
        blocks.push(Block::Paragraph(
            "The reference has no example of it.".to_owned(),
        ));
    }
    for example in examples {
        let text = pretty(&example).unwrap_or(example);
        if clean::names_ticket(&text) {
            continue;
        }
        blocks.push(Block::Code {
            lang: "json".to_owned(),
            text,
        });
    }

    // Its result.
    let results: Vec<(String, String)> = rows
        .iter()
        .flat_map(|row| {
            row.iter()
                .filter(|(header, cell)| {
                    !header.to_lowercase().contains("field") && !cell.is_empty()
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    if !results.is_empty() || schema_result.is_some() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Result".to_owned(),
        });
        if let Some(result) = &schema_result {
            blocks.push(Block::Paragraph(result.clone()));
        }
        for (header, cell) in results {
            // Not the schema's result again.
            if schema_result.as_deref() == Some(cell.as_str()) {
                continue;
            }
            let label = if header.is_empty() || header == "Result" {
                String::new()
            } else {
                format!("{header}: ")
            };
            blocks.push(Block::Paragraph(format!("{label}{cell}")));
        }
    }

    // Its refusals.
    let mut said: Vec<&str> = texts.iter().map(String::as_str).collect();
    let cells: Vec<String> = rows
        .iter()
        .flatten()
        .map(|(_, cell)| cell.clone())
        .collect();
    said.extend(cells.iter().map(String::as_str));
    said.push(&description);
    let refused = refusals(&said);
    if !refused.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Refusals and errors".to_owned(),
        });
        blocks.push(Block::List(
            refused
                .into_iter()
                .map(|text| Item {
                    depth: 0,
                    ordered: false,
                    text,
                })
                .collect(),
        ));
    }

    // The reference's own text of it.
    let details: Vec<Block> = if prose.is_empty() { nearby } else { prose };
    if !details.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "In the reference".to_owned(),
        });
        blocks.extend(details);
    }
    if let Some(source) = &source
        && !source.also_in.is_empty()
    {
        let items: Vec<Item> = source
            .also_in
            .iter()
            .map(|path| Item {
                depth: 0,
                ordered: false,
                text: cleaner.path_link(path),
            })
            .filter(|item| !item.text.is_empty())
            .collect();
        if !items.is_empty() {
            blocks.push(Block::Heading {
                level: 2,
                text: "Also described in".to_owned(),
            });
            blocks.push(Block::List(items));
        }
    }
    if source.is_none() && spec.is_none() {
        gaps.push(format!("{word} {name}: no fields"));
    }
    let group = if section.is_empty() {
        "Other".to_owned()
    } else {
        cleaner.text(&section.replace(" > ", " › "))
    };
    let page = Page {
        path,
        title: format!("`{name}`"),
        kind,
        summary: first_sentence(&purpose),
        group,
        blocks,
    };
    (page, cleaner.omissions, gaps)
}

/// A feature type's page, what its cleaning left out and its gaps.
fn feature_page(
    vocabulary: &Vocabulary,
    links: &Links,
    name: &str,
) -> (Page, Vec<Omission>, Vec<String>) {
    let path = item_path(PageKind::Feature, name);
    let mut cleaner = Cleaner {
        links,
        page: path.clone(),
        omissions: Vec::new(),
    };
    let mut gaps = Vec::new();
    let mut blocks = vec![Block::Heading {
        level: 1,
        text: format!("`{name}`"),
    }];
    let source = reference::feature_source(vocabulary, name);
    let section = source.as_ref().map(|source| source.path.clone());
    let all = source
        .as_ref()
        .map(|source| nested(cleaner.blocks(&source.text), 3))
        .unwrap_or_default();
    // The section's own text: up to its first subsection.
    let own: Vec<&Block> = all
        .iter()
        .take_while(|block| !matches!(block, Block::Heading { .. }))
        .collect();
    let first = own.iter().find_map(|block| match block {
        Block::Paragraph(text) => Some(text.clone()),
        _ => None,
    });
    let purpose = first
        .as_deref()
        .map(first_sentence)
        .unwrap_or_else(|| format!("The feature type `{name}`."));
    blocks.push(Block::Paragraph(purpose.clone()));
    let mut about = format!(
        "A feature definition of type `{name}` is the `def` of [add_feature](page:commands/add_feature) and [edit_feature](page:commands/edit_feature), and what the `feature` query gives"
    );
    if let Some(section) = &section {
        about.push_str(&format!("; its section: {}", cleaner.path_link(section)));
    }
    about.push('.');
    blocks.push(Block::Paragraph(about));

    blocks.push(Block::Heading {
        level: 2,
        text: "Fields".to_owned(),
    });
    let fields: Vec<FieldRow> = own
        .iter()
        .flat_map(|block| match block {
            Block::List(items) => items
                .iter()
                .filter(|item| item.depth == 0)
                .filter_map(|item| leading_names(&item.text))
                .map(|(names, text)| FieldRow::described(names, text))
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    if fields.is_empty() {
        blocks.push(Block::Paragraph(
            "The section below describes its fields.".to_owned(),
        ));
    } else {
        blocks.push(fields_table(fields));
        blocks.push(Block::Paragraph(
            "Lengths are in mm and angles in radians; every length, angle, count and factor is a value (a number or an expression). A blank Required means the reference does not say; `type` is always required.".to_owned(),
        ));
    }

    blocks.push(Block::Heading {
        level: 2,
        text: "Example".to_owned(),
    });
    let example = source.as_ref().and_then(|source| source.example.clone());
    match example
        .as_deref()
        .map(|text| serde_json::from_str::<Value>(text.trim()))
    {
        Some(Ok(def)) => {
            let command = json!({"cmd": "add_feature", "def": def});
            blocks.push(Block::Code {
                lang: "json".to_owned(),
                text: serde_json::to_string_pretty(&command).unwrap_or_default(),
            });
        }
        Some(Err(_)) => {
            let text = example.unwrap_or_default();
            if !clean::names_ticket(&text) {
                blocks.push(Block::Code {
                    lang: "json".to_owned(),
                    text,
                });
            }
        }
        None => {
            gaps.push(format!("feature type {name}: no example"));
            blocks.push(Block::Paragraph(
                "The reference has no example of it.".to_owned(),
            ));
        }
    }

    if source.is_none() {
        gaps.push(format!(
            "feature type {name}: the reference does not describe it"
        ));
    }
    if !all.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "In the reference".to_owned(),
        });
        blocks.extend(all);
    }
    let group = section
        .as_deref()
        .map(|section| cleaner.text(&section.replace(" > ", " › ")))
        .unwrap_or_else(|| "Other".to_owned());
    let page = Page {
        path,
        title: format!("`{name}`"),
        kind: PageKind::Feature,
        summary: purpose,
        group,
        blocks,
    };
    (page, cleaner.omissions, gaps)
}

/// The overview: what each of [`OVERVIEW`]'s sections says before its
/// subsections, with a link to its whole text.
fn overview_page(
    sections: &[reference::SectionSource],
    links: &Links,
    omissions: &mut Vec<Omission>,
) -> Page {
    let mut cleaner = Cleaner {
        links,
        page: "overview".to_owned(),
        omissions: Vec::new(),
    };
    let mut blocks = vec![
        Block::Heading {
            level: 1,
            text: "Overview".to_owned(),
        },
        Block::Paragraph(
            "MitCad's model API is JSON: a script, `mitcad-cli` or an MCP client changes a design with commands (`{\"cmd\": \"<name>\", …}`) and reads it with queries (`{\"query\": \"<name>\", …}`). This page tells how commands, queries, previews and undo work, how values, units and expressions are written, how features, bodies, faces and edges are named, and how errors are given. Each command, query and feature type has a page of its own; the topics hold the whole text of each part of the reference.".to_owned(),
        ),
    ];
    for title in OVERVIEW {
        let Some(section) = sections.iter().find(|section| section.title == *title) else {
            continue;
        };
        let own = cleaner.blocks(&section.own);
        if own.is_empty() {
            continue;
        }
        blocks.push(Block::Heading {
            level: 2,
            text: (*title).to_owned(),
        });
        blocks.extend(nested(own, 3));
        let more = cleaner.path_link(&section.path);
        blocks.push(Block::Paragraph(format!("The whole section: {more}.")));
    }
    omissions.append(&mut cleaner.omissions);
    Page {
        path: "overview".to_owned(),
        title: "Overview".to_owned(),
        kind: PageKind::Overview,
        summary: "How commands, queries, preview and undo work; values and units; names of features, bodies, faces and edges; errors.".to_owned(),
        group: String::new(),
        blocks,
    }
}

/// The index: the overview, the commands and queries by their sections,
/// the feature types and the topics.
fn index_page(items: &[Page], topics: &[Page], version: &str) -> Page {
    let link = |page: &Page| -> String {
        if page.summary.is_empty() {
            format!("[{}](page:{})", page.title, page.path)
        } else {
            format!("[{}](page:{}): {}", page.title, page.path, page.summary)
        }
    };
    let mut blocks = vec![
        Block::Heading {
            level: 1,
            text: "MitCad API reference".to_owned(),
        },
        Block::Paragraph(format!(
            "The model API of MitCad {version}: the commands and queries that scripts, `mitcad-cli` and MCP clients send as JSON, and the feature types they define. Start with the [overview](page:overview)."
        )),
    ];
    for (kind, title) in [
        (PageKind::Command, "Commands"),
        (PageKind::Query, "Queries"),
        (PageKind::Feature, "Feature types"),
    ] {
        blocks.push(Block::Heading {
            level: 2,
            text: title.to_owned(),
        });
        // By group, in the order the groups first come.
        let mut groups: Vec<(&str, Vec<&Page>)> = Vec::new();
        for page in items.iter().filter(|page| page.kind == kind) {
            match groups.iter_mut().find(|(group, _)| *group == page.group) {
                Some((_, pages)) => pages.push(page),
                None => groups.push((&page.group, vec![page])),
            }
        }
        if kind == PageKind::Feature {
            let mut all: Vec<&Page> = groups.into_iter().flat_map(|(_, pages)| pages).collect();
            all.sort_by(|a, b| a.title.cmp(&b.title));
            blocks.push(Block::List(
                all.into_iter()
                    .map(|page| Item {
                        depth: 0,
                        ordered: false,
                        text: link(page),
                    })
                    .collect(),
            ));
            continue;
        }
        for (group, pages) in groups {
            blocks.push(Block::Heading {
                level: 3,
                text: group.to_owned(),
            });
            blocks.push(Block::List(
                pages
                    .into_iter()
                    .map(|page| Item {
                        depth: 0,
                        ordered: false,
                        text: link(page),
                    })
                    .collect(),
            ));
        }
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Topics".to_owned(),
    });
    blocks.push(Block::List(
        topics
            .iter()
            .map(|page| Item {
                depth: 0,
                ordered: false,
                text: format!("[{}](page:{})", page.title, page.path),
            })
            .collect(),
    ));
    Page {
        path: "index".to_owned(),
        title: "MitCad API reference".to_owned(),
        kind: PageKind::Index,
        summary: String::new(),
        group: String::new(),
        blocks,
    }
}

/// The search's box and list, between the index's introduction and its
/// lists, which the search hides while it shows results.
fn with_search(html: &str) -> String {
    let Some(first) = html.find("<h2 ") else {
        return html.to_owned();
    };
    let end = html.rfind("</main>").unwrap_or(html.len());
    format!(
        "{}<p><input id=\"search\" type=\"search\" placeholder=\"Search commands, queries, feature types and topics\" aria-label=\"Search\"></p>\n<ul id=\"results\"></ul>\n<div id=\"lists\">\n{}</div>\n{}",
        &html[..first],
        &html[first..end],
        &html[end..]
    )
}

/// What a page's text has that the reference must not: a ticket
/// reference, history, a file's or a test's name or a note on the code
/// inside, each as `<what>: <line>`.
pub fn problems(markdown: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut fenced = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        let mut problem = |what: &str| found.push(format!("{what}: {line}"));
        if fenced {
            if clean::names_ticket(line) {
                problem("a ticket reference");
            }
            continue;
        }
        if clean::has_ticket(line)
            || clean::spans(line)
                .iter()
                .any(|(code, span)| *code && clean::names_ticket(span))
        {
            problem("a ticket reference");
        }
        let prose = clean::prose(line);
        if clean::history(&prose).is_some() {
            problem("history");
        }
        if clean::internal_note(&prose).is_some() {
            problem("a note on the code inside");
        }
        if clean::spans(line)
            .iter()
            .any(|(code, span)| *code && clean::internal_name(span))
        {
            problem("a file's or a test's name");
        }
    }
    found
}

/// The files a folder holds, at any depth, as paths from it joined with
/// `/` (what [`ApiReference::foreign_files`] takes); none for a folder
/// that is not there.
pub fn files_in(folder: &Path) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    if !folder.exists() {
        return Ok(found);
    }
    let mut pending = vec![folder.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path.strip_prefix(folder).unwrap_or(&path);
            let parts: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            found.push(parts.join("/"));
        }
    }
    found.sort();
    Ok(found)
}

impl ApiReference {
    /// The files of the reference: each page as `.md` and `.html`, the
    /// style sheet and the search, by their paths from its folder.
    pub fn files(&self) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let mut index = Vec::new();
        for page in &self.pages {
            files.push((format!("{}.md", page.path), page.markdown()));
            let html = page.html(&self.version);
            let html = if page.kind == PageKind::Index {
                with_search(&html)
            } else {
                html
            };
            files.push((format!("{}.html", page.path), html));
            if page.kind != PageKind::Index {
                index.push(json!({
                    "t": page.title.replace('`', ""),
                    "k": page.kind.word(),
                    "u": format!("{}.html", page.path),
                    "s": page.summary.replace('`', ""),
                }));
            }
        }
        files.push((
            "search-index.js".to_owned(),
            format!("var MITCAD_API_INDEX = {};\n", Value::Array(index)),
        ));
        files.push(("search.js".to_owned(), SEARCH.to_owned()));
        files.push(("style.css".to_owned(), STYLE.to_owned()));
        files
    }

    /// The paths of [`Self::files`], without making their texts.
    pub fn file_paths(&self) -> BTreeSet<String> {
        let mut paths: BTreeSet<String> = self
            .pages
            .iter()
            .flat_map(|page| [format!("{}.md", page.path), format!("{}.html", page.path)])
            .collect();
        for name in ["search-index.js", "search.js", "style.css"] {
            paths.insert(name.to_owned());
        }
        paths
    }

    /// Of the files a folder holds (`present`, paths from the folder with
    /// `/`), those that are not files of this reference, sorted: an earlier
    /// run's pages that are no longer pages, or anything else put there.
    /// The folder is published as it is (mitcad#2636), so the example
    /// refuses to write into a folder that holds any.
    pub fn foreign_files(&self, present: &[String]) -> Vec<String> {
        let own = self.file_paths();
        let mut foreign: Vec<String> = present
            .iter()
            .filter(|path| !own.contains(path.as_str()))
            .cloned()
            .collect();
        foreign.sort();
        foreign
    }

    /// Writes the files into a folder (made when missing), each one
    /// replaced; files of an earlier run that are no longer pages stay
    /// ([`Self::foreign_files`] finds them first).
    pub fn write_to(&self, folder: &Path) -> io::Result<()> {
        for (path, text) in self.files() {
            let file = folder.join(&path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, text)?;
        }
        Ok(())
    }

    /// The pages of a kind.
    pub fn count(&self, kind: PageKind) -> usize {
        self.pages.iter().filter(|page| page.kind == kind).count()
    }

    /// The report for the maintainers: the pages, what the cleaning left
    /// out (the sentences to fix in `commands.md` or to mark internal
    /// there) and the names without an example or a description.
    pub fn report_text(&self) -> String {
        let mut out = format!(
            "MitCad API reference {}: {} pages ({} commands, {} queries, {} feature types, {} topics)\n",
            self.version,
            self.pages.len(),
            self.count(PageKind::Command),
            self.count(PageKind::Query),
            self.count(PageKind::Feature),
            self.count(PageKind::Topic),
        );
        let reasons = [
            clean::UNCLEAN,
            clean::HISTORY,
            clean::TESTS,
            clean::INTERNAL,
        ];
        for reason in reasons {
            let count = self
                .omissions
                .iter()
                .filter(|omission| omission.reason == reason)
                .count();
            out.push_str(&format!("left out, {reason}: {count} sentences\n"));
        }
        out.push_str(&format!("gaps: {}\n", self.gaps.len()));
        for reason in reasons {
            out.push_str(&format!("\n## Left out: {reason}\n\n"));
            for omission in self
                .omissions
                .iter()
                .filter(|omission| omission.reason == reason)
            {
                out.push_str(&format!("{}\t{}\n", omission.page, omission.text));
            }
        }
        out.push_str("\n## Gaps\n\n");
        for gap in &self.gaps {
            out.push_str(gap);
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_described_in_words_are_read() {
        let fields = cell_fields(
            "`expression`, `kind`: `length` (default), `angle` or `unitless`; `parameter` (optional, a parameter's name)",
        );
        let names: Vec<Vec<String>> = fields.iter().map(|field| field.names.clone()).collect();
        assert_eq!(
            names,
            vec![
                vec!["expression".to_owned()],
                vec!["kind".to_owned()],
                vec!["parameter".to_owned()]
            ]
        );
        assert_eq!(fields[1].text, "`length` (default), `angle` or `unitless`");
        assert_eq!(fields[2].required, "no");
        assert!(cell_fields("the shown bodies").is_empty());
        assert_eq!(
            leading_names("`surface`, `curves`: a surface extrusion"),
            Some((
                vec!["surface".to_owned(), "curves".to_owned()],
                "a surface extrusion".to_owned()
            ))
        );
        assert_eq!(leading_names("a `name`"), None);
    }

    #[test]
    fn units_defaults_and_types_are_told() {
        assert_eq!(unit_of("a length in mm"), "mm");
        assert_eq!(
            unit_of("a number in mm (a length) or radians (an angle)"),
            "mm or rad"
        );
        assert_eq!(unit_of("the radius of the arc"), "");
        assert_eq!(unit_of("min_volume (mm³, default 1e-6)"), "mm³");
        assert_eq!(default_of("`dependents` (default false)"), "false");
        assert_eq!(default_of("`size` (default 100) mm"), "100");
        assert_eq!(default_of("a plain field"), "");
        assert_eq!(type_of(&json!({"type": "string"})), "string");
        assert_eq!(
            type_of(&json!({"type": ["number", "string"]})),
            "number or string"
        );
        assert_eq!(type_of(&json!({"enum": ["a", "b"]})), "one of `a`, `b`");
        assert_eq!(
            type_of(&json!({"type": "array", "items": {"type": "string"}})),
            "array of string"
        );
        assert_eq!(type_of(&json!({})), "feature definition");
    }

    #[test]
    fn examples_are_the_json_that_names_the_item() {
        let found = vec![
            "{\"query\": \"bodies\", \"properties\": true}".to_owned(),
            "{\"query\": \"faces\"}\n{\"query\": \"bodies\"}".to_owned(),
            "not json".to_owned(),
        ];
        let examples = json_examples(&found, "query", "bodies");
        assert_eq!(examples.len(), 2);
        assert!(examples[0].contains("\"properties\": true"));
    }
}
