// SPDX-License-Identifier: MIT
//! The agent's tool calls as groups in the chat panel (mitcad#983): the
//! calls that follow each other in the conversation, with nothing else
//! between them, are one row that opens to the calls. The agent's text, an
//! approval card, a notice or the user's message ends a group; the next
//! call starts a new one.
//!
//! The panel's entries in (`role`; a call's `name`, `summary` and `state`
//! running, done or failed, or a history entry's `error`), the groups out:
//!
//! | field | meaning |
//! |---|---|
//! | `first`, `last` | the indices of the group's first and last call |
//! | `count`, `failed`, `running` | how many calls, failed ones, running ones |
//! | `state` | the latest call's state: `running`, `done`, `failed` |
//! | `latest` | the latest call's line: `model_query · bodies — running` |
//! | `head` | the group's row: `3 calls (1 failed) · model_query · bodies — running`; one call's line alone |
//!
//! A call's line is its tool, what it did when known, and `— failed` or
//! `— running` unless it is done: the same line as the call's own row.

use serde_json::{Value, json};

use super::text_field;

/// `tool_groups`: `entries` → `groups`; with `index`, also `group`, the one
/// holding that entry (null when it is no call).
pub fn groups(request: &Value) -> Value {
    let entries = request
        .get("entries")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut found = Vec::new();
    let mut start: Option<usize> = None;
    for (index, entry) in entries.iter().enumerate() {
        if text_field(entry, "role") == "tool" {
            start.get_or_insert(index);
        } else if let Some(first) = start.take() {
            found.push(group(entries, first, index - 1));
        }
    }
    if let Some(first) = start {
        found.push(group(entries, first, entries.len() - 1));
    }
    let holding = request
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| {
            found.iter().find(|group| {
                group["first"].as_u64().is_some_and(|first| first <= index)
                    && group["last"].as_u64().is_some_and(|last| index <= last)
            })
        })
        .cloned()
        .unwrap_or(Value::Null);
    json!({"groups": found, "group": holding})
}

/// A call's state: its `state`, else a history entry's `error`.
fn state(call: &Value) -> &'static str {
    match call.get("state").and_then(Value::as_str) {
        Some("done") => "done",
        Some("failed") => "failed",
        Some(_) => "running",
        None => {
            if call.get("error").and_then(Value::as_bool) == Some(true) {
                "failed"
            } else {
                "done"
            }
        }
    }
}

/// A call's line: `name · summary — failed`.
fn line(call: &Value) -> String {
    let mut text = text_field(call, "name").to_owned();
    let summary = text_field(call, "summary");
    if !summary.is_empty() {
        text += " · ";
        text += summary;
    }
    match state(call) {
        "failed" => text += " — failed",
        "running" => text += " — running",
        _ => {}
    }
    text
}

fn group(entries: &[Value], first: usize, last: usize) -> Value {
    let calls = &entries[first..=last];
    let count = calls.len();
    let failed = calls.iter().filter(|call| state(call) == "failed").count();
    let running = calls.iter().filter(|call| state(call) == "running").count();
    let latest = calls.last().map(line).unwrap_or_default();
    let latest_state = calls.last().map_or("done", state);
    let head = if count < 2 {
        latest.clone()
    } else if failed > 0 {
        format!("{count} calls ({failed} failed) · {latest}")
    } else {
        format!("{count} calls · {latest}")
    };
    json!({
        "first": first,
        "last": last,
        "count": count,
        "failed": failed,
        "running": running,
        "state": latest_state,
        "latest": latest,
        "head": head,
    })
}
