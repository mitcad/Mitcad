// SPDX-License-Identifier: MIT
//! The chat panel's own logic (mitcad#947), without Qt and without IO: the
//! agents' stream protocols turned into one vocabulary of panel events,
//! the agents' command lines and input messages, which tool calls of the
//! chat's own bridge connection ask the user first, whether a message's
//! undo steps merge into one, the format of the conversation history and
//! which tool calls the panel shows as one group row.
//!
//! The application (`app/agent/`) runs the agent processes, shows the
//! panel, keeps the files and asks the window; it calls [`request`] with a
//! JSON object naming the operation (`op`) and reads its JSON answer, and
//! feeds each line an agent writes to a [`Stream`].
//!
//! The panel events, whichever agent wrote them (`kind`):
//!
//! | kind | fields | meaning |
//! |---|---|---|
//! | `init` | `session`, `model`, `bridge` (`connected`, `pending`, `failed`, `missing`), `bridge_error` | the agent started |
//! | `text` | `text` | a piece of the streamed answer |
//! | `tool_start` | `id`, `name` | a tool call begins (its input follows) |
//! | `tool` | `id`, `name`, `server`, `input` | a tool call with its input |
//! | `tool_result` | `id`, `error`, `text` | a tool call's result (`text` cut to 2000 characters) |
//! | `turn_end` | `error`, `subtype`, `text`, `session` | the turn ended |
//! | `status` | `subtype`, `text` | another message of the agent (a retry, a compaction) |
//! | `output` | `text` | a line that is no message of the protocol (logged, not shown) |
//! | `bridge` | `bridge`, `bridge_error` | MitCad's tools connected or failed after `init` (Codex) |
//! | `models` | `models` (`id`, `name`, `default`) | the models the agent offers (Codex) |
//! | `write` | `line` | a line for the agent's stdin (Codex's answers and requests; not shown) |
//! | `end` | `text` | the conversation cannot go on: the process is ended (Codex; not shown) |
//!
//! Codex's `init` adds `resume_failed`: the thread to resume was gone and a
//! new one started.

mod claude;
mod codex;
mod groups;
mod history;
mod policy;
mod undo;

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod tests;

pub use claude::{CLAUDE_MINIMUM_VERSION, ClaudeStream};
pub use codex::{CODEX_MINIMUM_VERSION, CodexStream};

use serde_json::{Value, json};

/// The longest text a panel event or a history entry keeps of one field.
pub const MAX_TEXT_CHARS: usize = 64 * 1024;
/// The longest tool result text a panel event keeps.
pub const MAX_RESULT_CHARS: usize = 2000;

/// A stream of one agent's output lines, turned into panel events.
pub enum Stream {
    Claude(ClaudeStream),
    Codex(Box<CodexStream>),
}

impl Stream {
    /// The stream of the agent named `agent` (`claude`, `codex`).
    pub fn new(agent: &str) -> Result<Stream, String> {
        match agent {
            "claude" => Ok(Stream::Claude(ClaudeStream::default())),
            "codex" => Ok(Stream::Codex(Box::default())),
            other => Err(format!("unknown chat agent '{}'", crate::echoed(other))),
        }
    }

    /// The panel events of one output line (without its newline): none, one
    /// or several.
    pub fn feed(&mut self, line: &str) -> Vec<Value> {
        match self {
            Stream::Claude(stream) => stream.feed(line),
            Stream::Codex(stream) => stream.feed(line),
        }
    }

    /// A control of a stream that holds a protocol's state (Codex: `start`,
    /// `message`, `interrupt`): its panel events, `write` among them. Claude
    /// Code's stream has none (its command line and messages are
    /// [`request`]s).
    pub fn control(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        match self {
            Stream::Claude(_) => Err("Claude Code's stream takes no controls".to_owned()),
            Stream::Codex(stream) => stream.control(request),
        }
    }
}

/// A model's name as an agent's option or parameter: letters, digits and
/// `. - _ : [ ] /`, not starting with a dash.
pub(crate) fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 100
        && !model.starts_with('-')
        && model.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']' | '/')
        })
}

/// The pictures of a message request (`images`: `media_type`, `data`):
/// PNG or JPEG, base64.
pub(crate) fn pictures(request: &Value) -> Result<Vec<(&str, &str)>, String> {
    let mut found = Vec::new();
    for image in request
        .get("images")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let media = text_field(image, "media_type");
        let data = text_field(image, "data");
        if !matches!(media, "image/png" | "image/jpeg") {
            return Err("a picture must be PNG or JPEG".to_owned());
        }
        if data.is_empty()
            || !data
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        {
            return Err("a picture must be base64 data".to_owned());
        }
        found.push((media, data));
    }
    Ok(found)
}

/// One operation of the chat's logic: `{"op": ..., ...}` in, its answer out.
///
/// - `claude_arguments`: `session`, `resume`, `bridge`, `instance`, `chat`,
///   `model` (optional) → `arguments` (the command line after the program)
/// - `claude_message`: `text`, `context` (optional), `images` (`media_type`,
///   `data`) → `line` (one stdin line with its newline)
/// - `claude_version`: `output` (of `claude --version`) → `version`, `ok`,
///   `minimum`
/// - `claude_auth`: `exit_code`, `output` (of `claude auth status`) →
///   `signed_in`, `method`
/// - `claude_models` → `models` (`id`, `name`): Claude Code's model
///   aliases for its `--model`
/// - `codex_arguments` → `arguments` (`app-server`; the rest goes through
///   the stream's `start` control)
/// - `codex_version`: `output` (of `codex --version`) → `version`, `ok`,
///   `minimum`
/// - `codex_auth`: `exit_code`, `output` (of `codex login status`) →
///   `signed_in`, `method`
/// - `approval`: `tool`, `arguments`, `mode`, `always`, `turn_depth`,
///   `undo_depth` → `verdict` (`run`, `ask`), `kind`, `path`
/// - `undo_merge`: `start_depth`, `end_depth`, `user_edited`, `sketch`,
///   `panel`, `message` → `merge`, `depth`, `label`, `reason`
/// - `history_entry`: `entry` → `line`
/// - `history_read`: `content`, `limit` → `entries`, `sessions`, `skipped`
/// - `tool_groups`: `entries` (the panel's: `role`; a call's `name`,
///   `summary`, `state` or `error`), `index` (optional) → `groups` (`first`,
///   `last`, `count`, `failed`, `running`, `state`, `latest`, `head`) and
///   `group`, the one holding `index` (the panel's group rows, mitcad#983)
pub fn request(request: &Value) -> Result<Value, String> {
    let op = request
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| "a chat request needs its op".to_owned())?;
    match op {
        "claude_arguments" => claude::arguments(request),
        "claude_message" => claude::message(request),
        "claude_version" => Ok(claude::version(text_field(request, "output"))),
        "claude_auth" => Ok(claude::auth(
            request
                .get("exit_code")
                .and_then(Value::as_i64)
                .unwrap_or(-1),
            text_field(request, "output"),
        )),
        "claude_models" => Ok(claude::models()),
        "codex_arguments" => Ok(codex::arguments()),
        "codex_version" => Ok(codex::version(text_field(request, "output"))),
        "codex_auth" => Ok(codex::auth(
            request
                .get("exit_code")
                .and_then(Value::as_i64)
                .unwrap_or(-1),
            text_field(request, "output"),
        )),
        "approval" => Ok(policy::decide(request)),
        "undo_merge" => Ok(undo::decide(request)),
        "history_entry" => history::entry_line(request.get("entry").unwrap_or(&Value::Null)),
        "history_read" => Ok(history::read(
            text_field(request, "content"),
            request
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(history::DEFAULT_LIMIT, |limit| limit as usize),
        )),
        "tool_groups" => Ok(groups::groups(request)),
        other => Err(format!("unknown chat op '{}'", crate::echoed(other))),
    }
}

/// [`request`] on JSON text, answered in JSON text; an error is an answer
/// with `error`.
pub fn request_text(text: &str) -> String {
    let answer = serde_json::from_str::<Value>(text)
        .map_err(|error| format!("invalid chat request: {error}"))
        .and_then(|value| request(&value));
    match answer {
        Ok(value) => value.to_string(),
        Err(error) => json!({"error": error}).to_string(),
    }
}

fn text_field<'a>(value: &'a Value, name: &str) -> &'a str {
    value.get(name).and_then(Value::as_str).unwrap_or_default()
}

/// At most `limit` characters of `text`, cut at a character's boundary.
pub(crate) fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}
