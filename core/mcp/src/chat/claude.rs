// SPDX-License-Identifier: MIT
//! Claude Code through its own print mode (mitcad#947): the command line of
//! one long-lived process per conversation, its stream-json input messages,
//! the version and sign-in checks, and its stream-json output turned into
//! panel events. Nothing here reads Claude Code's files or credentials:
//! `claude auth status` is the only sign-in check.

use serde_json::{Map, Value, json};

use super::{MAX_RESULT_CHARS, MAX_TEXT_CHARS, cut, pictures, text_field, valid_model};

/// The oldest Claude Code the chat starts: `--tools`, `--strict-mcp-config`,
/// `--include-partial-messages`, `--session-id`, `--resume` of a print-mode
/// session and `claude auth status` are all there.
pub const CLAUDE_MINIMUM_VERSION: &str = "2.1.0";

/// The live MCP server's name in the agent's configuration: its tools are
/// `mcp__mitcad__<tool>`.
const SERVER: &str = "mitcad";
const TOOL_PREFIX: &str = "mcp__mitcad__";

/// Identifiers the application passes on the command line: a UUID, the
/// listener's instance id, the chat's marker. Letters, digits and dashes
/// only, so that none can be read as an option.
fn plain_id(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && !value.starts_with('-')
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// `claude -p` with stream-json in and out, the live MCP bridge as the only
/// tool source and every built-in tool off.
pub fn arguments(request: &Value) -> Result<Value, String> {
    let session = text_field(request, "session");
    if !uuid(session) {
        return Err("the chat's session id must be a UUID".to_owned());
    }
    let bridge = text_field(request, "bridge");
    if bridge.is_empty() || bridge.contains('\0') {
        return Err("the chat needs the installed mitcad-cli bridge".to_owned());
    }
    let instance = text_field(request, "instance");
    let chat = text_field(request, "chat");
    if !plain_id(instance, 64) || !plain_id(chat, 64) {
        return Err("the chat needs the listener's instance id and its own marker".to_owned());
    }
    let config = json!({"mcpServers": {SERVER: {
        "command": bridge,
        "args": ["mcp", "--connect", "--instance", instance, "--chat-session", chat],
    }}})
    .to_string();
    let mut arguments: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();
    let resume = request
        .get("resume")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    arguments.push(if resume { "--resume" } else { "--session-id" }.to_owned());
    arguments.push(session.to_owned());
    for word in [
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        config.as_str(),
        "--allowedTools",
        "mcp__mitcad",
    ] {
        arguments.push(word.to_owned());
    }
    let model = text_field(request, "model");
    if !model.is_empty() {
        if !valid_model(model) {
            return Err("the model's name has characters a model name has not".to_owned());
        }
        arguments.push("--model".to_owned());
        arguments.push(model.to_owned());
    }
    Ok(json!({"arguments": arguments, "mcp_config": config}))
}

/// The model choices the panel offers for Claude Code: the aliases its
/// `--model` takes (any other model name can be typed).
pub fn models() -> Value {
    json!({"models": [
        {"id": "opus", "name": "Opus"},
        {"id": "sonnet", "name": "Sonnet"},
        {"id": "haiku", "name": "Haiku"},
    ]})
}

/// One user message for the process's stdin, as one line: the text, the
/// context the panel attaches (the selection) and pictures (the view).
pub fn message(request: &Value) -> Result<Value, String> {
    let text = text_field(request, "text").trim();
    if text.is_empty() {
        return Err("a message needs its text".to_owned());
    }
    let mut content = vec![json!({"type": "text", "text": cut(text, MAX_TEXT_CHARS)})];
    let context = text_field(request, "context").trim();
    if !context.is_empty() {
        content.push(json!({"type": "text", "text": cut(context, MAX_TEXT_CHARS)}));
    }
    for (media, data) in pictures(request)? {
        content.push(json!({"type": "image", "source": {
            "type": "base64", "media_type": media, "data": data}}));
    }
    let line = json!({
        "type": "user",
        "message": {"role": "user", "content": content},
        "parent_tool_use_id": null,
    })
    .to_string()
        + "\n";
    Ok(json!({"line": line}))
}

/// The version in `claude --version`'s output ("2.1.3 (Claude Code)") and
/// whether it is at least the minimum.
pub fn version(output: &str) -> Value {
    let found = output
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == 'v')
        .find_map(parse_version);
    match found {
        Some(parts) => json!({
            "version": format!("{}.{}.{}", parts[0], parts[1], parts[2]),
            "ok": parts >= parse_version(CLAUDE_MINIMUM_VERSION).unwrap_or_default(),
            "minimum": CLAUDE_MINIMUM_VERSION,
        }),
        None => json!({"version": null, "ok": false, "minimum": CLAUDE_MINIMUM_VERSION}),
    }
}

fn parse_version(word: &str) -> Option<[u64; 3]> {
    let mut parts = word.split('.');
    let mut numbers = [0u64; 3];
    for number in &mut numbers {
        *number = parts.next()?.parse().ok()?;
    }
    // "2.1.3-beta" is not a plain version: the next word may be.
    parts.next().is_none().then_some(numbers)
}

/// Signed in by `claude auth status`: exit status 0, unless its JSON says
/// `loggedIn` false. Only the method is read besides.
pub fn auth(exit_code: i64, output: &str) -> Value {
    let status = serde_json::from_str::<Value>(output.trim()).ok();
    let logged_in = status
        .as_ref()
        .and_then(|value| value.get("loggedIn"))
        .and_then(Value::as_bool);
    let method = status
        .as_ref()
        .and_then(|value| value.get("authMethod"))
        .and_then(Value::as_str)
        .map(|method| cut(method, 80));
    json!({"signed_in": exit_code == 0 && logged_in != Some(false), "method": method})
}

/// Claude Code's stream-json output as panel events. With partial messages
/// the answer's text comes as deltas and again whole in the assistant
/// message: a message whose deltas were seen gives no text twice.
#[derive(Default)]
pub struct ClaudeStream {
    // The message whose deltas are streaming, and the last ones that
    // streamed text.
    message: Option<String>,
    streamed: Vec<String>,
}

impl ClaudeStream {
    pub fn feed(&mut self, line: &str) -> Vec<Value> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Vec::new();
        }
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(line) else {
            return vec![json!({"kind": "output", "text": cut(line, 500)})];
        };
        match message
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "system" => self.system(&message),
            "stream_event" => self.stream_event(message.get("event").unwrap_or(&Value::Null)),
            "assistant" => self.assistant(message.get("message").unwrap_or(&Value::Null)),
            "user" => tool_results(message.get("message").unwrap_or(&Value::Null)),
            "result" => {
                self.message = None;
                self.streamed.clear();
                let subtype = string(&message, "subtype");
                let error = message
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || subtype != "success";
                vec![json!({
                    "kind": "turn_end",
                    "error": error,
                    "subtype": subtype,
                    "text": cut(&string(&message, "result"), MAX_TEXT_CHARS),
                    "session": string(&message, "session_id"),
                })]
            }
            _ => Vec::new(),
        }
    }

    fn system(&mut self, message: &Map<String, Value>) -> Vec<Value> {
        let subtype = string(message, "subtype");
        if subtype != "init" {
            return vec![json!({"kind": "status", "subtype": subtype,
                               "text": cut(&string(message, "message"), 500)})];
        }
        let server = message
            .get("mcp_servers")
            .and_then(Value::as_array)
            .and_then(|servers| {
                servers
                    .iter()
                    .find(|server| server.get("name").and_then(Value::as_str) == Some(SERVER))
            });
        let bridge = match server
            .and_then(|server| server.get("status"))
            .and_then(Value::as_str)
        {
            None if server.is_none() => "missing",
            Some("connected") => "connected",
            Some("pending") => "pending",
            _ => "failed",
        };
        let mut error = server
            .and_then(|server| server.get("error"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        // The errors of the servers that did not start: an array of
        // {name or server, error or message}, or an object by name.
        match message.get("mcp_server_errors") {
            Some(Value::Array(errors)) => {
                for entry in errors {
                    let name = entry
                        .get("name")
                        .or_else(|| entry.get("server"))
                        .and_then(Value::as_str);
                    if name == Some(SERVER) {
                        error = entry
                            .get("error")
                            .or_else(|| entry.get("message"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or(error);
                    }
                }
            }
            Some(Value::Object(errors)) => {
                if let Some(text) = errors.get(SERVER) {
                    error = Some(
                        text.as_str()
                            .map_or_else(|| text.to_string(), str::to_owned),
                    );
                }
            }
            _ => {}
        }
        vec![json!({
            "kind": "init",
            "session": string(message, "session_id"),
            "model": cut(&string(message, "model"), 100),
            "bridge": bridge,
            "bridge_error": error.map(|text| cut(&text, 500)),
        })]
    }

    fn stream_event(&mut self, event: &Value) -> Vec<Value> {
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "message_start" => {
                self.message = event
                    .get("message")
                    .and_then(|message| message.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Vec::new()
            }
            "content_block_start" => {
                let block = event.get("content_block").unwrap_or(&Value::Null);
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    return Vec::new();
                }
                let (name, _) = tool_name(
                    block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
                vec![json!({"kind": "tool_start", "id": string_of(block, "id"), "name": name})]
            }
            "content_block_delta" => {
                let delta = event.get("delta").unwrap_or(&Value::Null);
                if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
                    return Vec::new();
                }
                let text = delta
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if text.is_empty() {
                    return Vec::new();
                }
                if let Some(id) = &self.message
                    && !self.streamed.contains(id)
                {
                    self.streamed.push(id.clone());
                    if self.streamed.len() > 16 {
                        self.streamed.remove(0);
                    }
                }
                vec![json!({"kind": "text", "text": cut(text, MAX_TEXT_CHARS)})]
            }
            _ => Vec::new(),
        }
    }

    fn assistant(&mut self, message: &Value) -> Vec<Value> {
        let id = message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let streamed = !id.is_empty() && self.streamed.iter().any(|seen| seen == id);
        let mut events = Vec::new();
        for block in message
            .get("content")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            match block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "text" if !streamed => {
                    let text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !text.is_empty() {
                        events.push(json!({"kind": "text", "text": cut(text, MAX_TEXT_CHARS)}));
                    }
                }
                "tool_use" => {
                    let (name, server) = tool_name(
                        block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
                    events.push(json!({
                        "kind": "tool",
                        "id": string_of(block, "id"),
                        "name": name,
                        "server": server,
                        "input": block.get("input").cloned().unwrap_or(Value::Null),
                    }));
                }
                _ => {}
            }
        }
        events
    }
}

/// A tool's name as the panel shows it ("model_command"), and its server
/// ("mitcad"; empty for a built-in tool).
fn tool_name(name: &str) -> (String, String) {
    if let Some(rest) = name.strip_prefix(TOOL_PREFIX) {
        return (cut(rest, 100), SERVER.to_owned());
    }
    if let Some(rest) = name.strip_prefix("mcp__")
        && let Some((server, tool)) = rest.split_once("__")
    {
        return (cut(tool, 100), cut(server, 100));
    }
    (cut(name, 100), String::new())
}

/// The tool results of a user message the agent writes back.
fn tool_results(message: &Value) -> Vec<Value> {
    let mut events = Vec::new();
    for block in message
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let text = match block.get("content") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(parts)) => parts
                .iter()
                .map(|part| match part.get("type").and_then(Value::as_str) {
                    Some("text") => part
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    Some("image") => "[image]".to_owned(),
                    _ => String::new(),
                })
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };
        events.push(json!({
            "kind": "tool_result",
            "id": string_of(block, "tool_use_id"),
            "error": block.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            "text": cut(&text, MAX_RESULT_CHARS),
        }));
    }
    events
}

fn string(object: &Map<String, Value>, name: &str) -> String {
    object
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn string_of(value: &Value, name: &str) -> String {
    cut(
        value.get(name).and_then(Value::as_str).unwrap_or_default(),
        200,
    )
}
