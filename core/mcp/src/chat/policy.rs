// SPDX-License-Identifier: MIT
//! Which tool calls of the chat's own bridge connection ask the user first
//! (mitcad#947). The live server asks this for every call of that
//! connection before it runs the tool; the panel shows a card for `ask`.
//!
//! The approval modes (Preferences › AI Assistants › Chat):
//!
//! | mode | reads | model edits | file operations, undo past the message's start, other tools |
//! |---|---|---|---|
//! | `ask` (Ask every time) | run | ask | ask |
//! | `model` (Model edits allowed, the default) | run | run | ask |
//! | `all` (Allow everything) | run | run | run |
//!
//! A kind the user allowed always (Always allow, per agent) runs in every
//! mode. The kinds: `read`, `edit`, `document_save`, `document_open`,
//! `model_export`, `screenshot_file`, `file_command` (a `file.*` command of
//! `ui_command`), `undo_past_start` and `other` (a tool this policy does not
//! know, which asks).

use serde_json::{Value, json};

use super::text_field;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Read,
    Edit,
    // File operations, an undo past the start and unknown tools.
    Guarded,
}

pub fn decide(request: &Value) -> Value {
    let tool = text_field(request, "tool");
    let arguments = request.get("arguments").unwrap_or(&Value::Null);
    let mode = match text_field(request, "mode") {
        "ask" => "ask",
        "all" => "all",
        _ => "model",
    };
    let always: Vec<&str> = request
        .get("always")
        .and_then(Value::as_array)
        .map(|kinds| kinds.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let past_start = match (
        request.get("turn_depth").and_then(Value::as_i64),
        request.get("undo_depth").and_then(Value::as_i64),
    ) {
        (Some(start), Some(now)) => now <= start,
        // No message's start to keep: every undo is past it.
        _ => true,
    };
    let (class, kind, path) = classify(tool, arguments, past_start);
    let allowed = always.contains(&kind);
    let ask = match class {
        Class::Read => false,
        Class::Edit => mode == "ask" && !allowed,
        Class::Guarded => mode != "all" && !allowed,
    };
    json!({
        "verdict": if ask { "ask" } else { "run" },
        "kind": kind,
        "path": path,
    })
}

fn classify(
    tool: &str,
    arguments: &Value,
    past_start: bool,
) -> (Class, &'static str, Option<String>) {
    let path = || {
        arguments
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(|path| super::cut(path, 1000))
    };
    match tool {
        "document_list" | "model_query" | "ui_state" | "ui_log" | "ui_camera" | "ui_select" => {
            (Class::Read, "read", None)
        }
        "ui_screenshot" => match path() {
            Some(file) => (Class::Guarded, "screenshot_file", Some(file)),
            None => (Class::Read, "read", None),
        },
        "model_command" => {
            let undo = arguments
                .get("command")
                .and_then(|command| command.get("cmd"))
                .and_then(Value::as_str)
                == Some("undo");
            if undo && past_start {
                (Class::Guarded, "undo_past_start", None)
            } else {
                (Class::Edit, "edit", None)
            }
        }
        "ui_command" => {
            let id = text_field(arguments, "id");
            if id == "edit.undo" && past_start {
                (Class::Guarded, "undo_past_start", None)
            } else if id.starts_with("file.") {
                (Class::Guarded, "file_command", None)
            } else {
                (Class::Edit, "edit", None)
            }
        }
        "document_save" => (Class::Guarded, "document_save", path()),
        "document_open" => (Class::Guarded, "document_open", path()),
        "model_export" => (Class::Guarded, "model_export", path()),
        _ => (Class::Guarded, "other", None),
    }
}
