// SPDX-License-Identifier: MIT
//! The Markdown of the generated API reference (mitcad#2606) as blocks:
//! the part of Markdown the command reference is written in (headings,
//! paragraphs, lists, tables, fenced code), read into [`Block`]s, which
//! are cleaned, and written again as Markdown and as HTML.
//!
//! Links to another page of the generated reference are written
//! `page:<path>` or `page:<path>#<id>` (`page:commands/add_feature`); the
//! Markdown gets `<path>.md`, the HTML `<path>.html`, each relative to the
//! page that links.

use std::collections::BTreeMap;

/// A list item: how deep it is nested, and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) depth: usize,
    pub(crate) ordered: bool,
    pub(crate) text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Heading {
        level: usize,
        text: String,
    },
    Paragraph(String),
    List(Vec<Item>),
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code {
        lang: String,
        text: String,
    },
}

/// Whether a line, without its indentation, starts a list item; the
/// length of its marker (`- `, `1. `) and whether it is numbered.
fn bullet(trimmed: &str) -> Option<(usize, bool)> {
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return Some((2, false));
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && trimmed[digits..].starts_with(". ")).then_some((digits + 2, true))
}

/// A heading line's level and text.
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' '))
        .then(|| (level, line[level..].trim()))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A table line's cells: split at `|` that is not escaped, `\|` read as
/// `|`.
pub(crate) fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell).trim().to_owned()),
            _ => cell.push(c),
        }
    }
    cells.push(cell.trim().to_owned());
    cells
}

/// Whether a table line is the line under its header (`|---|:--:|`).
fn separator(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|')
        && line.contains('-')
        && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// The blocks of a Markdown text. HTML comments are left out.
pub(crate) fn parse(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if trimmed.starts_with("<!--") {
            while i < lines.len() && !lines[i].contains("-->") {
                i += 1;
            }
            i += 1;
            continue;
        }
        if let Some(fence) = trimmed.strip_prefix("```") {
            let lang = fence.trim().to_owned();
            let margin = indent(line);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                let own = lines[i];
                let cut = indent(own).min(margin);
                let cut = if own.is_char_boundary(cut) { cut } else { 0 };
                code.push(&own[cut..]);
                i += 1;
            }
            i += 1;
            blocks.push(Block::Code {
                lang,
                text: code.join("\n"),
            });
            continue;
        }
        if let Some((level, title)) = heading(line) {
            blocks.push(Block::Heading {
                level,
                text: title.to_owned(),
            });
            i += 1;
            continue;
        }
        if trimmed.starts_with('|') {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                rows.push(lines[i]);
                i += 1;
            }
            let header = cells(rows[0]);
            let body = rows
                .iter()
                .skip(1)
                .filter(|row| !separator(row))
                .map(|row| cells(row))
                .collect();
            blocks.push(Block::Table { header, rows: body });
            continue;
        }
        if bullet(trimmed).is_some() {
            let mut items: Vec<Item> = Vec::new();
            // The indentations of the items' markers, outermost first.
            let mut margins: Vec<usize> = Vec::new();
            while i < lines.len() {
                let line = lines[i];
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    // A list goes on after an empty line with an indented
                    // line or another item.
                    let next = lines[i + 1..].iter().find(|l| !l.trim().is_empty());
                    match next {
                        Some(next)
                            if (bullet(next.trim()).is_some() || indent(next) >= 2)
                                && !next.trim_start().starts_with("```")
                                && heading(next).is_none() =>
                        {
                            i += 1;
                            continue;
                        }
                        _ => break,
                    }
                }
                if trimmed.starts_with("```")
                    || trimmed.starts_with('|')
                    || heading(line).is_some()
                    || trimmed.starts_with("<!--")
                {
                    break;
                }
                if let Some((marker, ordered)) = bullet(trimmed) {
                    let at = indent(line);
                    while margins.last().is_some_and(|&margin| margin > at) {
                        margins.pop();
                    }
                    if margins.last() != Some(&at) {
                        margins.push(at);
                    }
                    items.push(Item {
                        depth: margins.len() - 1,
                        ordered,
                        text: trimmed[marker..].trim().to_owned(),
                    });
                } else if indent(line) == 0 && items.is_empty() {
                    break;
                } else if let Some(item) = items.last_mut() {
                    // A line of the item before; a line at the margin ends
                    // the list unless it follows the item directly.
                    if indent(line) == 0 && lines[i - 1].trim().is_empty() {
                        break;
                    }
                    item.text.push(' ');
                    item.text.push_str(trimmed);
                }
                i += 1;
            }
            blocks.push(Block::List(items));
            continue;
        }
        let mut paragraph = vec![trimmed];
        i += 1;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("```")
                || trimmed.starts_with('|')
                || trimmed.starts_with("<!--")
                || heading(line).is_some()
                || (bullet(trimmed).is_some() && indent(line) < 2)
            {
                break;
            }
            paragraph.push(trimmed);
            i += 1;
        }
        blocks.push(Block::Paragraph(paragraph.join(" ")));
    }
    blocks
}

/// A table cell as Markdown writes it.
fn cell_markdown(cell: &str) -> String {
    cell.replace('|', "\\|")
}

/// The blocks as Markdown, `page:` links made `.md` links relative to
/// `root` (`""` or `"../"`).
pub(crate) fn to_markdown(blocks: &[Block], root: &str) -> String {
    let mut out = String::new();
    for block in blocks {
        if !out.is_empty() {
            out.push('\n');
        }
        match block {
            Block::Heading { level, text } => {
                out.push_str(&format!(
                    "{} {}\n",
                    "#".repeat(*level),
                    links(text, root, "md")
                ));
            }
            Block::Paragraph(text) => {
                out.push_str(&links(text, root, "md"));
                out.push('\n');
            }
            Block::List(items) => {
                for item in items {
                    let marker = if item.ordered { "1." } else { "-" };
                    out.push_str(&format!(
                        "{}{marker} {}\n",
                        "  ".repeat(item.depth),
                        links(&item.text, root, "md")
                    ));
                }
            }
            Block::Table { header, rows } => {
                let line = |cells: &[String]| {
                    let cells: Vec<String> = cells
                        .iter()
                        .map(|cell| cell_markdown(&links(cell, root, "md")))
                        .collect();
                    format!("| {} |\n", cells.join(" | "))
                };
                out.push_str(&line(header));
                out.push_str(&format!("|{}\n", "---|".repeat(header.len())));
                for row in rows {
                    out.push_str(&line(row));
                }
            }
            Block::Code { lang, text } => {
                out.push_str(&format!("```{lang}\n{text}\n```\n"));
            }
        }
    }
    out
}

/// A `page:` link's target as a file of the format (`md`, `html`)
/// relative to `root`; other targets as they are.
fn target(url: &str, root: &str, extension: &str) -> String {
    match url.strip_prefix("page:") {
        Some(page) => {
            let (path, fragment) = page.split_once('#').unwrap_or((page, ""));
            let mut target = format!("{root}{path}.{extension}");
            if !fragment.is_empty() {
                target.push('#');
                target.push_str(fragment);
            }
            target
        }
        None => url.to_owned(),
    }
}

/// A link at `at` in `text` (`[label](url)`, outside code): its label,
/// its url and the byte after it.
pub(crate) fn link_at(text: &str, at: usize) -> Option<(&str, &str, usize)> {
    let rest = &text[at..];
    if !rest.starts_with('[') {
        return None;
    }
    // The label's end: the `]` that closes it, outside code.
    let mut depth = 0;
    let mut code = false;
    let mut close = None;
    for (i, c) in rest.char_indices() {
        match c {
            '`' => code = !code,
            '[' if !code => depth += 1,
            ']' if !code => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let after = &rest[close + 1..];
    let url = after.strip_prefix('(')?;
    let end = url.find(')')?;
    let url_text = &url[..end];
    if url_text.contains(' ') || url_text.is_empty() {
        return None;
    }
    Some((&rest[1..close], url_text, at + close + 1 + 1 + end + 1))
}

/// Markdown text with its `page:` links made links to files of the format.
fn links(text: &str, root: &str, extension: &str) -> String {
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
            && let Some((label, url, end)) = link_at(text, i)
        {
            out.push_str(&format!("[{label}]({})", target(url, root, extension)));
            i = end;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Text escaped for HTML.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Inline Markdown as HTML: code spans, links, `**strong**` and
/// `*emphasis*`.
pub(crate) fn inline_html(text: &str, root: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut strong = false;
    let mut emphasis = false;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if c == '`'
            && let Some(end) = rest[1..].find('`')
        {
            out.push_str("<code>");
            out.push_str(&escape(&rest[1..=end]));
            out.push_str("</code>");
            i += end + 2;
            continue;
        }
        if c == '['
            && let Some((label, url, end)) = link_at(text, i)
        {
            out.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                escape(&target(url, root, "html")),
                inline_html(label, root)
            ));
            i = end;
            continue;
        }
        if rest.starts_with("**") && (strong || rest[2..].contains("**")) {
            out.push_str(if strong { "</strong>" } else { "<strong>" });
            strong = !strong;
            i += 2;
            continue;
        }
        if c == '*' {
            let opens = !emphasis
                && rest[1..].starts_with(|next: char| !next.is_whitespace())
                && rest[1..].contains('*');
            let closes = emphasis && !out.ends_with(' ');
            if opens || closes {
                out.push_str(if emphasis { "</em>" } else { "<em>" });
                emphasis = !emphasis;
                i += 1;
                continue;
            }
        }
        out.push_str(&escape(&c.to_string()));
        i += c.len_utf8();
    }
    if emphasis {
        out.push_str("</em>");
    }
    if strong {
        out.push_str("</strong>");
    }
    out
}

/// A heading's id: lower case letters, digits, `-` and `_`, the words
/// joined by `-` (`Timeline and parameters` gives
/// `timeline-and-parameters`), as GitHub makes anchors.
pub(crate) fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}

/// Ids given out on one page, each once (`-1`, `-2` after the first).
#[derive(Default)]
pub(crate) struct Ids(BTreeMap<String, usize>);

impl Ids {
    pub(crate) fn next(&mut self, title: &str) -> String {
        let base = slug(title);
        let count = self.0.entry(base.clone()).or_insert(0);
        let id = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        id
    }
}

/// The blocks as HTML relative to `root`, headings with ids from `ids`.
pub(crate) fn to_html(blocks: &[Block], root: &str, ids: &mut Ids) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Heading { level, text } => {
                let plain = text.replace('`', "");
                out.push_str(&format!(
                    "<h{level} id=\"{}\">{}</h{level}>\n",
                    escape(&ids.next(&plain)),
                    inline_html(text, root)
                ));
            }
            Block::Paragraph(text) => {
                out.push_str(&format!("<p>{}</p>\n", inline_html(text, root)));
            }
            Block::List(items) => list_html(items, root, &mut out),
            Block::Table { header, rows } => {
                out.push_str("<div class=\"table\"><table>\n<thead><tr>");
                for cell in header {
                    out.push_str(&format!("<th>{}</th>", inline_html(cell, root)));
                }
                out.push_str("</tr></thead>\n<tbody>\n");
                for row in rows {
                    out.push_str("<tr>");
                    for cell in row {
                        out.push_str(&format!("<td>{}</td>", inline_html(cell, root)));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</tbody></table></div>\n");
            }
            Block::Code { lang, text } => {
                let class = if lang.is_empty() {
                    String::new()
                } else {
                    format!(" class=\"language-{}\"", escape(lang))
                };
                out.push_str(&format!(
                    "<pre><code{class}>{}</code></pre>\n",
                    escape(text)
                ));
            }
        }
    }
    out
}

/// A list's items as nested HTML lists.
fn list_html(items: &[Item], root: &str, out: &mut String) {
    // The lists open, innermost last: whether each is numbered.
    let mut open: Vec<bool> = Vec::new();
    for item in items {
        let depth = item.depth.min(open.len());
        if depth + 1 > open.len() {
            out.push_str(if item.ordered { "<ol>\n" } else { "<ul>\n" });
            open.push(item.ordered);
        } else {
            while open.len() > depth + 1 {
                let numbered = open.pop().unwrap_or(false);
                out.push_str(if numbered {
                    "</li></ol>\n"
                } else {
                    "</li></ul>\n"
                });
            }
            out.push_str("</li>\n");
        }
        out.push_str(&format!("<li>{}", inline_html(&item.text, root)));
    }
    while let Some(numbered) = open.pop() {
        out.push_str(if numbered {
            "</li></ol>\n"
        } else {
            "</li></ul>\n"
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_read_into_blocks_and_written_again() {
        let text = "## Title (here)\n\nA paragraph\non two lines.\n\n- `a`: one\n  more\n  - nested\n- `b`\n\n| Field | Text |\n|---|---|\n| `E{a\\|b}` | x |\n\n```json\n{\"cmd\": \"undo\"}\n```\n<!-- mcp: internal -->\n";
        let blocks = parse(text);
        assert_eq!(
            blocks,
            vec![
                Block::Heading {
                    level: 2,
                    text: "Title (here)".to_owned()
                },
                Block::Paragraph("A paragraph on two lines.".to_owned()),
                Block::List(vec![
                    Item {
                        depth: 0,
                        ordered: false,
                        text: "`a`: one more".to_owned()
                    },
                    Item {
                        depth: 1,
                        ordered: false,
                        text: "nested".to_owned()
                    },
                    Item {
                        depth: 0,
                        ordered: false,
                        text: "`b`".to_owned()
                    },
                ]),
                Block::Table {
                    header: vec!["Field".to_owned(), "Text".to_owned()],
                    rows: vec![vec!["`E{a|b}`".to_owned(), "x".to_owned()]],
                },
                Block::Code {
                    lang: "json".to_owned(),
                    text: "{\"cmd\": \"undo\"}".to_owned()
                },
            ]
        );
        let markdown = to_markdown(&blocks, "../");
        assert!(markdown.contains("| `E{a\\|b}` | x |"));
        let html = to_html(&blocks, "../", &mut Ids::default());
        assert!(html.contains("<h2 id=\"title-here\">Title (here)</h2>"));
        assert!(html.contains("<li><code>a</code>: one more<ul>\n<li>nested</li></ul>\n</li>"));
        assert!(html.contains("<td><code>E{a|b}</code></td>"));
        assert!(html.contains("{&quot;cmd&quot;: &quot;undo&quot;}"));
    }

    #[test]
    fn page_links_point_at_the_format_of_the_page() {
        let text = "See [extrude](page:features/extrude#faces) and [x](https://mitcad.org/).";
        assert_eq!(
            links(text, "../", "md"),
            "See [extrude](../features/extrude.md#faces) and [x](https://mitcad.org/)."
        );
        assert_eq!(
            inline_html(text, ""),
            "See <a href=\"features/extrude.html#faces\">extrude</a> and <a href=\"https://mitcad.org/\">x</a>."
        );
        assert_eq!(
            inline_html("*Measure* and **bold** `a*b`", ""),
            "<em>Measure</em> and <strong>bold</strong> <code>a*b</code>"
        );
        assert_eq!(
            slug("Errors: message, places and detail"),
            "errors-message-places-and-detail"
        );
        let mut ids = Ids::default();
        assert_eq!(ids.next("Files"), "files");
        assert_eq!(ids.next("Files"), "files-1");
    }
}
