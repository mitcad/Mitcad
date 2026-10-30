// SPDX-License-Identifier: MIT
//! The chat's history (mitcad#947): one JSON Lines file per design in the
//! user's data folder (the application keeps the files), one entry per
//! line:
//!
//! ```json
//! {"v":1,"role":"user","time":"2026-10-29T12:00:00Z","text":"a 30 x 20 x 10 mm box","agent":"claude","session":"…"}
//! ```
//!
//! `role`: `user`, `assistant`, `tool` (`name`, `summary`, `error`) or
//! `notice`; a notice with `"event": "new_conversation"` starts a new
//! conversation, and what came before it is not shown again. `agent` and
//! `session` say which agent session the entry belongs to: the last one
//! of each agent in the current conversation is the one the panel
//! resumes (Claude Code's session id, Codex's thread id). A line that cannot be read
//! is skipped; the file is only appended to.

use std::collections::VecDeque;

use serde_json::{Map, Value, json};

use super::{MAX_TEXT_CHARS, cut};

/// The entries shown of one design's history at most.
pub const DEFAULT_LIMIT: usize = 500;
const VERSION: u64 = 1;
const ROLES: [&str; 4] = ["user", "assistant", "tool", "notice"];
const TEXT_FIELDS: [&str; 7] = [
    "time", "text", "agent", "session", "name", "summary", "event",
];

/// One entry as its line, with the newline: the known fields only, the
/// texts cut.
pub fn entry_line(entry: &Value) -> Result<Value, String> {
    let entry = entry
        .as_object()
        .ok_or_else(|| "a history entry is a JSON object".to_owned())?;
    let role = entry
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !ROLES.contains(&role) {
        return Err("a history entry needs its role: user, assistant, tool or notice".to_owned());
    }
    Ok(json!({"line": kept(entry).to_string() + "\n"}))
}

fn kept(entry: &Map<String, Value>) -> Value {
    let mut kept = Map::new();
    kept.insert("v".to_owned(), json!(VERSION));
    kept.insert(
        "role".to_owned(),
        entry.get("role").cloned().unwrap_or(Value::Null),
    );
    for field in TEXT_FIELDS {
        if let Some(text) = entry.get(field).and_then(Value::as_str) {
            kept.insert(field.to_owned(), json!(cut(text, MAX_TEXT_CHARS)));
        }
    }
    if let Some(error) = entry.get("error").and_then(Value::as_bool) {
        kept.insert("error".to_owned(), json!(error));
    }
    Value::Object(kept)
}

/// The entries of a history file's content (the last `limit` of its
/// current conversation), the last session of each agent and how many
/// lines were skipped.
pub fn read(content: &str, limit: usize) -> Value {
    let mut entries = VecDeque::new();
    let mut sessions = Map::new();
    let mut skipped = 0u64;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(Value::Object(entry)) = serde_json::from_str::<Value>(line) else {
            skipped += 1;
            continue;
        };
        let readable = entry
            .get("v")
            .and_then(Value::as_u64)
            .is_some_and(|v| v <= VERSION)
            && entry
                .get("role")
                .and_then(Value::as_str)
                .is_some_and(|role| ROLES.contains(&role));
        if !readable {
            skipped += 1;
            continue;
        }
        if entry.get("event").and_then(Value::as_str) == Some("new_conversation") {
            // Only the current conversation's sessions resume (another
            // agent chosen in the panel starts a new conversation).
            sessions.clear();
        }
        if let (Some(agent), Some(session)) = (
            entry.get("agent").and_then(Value::as_str),
            entry.get("session").and_then(Value::as_str),
        ) && !agent.is_empty()
            && !session.is_empty()
        {
            sessions.insert(agent.to_owned(), json!(session));
        }
        if entry.get("event").and_then(Value::as_str) == Some("new_conversation") {
            entries.clear();
        }
        entries.push_back(kept(&entry));
        while entries.len() > limit.max(1) {
            entries.pop_front();
        }
    }
    json!({"entries": Vec::from(entries), "sessions": sessions, "skipped": skipped})
}
