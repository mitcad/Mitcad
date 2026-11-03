// SPDX-License-Identifier: MIT
//! A custom command that speaks the Agent Client Protocol itself
//! (mitcad#1198): one long-lived process per conversation, JSON-RPC 2.0
//! over stdio, one message per line, as the protocol's published schema
//! (agentclientprotocol.com, protocol version 1) defines it: `initialize`,
//! `session/new` or `session/load`, `session/prompt` per message,
//! `session/cancel` for Stop, and the agent's `session/update`
//! notifications until the prompt's answer names its stop reason.
//!
//! MitCad starts the command the user set in Preferences (no adapter is
//! shipped or installed) and gives it the window's bridge as the session's
//! only MCP server (`mcpServers` of `session/new` and `session/load`), so
//! that the live server's approvals for the chat's connection apply as for
//! Claude Code and Codex. MitCad offers the agent no file system and no
//! terminal (`clientCapabilities`), and answers its permission requests
//! itself: a call of MitCad's own tools is allowed once (the live server
//! decides and asks the user), every other is rejected. A request MitCad
//! does not handle (`fs/...`, `terminal/...`) is answered with an error.
//!
//! The session's model is the agent's model selector, when it has one (a
//! `configOptions` entry of the category `model`): the models it lists
//! become the panel's choice, and a chosen one is set with
//! `session/set_config_option` before the first prompt.
//!
//! The stream is driven from both sides as Codex's ([`AcpStream::control`]
//! and [`AcpStream::feed`]), with the same `write` and `end` events. The
//! handshake alone (`acp_handshake`, `acp_handshake_answer`) is the setup
//! check: a command that does not answer it is not an agent of this kind.
//! Everything of the protocol is in this one file.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::{MAX_RESULT_CHARS, MAX_TEXT_CHARS, cut, pictures, text_field, valid_model};

/// The version of the Agent Client Protocol MitCad speaks.
pub const ACP_PROTOCOL_VERSION: u64 = 1;

/// The live MCP server's name in the session's `mcpServers`.
const SERVER: &str = "mitcad";
/// The most arguments a custom command is given.
const MAX_ARGUMENTS: usize = 64;

/// What a request of ours was, to read its answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Initialize,
    NewSession,
    LoadSession,
    Prompt,
    SetModel,
}

/// A tool call shown in the panel: its name and server, whether its input
/// and its result were shown.
#[derive(Clone, Debug, Default)]
struct Shown {
    name: String,
    server: String,
    input: bool,
    done: bool,
}

#[derive(Default)]
pub struct AcpStream {
    next: u64,
    asked: HashMap<u64, Asked>,
    // The session's settings, kept from `start` for `session/new` (also
    // after a failed load).
    servers: Value,
    cwd: String,
    model: String,
    resume: Option<String>,
    resume_failed: bool,
    // What the agent said it can (initialize).
    load_session: bool,
    images: bool,
    session: Option<String>,
    // A loaded session's replay runs: its updates are MitCad's history.
    loading: bool,
    // The chosen model is being set: prompts wait for it.
    configuring: bool,
    model_option: Option<String>,
    current_model: String,
    // Messages given before the session was there, as prompt blocks.
    waiting: Vec<Vec<Value>>,
    // The running prompt's request id; whether it was cancelled.
    prompt: Option<u64>,
    cancelled: bool,
    tools: HashMap<String, Shown>,
}

fn write(message: Value) -> Value {
    json!({"kind": "write", "line": message.to_string() + "\n"})
}

fn status(subtype: &str, text: &str) -> Value {
    json!({"kind": "status", "subtype": subtype, "text": cut(text, 500)})
}

fn end(text: &str) -> Value {
    json!({"kind": "end", "text": cut(text, MAX_TEXT_CHARS)})
}

fn string_of(value: &Value, name: &str, limit: usize) -> String {
    cut(
        value.get(name).and_then(Value::as_str).unwrap_or_default(),
        limit,
    )
}

fn plain_id(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A session id an agent chose, as the history keeps it: letters, digits
/// and `- _ . :`.
fn session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// An error's message: `{message}` or a plain string.
fn error_message(error: &Value) -> String {
    match error {
        Value::String(text) => cut(text, MAX_TEXT_CHARS),
        Value::Object(_) => string_of(error, "message", MAX_TEXT_CHARS),
        _ => String::new(),
    }
}

/// Why an `initialize` answer's protocol version is not MitCad's.
fn version_mismatch(version: Option<u64>) -> String {
    match version {
        Some(version) => format!(
            "it answered with protocol version {version}; MitCad speaks version {ACP_PROTOCOL_VERSION}"
        ),
        None => "its answer named no protocol version".to_owned(),
    }
}

/// `initialize`'s params: the protocol's version, no file system and no
/// terminal of the client's, and MitCad's name.
fn initialize_params() -> Value {
    json!({
        "protocolVersion": ACP_PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false,
        },
        "clientInfo": {"name": "mitcad", "title": "MitCad", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// The setup check's first line: `initialize` with the id 0.
pub fn handshake() -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
                         "params": initialize_params()});
    json!({"line": request.to_string() + "\n"})
}

/// What a command wrote after the handshake's line (its whole lines):
/// `state` `waiting` (no answer yet), `ok` (`agent`: its name and version) or `failed`
/// (`reason`). A line that is not JSON fails it: the protocol's output
/// holds its messages only.
pub fn handshake_answer(output: &str) -> Value {
    // Only whole lines: the rest is still being written.
    let complete = output.rfind('\n').map_or("", |end| &output[..end]);
    for line in complete
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(line) else {
            return json!({"state": "failed",
                          "reason": format!("it wrote “{}” instead", cut(line, 80))});
        };
        if message.get("method").is_some() || message.get("id").and_then(Value::as_u64) != Some(0) {
            continue;
        }
        if let Some(error) = message.get("error").filter(|error| !error.is_null()) {
            return json!({"state": "failed",
                          "reason": format!("it refused: {}", cut(&error_message(error), 200))});
        }
        let result = message.get("result").unwrap_or(&Value::Null);
        let version = result.get("protocolVersion").and_then(Value::as_u64);
        if version != Some(ACP_PROTOCOL_VERSION) {
            return json!({"state": "failed", "reason": version_mismatch(version)});
        }
        let info = result.get("agentInfo").unwrap_or(&Value::Null);
        let mut agent = string_of(info, "title", 100);
        if agent.is_empty() {
            agent = string_of(info, "name", 100);
        }
        let version = string_of(info, "version", 40);
        if !version.is_empty() {
            agent = if agent.is_empty() {
                version
            } else {
                format!("{agent} {version}")
            };
        }
        return json!({"state": "ok", "agent": agent});
    }
    json!({"state": "waiting"})
}

/// The words of a command's arguments as the user typed them: separated by
/// white space, grouped by double or single quotes; a backslash only
/// escapes a double quote or a backslash inside double quotes (a Windows
/// path keeps its backslashes).
fn words(text: &str) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    word.push(c);
                }
            }
            Some(_) => match c {
                '"' => quote = None,
                '\\' if matches!(chars.peek(), Some('"' | '\\')) => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                    }
                }
                _ => word.push(c),
            },
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                _ if c.is_whitespace() => {
                    if started {
                        found.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                _ => {
                    word.push(c);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err("a quote in the arguments is not closed".to_owned());
    }
    if started {
        found.push(word);
    }
    Ok(found)
}

/// `acp_command`: the custom command (`command`, the program) and its
/// `arguments` (one line) → `program`, `arguments` (the words).
pub fn command(request: &Value) -> Result<Value, String> {
    let program = text_field(request, "command").trim();
    if program.is_empty() {
        return Err("no command is set for the custom agent".to_owned());
    }
    let arguments = text_field(request, "arguments");
    if program.contains('\0') || arguments.contains('\0') {
        return Err("the custom command has a character a command line cannot have".to_owned());
    }
    let found = words(arguments)?;
    if found.len() > MAX_ARGUMENTS {
        return Err(format!(
            "the custom command has more than {MAX_ARGUMENTS} arguments"
        ));
    }
    Ok(json!({"program": program, "arguments": found}))
}

/// Whether `name` is one of the live server's tools.
fn mitcad_tool(name: &str) -> bool {
    crate::catalog::live_tools()
        .iter()
        .any(|tool| tool["name"].as_str() == Some(name))
}

/// A tool call's name for the panel and its MCP server: its `name`, else
/// its `title`, without a prefix naming MitCad's server (`mcp__mitcad__`,
/// `mitcad__`, `mitcad/`, `mitcad.`, `mitcad:`); a bare name of one of
/// MitCad's live tools is MitCad's too.
fn tool_name(call: &Value) -> (String, String) {
    let given = [call.get("name"), call.get("title")]
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .find(|text| !text.is_empty())
        .unwrap_or_default();
    let given = cut(given, 200);
    for prefix in ["mcp__mitcad__", "mitcad__", "mitcad/", "mitcad.", "mitcad:"] {
        if let Some(rest) = given.strip_prefix(prefix) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return (cut(rest, 100), SERVER.to_owned());
            }
        }
    }
    let server = if mitcad_tool(&given) { SERVER } else { "" };
    (cut(&given, 100), server.to_owned())
}

/// The text of content blocks (MCP's or the protocol's): text, a picture
/// as `[image]`.
fn blocks_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => string_of(block, "text", MAX_RESULT_CHARS),
            Some("image") => "[image]".to_owned(),
            _ => String::new(),
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A tool call's result as text: its `content` (a content block, a diff's
/// path, a terminal), else its `rawOutput` (an MCP result's content or
/// structured content, a string, other JSON as it is).
fn result_text(update: &Value) -> String {
    let mut parts = Vec::new();
    for item in update
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let part = match item.get("type").and_then(Value::as_str) {
            Some("content") => blocks_text(std::slice::from_ref(
                item.get("content").unwrap_or(&Value::Null),
            )),
            Some("diff") => format!("{} changed", string_of(item, "path", 500)),
            Some("terminal") => "[terminal]".to_owned(),
            _ => String::new(),
        };
        if !part.is_empty() {
            parts.push(part);
        }
    }
    if !parts.is_empty() {
        return parts.join("\n");
    }
    match update.get("rawOutput") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => cut(text, MAX_RESULT_CHARS),
        Some(output) => {
            let text = blocks_text(
                output
                    .get("content")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            );
            if !text.is_empty() {
                return text;
            }
            match output.get("structuredContent") {
                Some(Value::Null) | None => cut(&output.to_string(), MAX_RESULT_CHARS),
                Some(structured) => cut(&structured.to_string(), MAX_RESULT_CHARS),
            }
        }
    }
}

impl AcpStream {
    fn request(&mut self, asked: Asked, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.asked.insert(id, asked);
        write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    /// A control of the application: `start` (`session` to resume or
    /// empty, `resume`, `bridge`, `instance`, `chat`, `model`, `cwd`),
    /// `message` (`text`, `context`, `images`) or `interrupt`.
    pub fn control(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        match text_field(request, "op") {
            "start" => self.start(request),
            "message" => self.message(request),
            "interrupt" => Ok(self.interrupt()),
            other => Err(format!(
                "unknown control “{}” of the custom agent",
                crate::echoed(other)
            )),
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
        if resume && !session_id(session) {
            return Err("the session to resume needs its id".to_owned());
        }
        let cwd = text_field(request, "cwd");
        if cwd.is_empty() || cwd.contains('\0') {
            return Err("the chat's working folder is not a path".to_owned());
        }
        *self = AcpStream {
            servers: json!([{
                "name": SERVER,
                "command": bridge,
                "args": ["mcp", "--connect", "--instance", instance, "--chat-session", chat],
                "env": [],
            }]),
            cwd: cwd.to_owned(),
            model: model.to_owned(),
            resume: resume.then(|| session.to_owned()),
            ..AcpStream::default()
        };
        Ok(vec![self.request(
            Asked::Initialize,
            "initialize",
            initialize_params(),
        )])
    }

    fn session_params(&self) -> Value {
        json!({"cwd": self.cwd, "mcpServers": self.servers})
    }

    fn new_session(&mut self) -> Value {
        let params = self.session_params();
        self.request(Asked::NewSession, "session/new", params)
    }

    fn message(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        let text = text_field(request, "text").trim();
        if text.is_empty() {
            return Err("a message needs its text".to_owned());
        }
        let mut prompt = vec![json!({"type": "text", "text": cut(text, MAX_TEXT_CHARS)})];
        let context = text_field(request, "context").trim();
        if !context.is_empty() {
            prompt.push(json!({"type": "text", "text": cut(context, MAX_TEXT_CHARS)}));
        }
        for (media, data) in pictures(request)? {
            prompt.push(json!({"type": "image", "mimeType": media, "data": data}));
        }
        self.waiting.push(prompt);
        Ok(self.next_prompt())
    }

    /// The next waiting message as a prompt, once the session is there, its
    /// model set and no prompt runs. Pictures go only to an agent that
    /// takes them.
    fn next_prompt(&mut self) -> Vec<Value> {
        let Some(session) = self.session.clone() else {
            return Vec::new();
        };
        if self.loading || self.configuring || self.prompt.is_some() || self.waiting.is_empty() {
            return Vec::new();
        }
        let mut prompt = self.waiting.remove(0);
        let mut events = Vec::new();
        if !self.images {
            let before = prompt.len();
            prompt.retain(|block| block["type"] != "image");
            if prompt.len() != before {
                events.push(status(
                    "pictures_left_out",
                    "the agent takes no pictures; they were left out",
                ));
            }
        }
        self.prompt = Some(self.next);
        self.cancelled = false;
        events.push(self.request(
            Asked::Prompt,
            "session/prompt",
            json!({"sessionId": session, "prompt": prompt}),
        ));
        events
    }

    fn interrupt(&mut self) -> Vec<Value> {
        if let (Some(session), Some(_)) = (self.session.clone(), self.prompt) {
            if self.cancelled {
                return Vec::new();
            }
            self.cancelled = true;
            // A notification: the prompt's answer says `cancelled`.
            return vec![write(json!({"jsonrpc": "2.0", "method": "session/cancel",
                                     "params": {"sessionId": session}}))];
        }
        if !self.waiting.is_empty() {
            // Not given to the agent yet: it ends here.
            self.waiting.clear();
            return vec![self.turn_end("cancelled", true, String::new())];
        }
        Vec::new()
    }

    fn turn_end(&self, subtype: &str, error: bool, text: String) -> Value {
        json!({
            "kind": "turn_end",
            "error": error,
            "subtype": subtype,
            "text": text,
            "session": self.session.clone().unwrap_or_default(),
        })
    }

    /// The panel events of one line the agent wrote (without its newline).
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
            (Some(method), Some(id)) => self.agent_request(method, id, params),
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
        if let Some(error) = message.get("error").filter(|error| !error.is_null()) {
            return self.refused(asked, &error_message(error));
        }
        match asked {
            Asked::Initialize => self.initialized(result),
            Asked::NewSession => self.session_started(result, false),
            Asked::LoadSession => self.session_started(result, true),
            Asked::Prompt => self.prompt_ended(result),
            Asked::SetModel => {
                self.configuring = false;
                let mut events = self.config_options(result);
                events.extend(self.next_prompt());
                events
            }
        }
    }

    fn initialized(&mut self, result: &Value) -> Vec<Value> {
        let version = result.get("protocolVersion").and_then(Value::as_u64);
        if version != Some(ACP_PROTOCOL_VERSION) {
            return vec![end(&format!(
                "The custom agent did not start: {}.",
                version_mismatch(version)
            ))];
        }
        let capabilities = result.get("agentCapabilities").unwrap_or(&Value::Null);
        self.load_session = capabilities
            .get("loadSession")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.images = capabilities
            .pointer("/promptCapabilities/image")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match self.resume.clone() {
            Some(session) if self.load_session => {
                self.loading = true;
                let mut params = self.session_params();
                params["sessionId"] = json!(session);
                vec![self.request(Asked::LoadSession, "session/load", params)]
            }
            Some(_) => {
                // It cannot go on with a session: a new one, and the panel
                // says so.
                self.resume = None;
                self.resume_failed = true;
                vec![
                    status("resume_failed", "the agent cannot load an earlier session"),
                    self.new_session(),
                ]
            }
            None => vec![self.new_session()],
        }
    }

    fn session_started(&mut self, result: &Value, loaded: bool) -> Vec<Value> {
        self.loading = false;
        let id = if loaded {
            self.resume.clone().unwrap_or_default()
        } else {
            string_of(result, "sessionId", 201)
        };
        if !session_id(&id) {
            return vec![end("The custom agent started no session (no session id).")];
        }
        self.session = Some(id.clone());
        let models = self.config_options(result);
        // The chosen model when the selector is to set it, else the
        // session's own.
        let model = if self.current_model.is_empty()
            || (!self.model.is_empty() && self.model_option.is_some())
        {
            self.model.clone()
        } else {
            self.current_model.clone()
        };
        let mut events = vec![json!({
            "kind": "init",
            "session": id,
            "model": model,
            "bridge": "pending",
            "bridge_error": null,
            "resume_failed": self.resume_failed,
        })];
        events.extend(models);
        events.extend(self.choose_model());
        events.extend(self.next_prompt());
        events
    }

    /// The agent's model selector (`configOptions`, the category `model`):
    /// its models for the panel; none when it has no selector.
    fn config_options(&mut self, result: &Value) -> Vec<Value> {
        let Some(options) = result.get("configOptions").and_then(Value::as_array) else {
            return Vec::new();
        };
        let Some(selector) = options.iter().find(|option| {
            option.get("category").and_then(Value::as_str) == Some("model")
                && option.get("type").and_then(Value::as_str) == Some("select")
        }) else {
            return Vec::new();
        };
        let id = string_of(selector, "id", 100);
        if id.is_empty() {
            return Vec::new();
        }
        self.model_option = Some(id);
        self.current_model = string_of(selector, "currentValue", 100);
        let mut models = Vec::new();
        for entry in selector
            .get("options")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            // Ungrouped options, or groups of them.
            let grouped = entry.get("options").and_then(Value::as_array);
            for option in grouped.map_or(std::slice::from_ref(entry), Vec::as_slice) {
                let value = option
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !valid_model(value) || models.len() >= 50 {
                    continue;
                }
                let name = option.get("name").and_then(Value::as_str).unwrap_or(value);
                models.push(json!({"id": value, "name": cut(name, 100),
                                   "default": value == self.current_model}));
            }
        }
        vec![json!({"kind": "models", "models": models})]
    }

    /// The chosen model set through the agent's model selector before the
    /// first prompt; without a selector it is only said for the log.
    fn choose_model(&mut self) -> Vec<Value> {
        if self.model.is_empty() || self.model == self.current_model {
            return Vec::new();
        }
        let (Some(option), Some(session)) = (self.model_option.clone(), self.session.clone())
        else {
            return vec![status(
                "model_unsupported",
                "the agent has no model selector; it uses its own model",
            )];
        };
        self.configuring = true;
        let params = json!({"sessionId": session, "configId": option, "value": self.model});
        vec![self.request(Asked::SetModel, "session/set_config_option", params)]
    }

    fn prompt_ended(&mut self, result: &Value) -> Vec<Value> {
        self.prompt = None;
        self.cancelled = false;
        self.tools.clear();
        let reason = string_of(result, "stopReason", 40);
        let (error, text) = match reason.as_str() {
            "" | "end_turn" => (false, ""),
            "cancelled" => (true, ""),
            "max_tokens" => (true, "The agent reached its output limit."),
            "max_turn_requests" => (
                true,
                "The agent reached its limit of model requests in one message.",
            ),
            "refusal" => (true, "The agent refused the message."),
            // A stop reason of a later version of the protocol.
            _ => (false, ""),
        };
        let subtype = if reason.is_empty() {
            "end_turn".to_owned()
        } else {
            reason
        };
        let mut events = vec![self.turn_end(&subtype, error, text.to_owned())];
        events.extend(self.next_prompt());
        events
    }

    fn refused(&mut self, asked: Asked, message: &str) -> Vec<Value> {
        match asked {
            Asked::Initialize => vec![end(&format!(
                "The custom agent refused to start: {message}"
            ))],
            Asked::LoadSession => {
                // The session is gone: a new one, and the panel says so.
                self.loading = false;
                self.resume = None;
                self.resume_failed = true;
                vec![status("resume_failed", message), self.new_session()]
            }
            Asked::NewSession => vec![end(&format!(
                "The custom agent did not start a session: {message}"
            ))],
            Asked::Prompt => {
                self.prompt = None;
                self.cancelled = false;
                self.tools.clear();
                let mut events = vec![self.turn_end("error", true, cut(message, MAX_TEXT_CHARS))];
                events.extend(self.next_prompt());
                events
            }
            Asked::SetModel => {
                self.configuring = false;
                let mut events = vec![status("model_failed", message)];
                events.extend(self.next_prompt());
                events
            }
        }
    }

    fn notification(&mut self, method: &str, params: &Value) -> Vec<Value> {
        if method != "session/update" || self.loading {
            // A loaded session's replay: MitCad shows its own history.
            return Vec::new();
        }
        if self.session.is_none()
            || params.get("sessionId").and_then(Value::as_str) != self.session.as_deref()
        {
            return Vec::new();
        }
        let update = params.get("update").unwrap_or(&Value::Null);
        match update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "agent_message_chunk" => {
                let content = update.get("content").unwrap_or(&Value::Null);
                let text = content
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if content.get("type").and_then(Value::as_str) != Some("text") || text.is_empty() {
                    return Vec::new();
                }
                vec![json!({"kind": "text", "text": cut(text, MAX_TEXT_CHARS)})]
            }
            "tool_call" | "tool_call_update" => self.tool(update),
            "config_option_update" => self.config_options(update),
            _ => Vec::new(),
        }
    }

    /// A tool call (`tool_call`) or a change of it (`tool_call_update`):
    /// shown once with its input (again when its input comes later), its
    /// result once it completed or failed.
    fn tool(&mut self, update: &Value) -> Vec<Value> {
        let id = string_of(update, "toolCallId", 200);
        if id.is_empty() {
            return Vec::new();
        }
        let input = update
            .get("rawInput")
            .filter(|input| !input.is_null())
            .cloned();
        let mut events = Vec::new();
        match self.tools.get(&id).cloned() {
            None => {
                let (name, server) = tool_name(update);
                if name.is_empty() {
                    // A change of a call never shown, without its title.
                    return Vec::new();
                }
                self.tools.insert(
                    id.clone(),
                    Shown {
                        name: name.clone(),
                        server: server.clone(),
                        input: input.is_some(),
                        done: false,
                    },
                );
                events.push(
                    json!({"kind": "tool", "id": id, "name": name, "server": server,
                                   "input": input.unwrap_or_else(|| json!({}))}),
                );
            }
            Some(shown) if !shown.input && input.is_some() => {
                if let Some(entry) = self.tools.get_mut(&id) {
                    entry.input = true;
                }
                events.push(json!({"kind": "tool", "id": id, "name": shown.name,
                                   "server": shown.server, "input": input}));
            }
            Some(_) => {}
        }
        let state = string_of(update, "status", 40);
        let finished = matches!(state.as_str(), "completed" | "failed");
        if let Some(entry) = self
            .tools
            .get_mut(&id)
            .filter(|entry| finished && !entry.done)
        {
            entry.done = true;
            let failed = state == "failed"
                || update
                    .pointer("/rawOutput/isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            events.push(json!({
                "kind": "tool_result",
                "id": id,
                "error": failed,
                "text": cut(&result_text(update), MAX_RESULT_CHARS),
            }));
        }
        events
    }

    /// A request of the agent to MitCad: a permission is answered here (a
    /// call of MitCad's own tools allowed once, as the live server asks the
    /// user itself; every other rejected; a cancelled prompt's cancelled);
    /// anything else is not handled (no file system, no terminal).
    fn agent_request(&mut self, method: &str, id: &Value, params: &Value) -> Vec<Value> {
        if method != "session/request_permission" {
            let answer = write(json!({"jsonrpc": "2.0", "id": id, "error": {
                "code": -32601,
                "message": format!("MitCad's chat does not handle {}", crate::echoed(method)),
            }}));
            return vec![answer, status("unhandled", method)];
        }
        let call = params.get("toolCall").unwrap_or(&Value::Null);
        let call_id = string_of(call, "toolCallId", 200);
        let (name, server) = match self.tools.get(&call_id) {
            Some(shown) => (shown.name.clone(), shown.server.clone()),
            None => tool_name(call),
        };
        let mitcad = server == SERVER;
        let kinds: [&str; 2] = if mitcad {
            ["allow_once", "allow_always"]
        } else {
            ["reject_once", "reject_always"]
        };
        let options = params
            .get("options")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let chosen = kinds.iter().find_map(|kind| {
            options
                .iter()
                .find(|option| option.get("kind").and_then(Value::as_str) == Some(*kind))
                .and_then(|option| option.get("optionId").cloned())
        });
        let verdict = if self.cancelled {
            "cancelled"
        } else if mitcad && chosen.is_some() {
            "allowed"
        } else {
            "declined"
        };
        let outcome = match chosen {
            Some(option) if !self.cancelled => json!({"outcome": "selected", "optionId": option}),
            _ => json!({"outcome": "cancelled"}),
        };
        vec![
            write(json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": outcome}})),
            status(verdict, &name),
        ]
    }
}
