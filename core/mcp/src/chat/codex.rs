// SPDX-License-Identifier: MIT
//! Codex through its own app server (mitcad#947): `codex app-server`, one
//! long-lived process per conversation speaking JSON-RPC over stdio (one
//! JSON object per line, without the `"jsonrpc"` member), as the Codex
//! project documents it: `initialize` and `initialized`, `thread/start` or
//! `thread/resume`, `turn/start` per message, `turn/interrupt` for Stop, and
//! the notifications of a turn until `turn/completed`.
//!
//! The message types follow the JSON Schema that `codex app-server
//! generate-json-schema` writes (checked against codex-cli 0.157.1, its
//! stable API without `--experimental`); the configuration keys follow the
//! keys that version knows. Everything of the protocol is in this one file,
//! so that a change of it touches this file and its tests only.
//!
//! MitCad's tools are the conversation's MCP server through the thread's
//! configuration overrides (`config` of `thread/start` and `thread/resume`):
//! `mcp_servers` given as the window's bridge alone, so that the user's
//! other MCP servers stay out (Codex layers the overrides over the user's
//! `config.toml`; whether a layer's table replaces the user's or merges with
//! it is for a real run to show, so another server's start is logged), the
//! built-in shell, picture and web search tools off and a read-only sandbox
//! (which also keeps `apply_patch` from writing: no key of the current Codex
//! removes that tool). Nothing is written into the user's `config.toml`.
//! Codex itself asks nothing (`approvalPolicy` `never`): MitCad's approvals
//! are enforced by the live server for the chat's bridge connection, as for
//! Claude Code. A request Codex still sends is answered here: a confirmation
//! of a call of MitCad's own server is accepted (the live server decides),
//! every other approval is declined.
//!
//! The stream is driven from both sides: [`CodexStream::control`] (start, a
//! message, an interrupt) and [`CodexStream::feed`] (a line Codex wrote)
//! return panel events, among them two the application acts on itself:
//! `write` (a line for Codex's stdin) and `end` (the conversation cannot go
//! on: the process is ended, `text` says why). `models` lists the models
//! Codex offers (`model/list`).

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::{MAX_RESULT_CHARS, MAX_TEXT_CHARS, cut, pictures, text_field, valid_model};

/// The oldest Codex the chat starts: the app server's thread and turn
/// requests (`thread/start` with its `config` overrides, `thread/resume`,
/// `turn/start` with pictures, `turn/interrupt`) and `codex login status`.
/// The messages were checked against 0.157.1's schema only; that 0.80.0
/// already has all of them is not checked.
pub const CODEX_MINIMUM_VERSION: &str = "0.80.0";

/// The live MCP server's name in Codex's configuration.
const SERVER: &str = "mitcad";
/// How long Codex waits for one of MitCad's tools: a call that asks the
/// user first waits for the card's answer.
const TOOL_TIMEOUT_SECONDS: u64 = 900;
const STARTUP_TIMEOUT_SECONDS: u64 = 60;
/// Why a command or a file change of Codex's own was declined.
const REFUSAL: &str = "MitCad's chat does not run commands or change files.";

/// What a request of ours was, to read its answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Initialize,
    ThreadStart,
    ThreadResume,
    Turn,
    Interrupt,
    Models,
}

#[derive(Default)]
pub struct CodexStream {
    next: u64,
    asked: HashMap<u64, Asked>,
    // The thread's settings, kept from `start` for `thread/start` (also
    // after a failed resume).
    config: Value,
    cwd: String,
    model: String,
    resume: Option<String>,
    resume_failed: bool,
    thread: Option<String>,
    // Messages given before the thread was there, as turn/start params.
    waiting: Vec<Value>,
    // The running turn: its id once known, whether one was asked for.
    turn: Option<String>,
    turn_asked: bool,
    interrupt: bool,
    last_error: String,
    // Agent messages whose deltas streamed, tool items already shown.
    streamed: Vec<String>,
    tools: Vec<String>,
}

fn plain_id(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `codex app-server`: the MitCad bridge is given per thread, not on the
/// command line.
pub fn arguments() -> Value {
    json!({"arguments": ["app-server"]})
}

/// The version in `codex --version`'s output ("codex-cli 0.80.0") and
/// whether it is at least the minimum; a pre-release ("0.81.0-alpha.2")
/// counts as its version.
pub fn version(output: &str) -> Value {
    let found = output
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .map(|word| word.trim_start_matches('v'))
        .find_map(|word| parse_version(word.split(['-', '+']).next().unwrap_or_default()));
    match found {
        Some(parts) => json!({
            "version": format!("{}.{}.{}", parts[0], parts[1], parts[2]),
            "ok": parts >= parse_version(CODEX_MINIMUM_VERSION).unwrap_or_default(),
            "minimum": CODEX_MINIMUM_VERSION,
        }),
        None => json!({"version": null, "ok": false, "minimum": CODEX_MINIMUM_VERSION}),
    }
}

fn parse_version(word: &str) -> Option<[u64; 3]> {
    let mut parts = word.split('.');
    let mut numbers = [0u64; 3];
    for number in &mut numbers {
        *number = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

/// Signed in by `codex login status`: exit status 0, unless it says it is
/// not logged in. Only the method is read besides ("ChatGPT", "API key").
pub fn auth(exit_code: i64, output: &str) -> Value {
    let lower = output.to_lowercase();
    let not = lower.contains("not logged in") || lower.contains("not signed in");
    let method = if lower.contains("chatgpt") {
        Some("ChatGPT")
    } else if lower.contains("api key") {
        Some("API key")
    } else {
        None
    };
    json!({"signed_in": exit_code == 0 && !not, "method": method})
}

/// The thread's configuration overrides: MitCad's bridge as the only MCP
/// server, Codex's own tools off, nothing written to the sandbox. Only keys
/// the current Codex knows: the picture tool is `features.view_image` (not
/// `tools.view_image`), web search the `web_search` mode (`tools.web_search`
/// is a table of search settings now), and `include_apply_patch_tool` is
/// gone. `features.web_search_request` is the web search switch of older
/// versions, which the current one still reads as a deprecated feature.
fn thread_config(bridge: &str, instance: &str, chat: &str) -> Value {
    json!({
        "mcp_servers": {SERVER: {
            "command": bridge,
            "args": ["mcp", "--connect", "--instance", instance, "--chat-session", chat],
            "startup_timeout_sec": STARTUP_TIMEOUT_SECONDS,
            "tool_timeout_sec": TOOL_TIMEOUT_SECONDS,
        }},
        "approval_policy": "never",
        "sandbox_mode": "read-only",
        "features.shell_tool": false,
        "features.unified_exec": false,
        "features.apply_patch_freeform": false,
        "features.view_image": false,
        "features.web_search_request": false,
        "web_search": "disabled",
    })
}

fn write(message: Value) -> Value {
    json!({"kind": "write", "line": message.to_string() + "\n"})
}

fn string_of(value: &Value, name: &str, limit: usize) -> String {
    cut(
        value.get(name).and_then(Value::as_str).unwrap_or_default(),
        limit,
    )
}

/// The text of an MCP result's content blocks (a picture as `[image]`),
/// else its structured content.
fn result_text(result: &Value) -> String {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => block
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            Some("image") => "[image]".to_owned(),
            _ => String::new(),
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        return text;
    }
    match result.get("structuredContent") {
        Some(Value::Null) | None => String::new(),
        Some(structured) => structured.to_string(),
    }
}

/// An error's message: `{message}` or a plain string.
fn error_message(error: &Value) -> String {
    match error {
        Value::String(text) => cut(text, MAX_TEXT_CHARS),
        Value::Object(_) => string_of(error, "message", MAX_TEXT_CHARS),
        _ => String::new(),
    }
}

impl CodexStream {
    fn request(&mut self, asked: Asked, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.asked.insert(id, asked);
        write(json!({"method": method, "id": id, "params": params}))
    }

    /// A control of the application: `start` (`session` to resume or
    /// empty, `resume`, `bridge`, `instance`, `chat`, `model`, `cwd`),
    /// `message` (`text`, `context`, `images`) or `interrupt`.
    pub fn control(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        match text_field(request, "op") {
            "start" => self.start(request),
            "message" => self.message(request),
            "interrupt" => Ok(self.interrupt()),
            other => Err(format!("unknown Codex control '{}'", crate::echoed(other))),
        }
    }

    fn start(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        let bridge = text_field(request, "bridge");
        if bridge.is_empty() || bridge.contains('\0') {
            return Err("the chat needs the installed mitcad-cli bridge".to_owned());
        }
        let instance = text_field(request, "instance");
        let chat = text_field(request, "chat");
        if !plain_id(instance, 64) || !plain_id(chat, 64) {
            return Err("the chat needs the listener's instance id and its own marker".to_owned());
        }
        let model = text_field(request, "model");
        if !model.is_empty() && !valid_model(model) {
            return Err("the model's name has characters a model name has not".to_owned());
        }
        let resume = request
            .get("resume")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let session = text_field(request, "session");
        if resume && !plain_id(session, 128) {
            return Err("the Codex thread to resume needs its id".to_owned());
        }
        let cwd = text_field(request, "cwd");
        if cwd.contains('\0') {
            return Err("the chat's working folder is not a path".to_owned());
        }
        *self = CodexStream {
            config: thread_config(bridge, instance, chat),
            cwd: cwd.to_owned(),
            model: model.to_owned(),
            resume: resume.then(|| session.to_owned()),
            ..CodexStream::default()
        };
        let initialize = self.request(
            Asked::Initialize,
            "initialize",
            json!({"clientInfo": {"name": "mitcad", "title": "MitCad",
                                  "version": env!("CARGO_PKG_VERSION")}}),
        );
        Ok(vec![initialize])
    }

    fn thread_params(&self) -> Value {
        let mut params = Map::new();
        params.insert("approvalPolicy".to_owned(), json!("never"));
        params.insert("config".to_owned(), self.config.clone());
        if !self.cwd.is_empty() {
            params.insert("cwd".to_owned(), json!(self.cwd));
        }
        if !self.model.is_empty() {
            params.insert("model".to_owned(), json!(self.model));
        }
        Value::Object(params)
    }

    fn start_thread(&mut self) -> Value {
        let params = self.thread_params();
        self.request(Asked::ThreadStart, "thread/start", params)
    }

    fn message(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        let text = text_field(request, "text").trim();
        if text.is_empty() {
            return Err("a message needs its text".to_owned());
        }
        let mut input = vec![json!({"type": "text", "text": cut(text, MAX_TEXT_CHARS)})];
        let context = text_field(request, "context").trim();
        if !context.is_empty() {
            input.push(json!({"type": "text", "text": cut(context, MAX_TEXT_CHARS)}));
        }
        for (media, data) in pictures(request)? {
            input.push(json!({"type": "image", "url": format!("data:{media};base64,{data}")}));
        }
        let mut params = Map::new();
        params.insert("input".to_owned(), Value::Array(input));
        if !self.model.is_empty() {
            params.insert("model".to_owned(), json!(self.model));
        }
        self.waiting.push(Value::Object(params));
        Ok(self.next_turn())
    }

    /// The next waiting message as a turn, once the thread is there and no
    /// turn runs.
    fn next_turn(&mut self) -> Vec<Value> {
        let Some(thread) = self.thread.clone() else {
            return Vec::new();
        };
        if self.turn_asked || self.turn.is_some() || self.waiting.is_empty() {
            return Vec::new();
        }
        let mut params = self.waiting.remove(0);
        if let Some(object) = params.as_object_mut() {
            object.insert("threadId".to_owned(), json!(thread));
        }
        self.turn_asked = true;
        self.last_error.clear();
        vec![self.request(Asked::Turn, "turn/start", params)]
    }

    fn interrupt(&mut self) -> Vec<Value> {
        if let (Some(thread), Some(turn)) = (self.thread.clone(), self.turn.clone()) {
            return vec![self.request(
                Asked::Interrupt,
                "turn/interrupt",
                json!({"threadId": thread, "turnId": turn}),
            )];
        }
        if self.turn_asked {
            // Its id comes with turn/start's answer or turn/started.
            self.interrupt = true;
            return Vec::new();
        }
        if !self.waiting.is_empty() {
            // Not given to Codex yet: it ends here.
            self.waiting.clear();
            return vec![self.turn_end("interrupted", true, String::new())];
        }
        Vec::new()
    }

    fn turn_end(&self, status: &str, error: bool, text: String) -> Value {
        json!({
            "kind": "turn_end",
            "error": error,
            "subtype": status,
            "text": text,
            "session": self.thread.clone().unwrap_or_default(),
        })
    }

    /// The turn's id is known: an interrupt asked meanwhile goes now.
    fn turn_known(&mut self, id: &str) -> Vec<Value> {
        if id.is_empty() || self.turn.is_some() {
            return Vec::new();
        }
        self.turn = Some(id.to_owned());
        if std::mem::take(&mut self.interrupt) {
            return self.interrupt();
        }
        Vec::new()
    }

    /// The panel events of one line Codex wrote (without its newline).
    pub fn feed(&mut self, line: &str) -> Vec<Value> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Vec::new();
        }
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(line) else {
            return vec![json!({"kind": "output", "text": cut(line, 500)})];
        };
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").filter(|id| !id.is_null());
        let params = message.get("params").unwrap_or(&Value::Null);
        match (method, id) {
            (Some(method), Some(id)) => self.server_request(method, id, params),
            (Some(method), None) => self.notification(method, params),
            (None, Some(id)) => self.answer(id, &message),
            (None, None) => vec![json!({"kind": "output", "text": cut(line, 500)})],
        }
    }

    fn answer(&mut self, id: &Value, message: &Map<String, Value>) -> Vec<Value> {
        let Some(asked) = id.as_u64().and_then(|id| self.asked.remove(&id)) else {
            return Vec::new();
        };
        let result = message.get("result").unwrap_or(&Value::Null);
        let failure = message.get("error").filter(|error| !error.is_null());
        if let Some(error) = failure {
            return self.refused(asked, &error_message(error));
        }
        match asked {
            Asked::Initialize => {
                // A notification without params (the schema has none).
                let mut events = vec![write(json!({"method": "initialized"}))];
                events.push(match self.resume.clone() {
                    Some(thread) => {
                        let mut params = self.thread_params();
                        if let Some(object) = params.as_object_mut() {
                            object.insert("threadId".to_owned(), json!(thread));
                        }
                        self.request(Asked::ThreadResume, "thread/resume", params)
                    }
                    None => self.start_thread(),
                });
                events.push(self.request(Asked::Models, "model/list", json!({})));
                events
            }
            Asked::ThreadStart | Asked::ThreadResume => {
                let thread = result.get("thread").unwrap_or(&Value::Null);
                let id = string_of(thread, "id", 200);
                if id.is_empty() {
                    return vec![json!({"kind": "end",
                                       "text": "Codex started no conversation (no thread id)."})];
                }
                self.thread = Some(id.clone());
                let model = [result.get("model"), thread.get("model")]
                    .into_iter()
                    .flatten()
                    .find_map(Value::as_str)
                    .map(|model| cut(model, 100))
                    .unwrap_or_else(|| self.model.clone());
                let mut events = vec![json!({
                    "kind": "init",
                    "session": id,
                    "model": model,
                    "bridge": "pending",
                    "bridge_error": null,
                    "resume_failed": self.resume_failed,
                })];
                events.extend(self.next_turn());
                events
            }
            Asked::Turn => {
                let turn = string_of(result.get("turn").unwrap_or(&Value::Null), "id", 200);
                self.turn_known(&turn)
            }
            Asked::Interrupt => Vec::new(),
            Asked::Models => {
                let models: Vec<Value> = result
                    .get("data")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter(|model| !model.get("hidden").and_then(Value::as_bool).unwrap_or(false))
                    .filter_map(|model| {
                        let id = [model.get("model"), model.get("id")]
                            .into_iter()
                            .flatten()
                            .find_map(Value::as_str)?;
                        if !valid_model(id) {
                            return None;
                        }
                        let name = model
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or(id);
                        Some(json!({"id": id, "name": cut(name, 100),
                                    "default": model.get("isDefault").and_then(Value::as_bool).unwrap_or(false)}))
                    })
                    .take(50)
                    .collect();
                vec![json!({"kind": "models", "models": models})]
            }
        }
    }

    fn refused(&mut self, asked: Asked, message: &str) -> Vec<Value> {
        match asked {
            Asked::Initialize => vec![json!({"kind": "end",
                "text": format!("Codex refused to start: {message}")})],
            Asked::ThreadResume => {
                // The thread is gone: a new one, and the panel says so.
                self.resume = None;
                self.resume_failed = true;
                vec![
                    json!({"kind": "status", "subtype": "resume_failed", "text": cut(message, 500)}),
                    self.start_thread(),
                ]
            }
            Asked::ThreadStart => vec![json!({"kind": "end",
                "text": format!("Codex did not start a conversation: {message}")})],
            Asked::Turn => {
                self.turn_asked = false;
                self.interrupt = false;
                let mut events = vec![self.turn_end("error", true, cut(message, MAX_TEXT_CHARS))];
                events.extend(self.next_turn());
                events
            }
            Asked::Interrupt | Asked::Models => {
                let subtype = if asked == Asked::Models {
                    "models_failed"
                } else {
                    "interrupt_failed"
                };
                vec![json!({"kind": "status", "subtype": subtype, "text": cut(message, 500)})]
            }
        }
    }

    fn notification(&mut self, method: &str, params: &Value) -> Vec<Value> {
        match method {
            "turn/started" => {
                let turn = string_of(params.get("turn").unwrap_or(&Value::Null), "id", 200);
                self.turn_known(&turn)
            }
            "item/agentMessage/delta" => {
                let delta = params
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if delta.is_empty() {
                    return Vec::new();
                }
                let item = string_of(params, "itemId", 200);
                if !item.is_empty() && !self.streamed.contains(&item) {
                    self.streamed.push(item);
                    if self.streamed.len() > 16 {
                        self.streamed.remove(0);
                    }
                }
                vec![json!({"kind": "text", "text": cut(delta, MAX_TEXT_CHARS)})]
            }
            "item/started" => self.item_started(params.get("item").unwrap_or(&Value::Null)),
            "item/completed" => self.item_completed(params.get("item").unwrap_or(&Value::Null)),
            "turn/completed" => {
                let turn = params.get("turn").unwrap_or(&Value::Null);
                let status = string_of(turn, "status", 40);
                let mut text = turn.get("error").map(error_message).unwrap_or_default();
                if text.is_empty() && status == "failed" {
                    text = self.last_error.clone();
                }
                self.turn = None;
                self.turn_asked = false;
                self.interrupt = false;
                self.streamed.clear();
                self.tools.clear();
                let status = if status.is_empty() {
                    "completed".to_owned()
                } else {
                    status
                };
                let mut events = vec![self.turn_end(&status, status != "completed", text)];
                events.extend(self.next_turn());
                events
            }
            "error" => {
                let text = params.get("error").map(error_message).unwrap_or_default();
                let retry = params
                    .get("willRetry")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !retry {
                    self.last_error = text.clone();
                }
                let subtype = if retry { "retry" } else { "error" };
                vec![json!({"kind": "status", "subtype": subtype, "text": cut(&text, 500)})]
            }
            "mcpServer/startupStatus/updated" => self.bridge_status(params),
            // A setting Codex ignores or finds wrong, for the log.
            "configWarning" => {
                let mut text = string_of(params, "summary", 500);
                let details = string_of(params, "details", 500);
                if !details.is_empty() {
                    text = format!("{text}: {details}");
                }
                vec![json!({"kind": "status", "subtype": "config_warning",
                            "text": cut(&text, 500)})]
            }
            _ => Vec::new(),
        }
    }

    /// An MCP server's start (`mcpServer/startupStatus/updated`: `name`,
    /// `status` `starting`, `ready`, `failed` or `cancelled`, `error`):
    /// MitCad's bridge connected or failed. Another server's start is only
    /// said for the log: the thread should have no other.
    fn bridge_status(&self, params: &Value) -> Vec<Value> {
        let name = string_of(params, "name", 100);
        let status = string_of(params, "status", 40);
        if name != SERVER {
            return vec![json!({"kind": "status", "subtype": "other_mcp_server",
                               "text": format!("{name} {status}")})];
        }
        let bridge = match status.as_str() {
            "ready" => "connected",
            "starting" => return Vec::new(),
            _ => "failed",
        };
        let error = params
            .get("error")
            .map(error_message)
            .filter(|text| !text.is_empty());
        vec![json!({"kind": "bridge", "bridge": bridge,
                    "bridge_error": error.map(|text| cut(&text, 500))})]
    }

    fn item_started(&mut self, item: &Value) -> Vec<Value> {
        let id = string_of(item, "id", 200);
        let field = |name: &str| item.get(name).cloned().unwrap_or(Value::Null);
        let (name, server, input) =
            match item.get("type").and_then(Value::as_str).unwrap_or_default() {
                "mcpToolCall" => {
                    let input = match item.get("arguments") {
                        Some(Value::String(text)) => {
                            serde_json::from_str(text).unwrap_or(Value::Null)
                        }
                        Some(value) => value.clone(),
                        None => Value::Null,
                    };
                    (
                        string_of(item, "tool", 100),
                        string_of(item, "server", 100),
                        input,
                    )
                }
                "commandExecution" => (
                    "shell".to_owned(),
                    String::new(),
                    json!({"command": field("command")}),
                ),
                "fileChange" => {
                    let changes = item
                        .get("changes")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    (
                        "file_change".to_owned(),
                        String::new(),
                        json!({"changes": changes}),
                    )
                }
                "webSearch" => (
                    "web_search".to_owned(),
                    String::new(),
                    json!({"query": field("query")}),
                ),
                _ => return Vec::new(),
            };
        if id.is_empty() || self.tools.contains(&id) {
            return Vec::new();
        }
        self.tools.push(id.clone());
        vec![json!({"kind": "tool", "id": id, "name": name, "server": server, "input": input})]
    }

    fn item_completed(&mut self, item: &Value) -> Vec<Value> {
        let kind = item.get("type").and_then(Value::as_str).unwrap_or_default();
        let id = string_of(item, "id", 200);
        if kind == "agentMessage" {
            let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
            if text.is_empty() || self.streamed.contains(&id) {
                return Vec::new();
            }
            return vec![json!({"kind": "text", "text": cut(text, MAX_TEXT_CHARS)})];
        }
        if !matches!(
            kind,
            "mcpToolCall" | "commandExecution" | "fileChange" | "webSearch"
        ) {
            return Vec::new();
        }
        let mut events = self.item_started(item);
        let status = string_of(item, "status", 40);
        let error = item.get("error").filter(|error| !error.is_null());
        let result = item.get("result").filter(|result| !result.is_null());
        let failed = matches!(status.as_str(), "failed" | "declined")
            || error.is_some()
            || result
                .and_then(|result| result.get("isError"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let text = match (error, result) {
            (Some(error), _) => error_message(error),
            (None, Some(result)) => result_text(result),
            (None, None) => string_of(item, "aggregatedOutput", MAX_RESULT_CHARS),
        };
        events.push(json!({
            "kind": "tool_result",
            "id": id,
            "error": failed,
            "text": cut(&text, MAX_RESULT_CHARS),
        }));
        events
    }

    /// A request of Codex to the application: approvals and questions.
    /// MitCad's own tools are allowed here (the live server asks the user
    /// for the chat's connection); the agent's own commands, file changes
    /// and permissions are declined; anything else is not handled.
    fn server_request(&mut self, method: &str, id: &Value, params: &Value) -> Vec<Value> {
        let (result, verdict) = match method {
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                (json!({"decision": "decline"}), "declined")
            }
            // The protocol's first approvals (its v1 API): a refusal is
            // `{"denied": {"rejection"}}`, the turn goes on.
            "execCommandApproval" | "applyPatchApproval" => (
                json!({"decision": {"denied": {"rejection": REFUSAL}}}),
                "declined",
            ),
            "item/permissions/requestApproval" => (json!({"permissions": {}}), "declined"),
            "mcpServer/elicitation/request" => {
                let server = params.get("serverName").and_then(Value::as_str);
                let url = params.get("mode").and_then(Value::as_str) == Some("url");
                if server == Some(SERVER) && !url {
                    (json!({"action": "accept", "content": {}}), "allowed")
                } else {
                    (json!({"action": "decline", "content": null}), "declined")
                }
            }
            "item/tool/requestUserInput" => (json!({"answers": {}}), "declined"),
            _ => {
                let answer = write(json!({"id": id, "error": {
                    "code": -32601,
                    "message": format!("MitCad's chat does not handle {}", crate::echoed(method)),
                }}));
                return vec![
                    answer,
                    json!({"kind": "status", "subtype": "unhandled", "text": cut(method, 200)}),
                ];
            }
        };
        vec![
            write(json!({"id": id, "result": result})),
            json!({"kind": "status", "subtype": verdict, "text": cut(method, 200)}),
        ]
    }
}
