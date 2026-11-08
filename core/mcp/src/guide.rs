// SPDX-License-Identifier: MIT
//! The guides for agents (mitcad#1508, the decisions D1 and D7 of
//! mitcad#1498): how to model well in MitCad, where the reference says
//! what exists. They are Markdown in `core/mcp/guide/mitcad-modeling/`, in
//! the Agent Skills layout: `SKILL.md`, whose frontmatter names the guide
//! and says when to read it and whose body is the index (one line per
//! topic: `- [<id>](references/<id>.md): <when to read it>`), and
//! `references/<id>.md`, one topic each. They are compiled in like the
//! reference. `model_reference {"guide": "<id>"}` gives a topic and
//! `{"guide": "index"}` the index; the resources `mitcad://guide/<id>`
//! mirror them for clients that read resources.

use serde_json::{Value, json};

use crate::reference::{LIMIT, cut, echoed, mark_cut};

/// The id of the index: the body of `SKILL.md`.
pub(crate) const INDEX: &str = "index";
/// A guide's resource is this and its id.
pub(crate) const URI_PREFIX: &str = "mitcad://guide/";

/// The guide as a skill: its frontmatter and its index.
pub(crate) const SKILL: &str = include_str!("../guide/mitcad-modeling/SKILL.md");

/// The topics by id, each `references/<id>.md`. A new topic needs a line
/// here and one in the index of `SKILL.md`; a test checks both against
/// the folder.
pub(crate) const TOPICS: &[(&str, &str)] = &[(
    "principles",
    include_str!("../guide/mitcad-modeling/references/principles.md"),
)];

/// A line of the index.
pub(crate) struct Entry {
    pub(crate) id: &'static str,
    /// When to read the topic.
    pub(crate) read_when: &'static str,
}

/// `SKILL.md`'s frontmatter, without its `---` lines, and its body.
fn parts() -> (&'static str, &'static str) {
    let mut lines = SKILL.split_inclusive('\n');
    let Some(first) = lines.next().filter(|line| line.trim_end() == "---") else {
        return ("", SKILL);
    };
    let start = first.len();
    let mut offset = start;
    for line in lines {
        if line.trim_end() == "---" {
            return (
                &SKILL[start..offset],
                SKILL[offset + line.len()..].trim_start(),
            );
        }
        offset += line.len();
    }
    ("", SKILL)
}

/// A field of `SKILL.md`'s frontmatter (`name`, `description`).
pub(crate) fn field(name: &str) -> Option<&'static str> {
    parts().0.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == name).then_some(value.trim())
    })
}

/// The index: the body of `SKILL.md`.
pub(crate) fn index() -> &'static str {
    parts().1
}

/// The index's lines, in its order: those that link a topic to its file,
/// `references/<id>.md`.
pub(crate) fn entries() -> Vec<Entry> {
    index()
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("- [")?;
            let (id, rest) = rest.split_once("](")?;
            let (path, rest) = rest.split_once(')')?;
            if path != format!("references/{id}.md") {
                return None;
            }
            let read_when = rest.trim_start().strip_prefix(':')?.trim();
            Some(Entry { id, read_when })
        })
        .collect()
}

/// A topic's text.
pub(crate) fn topic(id: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(topic, _)| *topic == id)
        .map(|(_, text)| *text)
}

/// The text of a guide id: the index or a topic.
fn text(id: &str) -> Option<&'static str> {
    if id == INDEX {
        Some(index())
    } else {
        topic(id)
    }
}

/// The guides as the contents of `model_reference` list them: each
/// topic's id and when to read it.
pub(crate) fn guides() -> Value {
    Value::Array(
        entries()
            .iter()
            .map(|entry| json!({"id": entry.id, "read_when": entry.read_when}))
            .collect(),
    )
}

/// The answer of `model_reference {"guide": id}`, from `offset`.
pub(crate) fn answer(id: &str, offset: usize) -> Result<Value, String> {
    let Some(whole) = text(id) else {
        let ids: Vec<&str> = std::iter::once(INDEX)
            .chain(TOPICS.iter().map(|(topic, _)| *topic))
            .collect();
        return Err(format!(
            "no guide {}; the guides: {}. model_reference {{\"guide\": \"index\"}} says when to read each",
            echoed(id),
            ids.join(", ")
        ));
    };
    let (text, next) = cut(whole, offset, LIMIT)?;
    let mut answer = json!({"guide": id, "text": text});
    mark_cut(&mut answer, next);
    Ok(answer)
}

/// `resources/list`'s rows of the guides: the index, then the topics.
pub(crate) fn resources() -> Vec<Value> {
    let mut resources = vec![json!({
        "uri": format!("{URI_PREFIX}{INDEX}"),
        "name": "MitCad modeling guide: index",
        "description": field("description").unwrap_or_default(),
        "mimeType": "text/markdown"
    })];
    resources.extend(
        entries()
            .into_iter()
            .filter(|entry| topic(entry.id).is_some())
            .map(|entry| {
                json!({
                    "uri": format!("{URI_PREFIX}{}", entry.id),
                    "name": format!("MitCad modeling guide: {}", entry.id),
                    "description": format!("Read {}", entry.read_when),
                    "mimeType": "text/markdown"
                })
            }),
    );
    resources
}

/// `resources/read`'s result for a guide's URI; None for another URI.
pub(crate) fn read(uri: &str) -> Option<Value> {
    let text = text(uri.strip_prefix(URI_PREFIX)?)?;
    Some(json!({"contents": [{
        "uri": uri,
        "mimeType": "text/markdown",
        "text": text
    }]}))
}
