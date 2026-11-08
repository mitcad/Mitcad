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
//! | `account_error` | `account` (`code`, `reason`), `text` | account access failed; kept until a fresh check passes |
//! | `status` | `subtype`, `text` | another message of the agent (a retry, a compaction) |
//! | `output` | `text` | a line that is no message of the protocol (logged, not shown) |
//! | `bridge` | `bridge`, `bridge_error` | MitCad's tools connected or failed after `init` (Codex) |
//! | `models` | `models` (`id`, `name`, `default`) | the models the agent offers (Codex) |
//! | `write` | `line` | a line for the agent's stdin (Codex's answers and requests; not shown) |
//! | `end` | `text` | the conversation cannot go on: the process is ended (Codex; not shown) |
//!
//! Codex's `init` adds `resume_failed`: the thread to resume was gone and a
//! new one started.
//!
//! The API agents (phase 2, [`api`]: the Anthropic API, Ollama, LM Studio,
//! a custom OpenAI-compatible endpoint) run their tool loop in MitCad: their
//! stream adds `mcp` (`line`: for MitCad's own listener, on the chat's
//! connection), `http` (`id`, `method`, `url`, `headers`, `auth`, `body`: a
//! request to the model's service), `http_abort` (`id`) and `transcript`
//! (`session`, `content`: the conversation to keep for resuming); its
//! `init` adds `resume_failed` too, and a `turn_end` whose model cannot call
//! tools has the subtype `no_tools`.
//!
//! A custom command that speaks the Agent Client Protocol (mitcad#1198,
//! [`acp`]) is driven as Codex is (`start`, `message`, `interrupt`; `write`,
//! `end`, `models`); its `init` adds `resume_failed`, and its turns end with
//! the protocol's stop reasons as subtypes (`end_turn`, `cancelled`,
//! `max_tokens`, `max_turn_requests`, `refusal`).

mod account;
mod acp;
mod api;
mod claude;
mod codex;
mod groups;
mod history;
mod policy;
mod undo;

#[cfg(test)]
mod acp_tests;
#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod tests;

pub use acp::{ACP_PROTOCOL_VERSION, AcpStream};
pub use api::ApiStream;
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
    Api(Box<ApiStream>),
    Acp(Box<AcpStream>),
}

impl Stream {
    /// The stream of the agent named `agent` (`claude`, `codex`, `acp`; the
    /// API agents').
    pub fn new(agent: &str) -> Result<Stream, String> {
        match agent {
            "claude" => Ok(Stream::Claude(ClaudeStream::default())),
            "codex" => Ok(Stream::Codex(Box::default())),
            "acp" => Ok(Stream::Acp(Box::default())),
            other => match ApiStream::new(other) {
                Some(stream) => Ok(Stream::Api(Box::new(stream))),
                None => Err(format!("unknown chat agent “{}”", crate::echoed(other))),
            },
        }
    }

    /// The panel events of one output line (without its newline): none, one
    /// or several. An API agent's lines are its MCP connection's: its account
    /// failures come from the model's service ([`api`]).
    pub fn feed(&mut self, line: &str) -> Vec<Value> {
        let events = match self {
            Stream::Claude(stream) => stream.feed(line),
            Stream::Codex(stream) => stream.feed(line),
            Stream::Acp(stream) => stream.feed(line),
            Stream::Api(stream) => return stream.feed(line),
        };
        account::events(line, events)
    }

    /// A control of a stream that holds a protocol's state (Codex and a
    /// custom ACP agent: `start`,
    /// `message`, `interrupt`; an API agent: those and `http_line`,
    /// `http_end`): its panel events, `write` among them. Claude
    /// Code's stream has none (its command line and messages are
    /// [`request`]s).
    pub fn control(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        match self {
            Stream::Claude(_) => Err("Claude Code's stream takes no controls".to_owned()),
            Stream::Codex(stream) => stream.control(request),
            Stream::Api(stream) => stream.control(request),
            Stream::Acp(stream) => stream.control(request),
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
///   `signed_in`, `method`, `account`, `organization`, `subscription`
///   (each null when not told)
/// - `account_error`: `text`, `details` (optional structured error) → `code`, `reason` or null
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
/// - `api_presets` → `agents` (`id`, `name`, `url`, `key`, `page`): the API
///   agents and their preset addresses
/// - `api_endpoint`: `url` → `url` (checked: https, or http to this
///   computer only), `local`; an error for another address
/// - `api_models_request`: `agent`, `url` → `url`, `headers`, `auth`
/// - `api_models`: `agent`, `status`, `body`, `error` → `ok`, `models`
///   (`id`, `name`), `account`, `text`
/// - `api_default_models`: `agent` → `models` offered before the service
///   listed its own
/// - `acp_command`: `command`, `arguments` (one line) → `program`,
///   `arguments` (its words): the custom agent's command line (mitcad#1198)
/// - `acp_handshake` → `line`: the setup check's `initialize` request
/// - `acp_handshake_answer`: `output` (what the command wrote) → `state`
///   (`waiting`, `ok`, `failed`), `agent` (its name and version), `reason`
/// - `tool_groups`: `entries` (the panel's: `role`; a call's `name`,
///   `summary`, `state` or `error`), `index` (optional) → `groups` (`first`,
///   `last`, `count`, `failed`, `running`, `state`, `latest`, `head`) and
///   `group`, the one holding `index` (the panel's group rows, mitcad#983)
/// - `units_context`: `length` (the design's length unit, as the document
///   query gives it) → `context`: what a message tells the agent about it,
///   empty in millimetres (mitcad#1452)
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
        "account_error" => Ok(account::classify(
            request.get("details").unwrap_or(&Value::Null),
            text_field(request, "text"),
        )
        .unwrap_or(Value::Null)),
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
        "api_presets" => Ok(api::presets()),
        "api_endpoint" => {
            let url = api::endpoint(text_field(request, "url"))?;
            Ok(json!({"local": api::local(&url), "url": url}))
        }
        "api_models_request" => api::models_request(request),
        "api_models" => Ok(api::models(request)),
        "api_default_models" => Ok(api::offered(text_field(request, "agent"))),
        "acp_command" => acp::command(request),
        "acp_handshake" => Ok(acp::handshake()),
        "acp_handshake_answer" => Ok(acp::handshake_answer(text_field(request, "output"))),
        "units_context" => Ok(json!({"context": units_context(text_field(request, "length"))})),
        other => Err(format!("unknown chat op “{}”", crate::echoed(other))),
    }
}

/// What a chat message tells the agent about the design's length unit
/// `length` (mitcad#1452): in a design that is not in millimetres, a length
/// the user writes without a unit is in the design's unit, while the JSON
/// numbers of MitCad's commands are millimetres in every design. An agent
/// that took the user's `0.875` (inches) as a JSON number made a part
/// 25.4 times too small. Empty in millimetres, which the instructions
/// already say.
fn units_context(length: &str) -> String {
    let unit = length.trim();
    if unit.is_empty() || unit == "mm" {
        return String::new();
    }
    let size = match unit {
        "um" => Some("0.001"),
        "cm" => Some("10"),
        "m" => Some("1000"),
        "in" => Some("25.4"),
        "ft" => Some("304.8"),
        "mil" => Some("0.0254"),
        "yd" => Some("914.4"),
        _ => None,
    };
    let convert = size.map_or_else(String::new, |mm| format!(" (1 {unit} = {mm} mm)"));
    format!(
        "The design's length unit is {unit}: a length the user gives without a unit is in {unit}. \
         JSON numbers in MitCad's commands are millimetres in every design, so convert such a \
         length to millimetres{convert}, or give a value as text, which is read in the design's \
         unit (\"0.875\" or \"0.875 {unit}\"); coordinates of positions (corner, center, at, \
         start, end) are numbers in millimetres only."
    )
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
