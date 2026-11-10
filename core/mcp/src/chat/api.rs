// SPDX-License-Identifier: MIT
//! API endpoints and local models in the chat panel (mitcad#947, phase 2):
//! MitCad runs the agent's tool loop itself, in process, against the
//! Anthropic Messages API or an OpenAI-compatible chat completions endpoint
//! (Ollama, LM Studio, a custom address), with the live tool catalog of the
//! window and no bridge process.
//!
//! The stream speaks two protocols at once, without IO of its own:
//!
//! - **MCP to MitCad's own listener.** The application opens the chat's
//!   connection to the live server (its token and the chat's marker, as the
//!   bridge would) and gives the stream every line the server writes
//!   ([`ApiStream::feed`]); the stream answers with `mcp` events, the lines
//!   to write: `initialize`, `notifications/initialized`, `tools/list` (the
//!   catalog the model is given, with the server's instructions as the
//!   system prompt) and one `tools/call` per call of the model. The calls go
//!   through the live server's approvals for the chat's connection, exactly
//!   as a bridge's would.
//! - **HTTP to the model's service.** An `http` event is one streamed
//!   request (`id`, `method`, `url`, `headers`, `auth`, `body`); the
//!   application adds the key the `auth` names (`x-api-key` or `bearer`;
//!   the key never comes here), sends it and gives back each line of the
//!   response (`http_line`: `id`, `status`, `line`) and its end (`http_end`:
//!   `id`, `status`, `error`). `http_abort` ends a request (Stop).
//!
//! The answer streams to the panel as the usual events (`text`,
//! `tool_start`, `tool`, `tool_result`, `turn_end`, `account_error`,
//! `status`); `transcript` carries the conversation after each message, so
//! that the application keeps it and a reopened design goes on with it
//! (`start` with `resume` and `transcript`); `end` means the conversation
//! cannot go on.
//!
//! Everything of the two services' protocols is in this one file.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::{MAX_RESULT_CHARS, MAX_TEXT_CHARS, account, cut, pictures, text_field, valid_model};

/// The Anthropic Messages API's address and version.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2026-10-21";
/// The model of the Anthropic API when none is chosen.
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";
/// The models the panel offers for the Anthropic API before its list came.
const ANTHROPIC_MODELS: [(&str, &str); 3] = [
    ("claude-opus-5-5", "Claude Opus 5.5"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
    ("claude-haiku-5-5", "Claude Haiku 5.5"),
];
/// The output limit of one answer (streamed, so no HTTP timeout cuts it).
const MAX_TOKENS: u64 = 32000;
/// Model requests in one message at most: a model that keeps calling
/// tools is stopped there.
const MAX_ROUNDS: u32 = 100;
/// The text of one tool result the model is given at most.
const MAX_TOOL_TEXT: usize = 100_000;
/// An error response's body read at most.
const MAX_ERROR_BODY: usize = 64 * 1024;
/// One server-sent event's data at most.
const MAX_EVENT_DATA: usize = 16 * 1024 * 1024;
/// A transcript kept for resuming at most.
const MAX_TRANSCRIPT: usize = 16 * 1024 * 1024;
const TRANSCRIPT_FORMAT: &str = "mitcad-chat-api";
/// What the model is told before the live server's own instructions.
const PREFACE: &str = "You are the assistant in the chat panel of MitCad, a parametric CAD application. The user's messages ask about or for changes to the design open in MitCad's window. Make the changes with MitCad's tools (the only tools you have), check the result, and answer briefly in the user's language. JSON numbers in MitCad's commands are millimetres and radians in every design; a length the user gives without a unit is in the design's length unit (the document query's units.length), so convert it, or give a value as text, which is read in that unit.";
/// Why a call ended without its result.
const STOPPED: &str = "The user stopped the message before this call finished.";
/// Why a call shown as it streamed was not made (mitcad#1317).
const NOT_MADE_STOPPED: &str = "The user stopped the message before this call was made.";
const NOT_MADE_CUT: &str =
    "The answer was cut at the model's output limit before this call was made.";
const NOT_MADE_FAILED: &str = "The answer ended before this call was made.";
const NOT_MADE_REFUSED: &str = "The model declined this request; the call was not made.";

/// The kind of service an agent speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAi,
}

/// The agents of this kind: `(id, name, preset address, key, page)`; the
/// key `required`, `optional` or `none`; the page is where the service is
/// set up (an empty address: the user gives it).
const PRESETS: [(&str, &str, &str, &str, &str); 4] = [
    (
        "anthropic",
        "Anthropic API",
        ANTHROPIC_URL,
        "required",
        "https://platform.claude.com/settings/keys",
    ),
    (
        "ollama",
        "Ollama",
        "http://localhost:11434/v1",
        "none",
        "https://ollama.com/download",
    ),
    (
        "lmstudio",
        "LM Studio",
        "http://localhost:1234/v1",
        "none",
        "https://lmstudio.ai/download",
    ),
    ("openai", "Custom endpoint", "", "optional", ""),
];

/// The provider of an agent id; `None` for an agent that is no API agent.
pub fn provider(agent: &str) -> Option<Provider> {
    match agent {
        "anthropic" => Some(Provider::Anthropic),
        "ollama" | "lmstudio" | "openai" => Some(Provider::OpenAi),
        _ => None,
    }
}

/// `api_presets`: the API agents, each `id`, `name`, `url` (the preset
/// address), `key` and `page`.
pub fn presets() -> Value {
    let agents: Vec<Value> = PRESETS
        .iter()
        .map(|(id, name, url, key, page)| {
            json!({"id": id, "name": name, "url": url, "key": key, "page": page})
        })
        .collect();
    json!({"agents": agents})
}

/// An endpoint's address, checked: `https`, or `http` only to this computer
/// (`localhost`, `127.0.0.1`, `::1`), so that a key never goes over plain
/// HTTP to another host. No user name or password in it, no query; the
/// trailing slash is dropped.
pub fn endpoint(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("the endpoint has no address".to_owned());
    }
    if url.len() > 2000 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("the endpoint's address is not a web address".to_owned());
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err("the endpoint's address needs https:// or http://".to_owned());
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err("the endpoint's address needs https:// or http://".to_owned());
    }
    if rest.contains(['?', '#']) {
        return Err("the endpoint's address cannot have a query".to_owned());
    }
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err("the endpoint's address has no host".to_owned());
    }
    if authority.contains('@') {
        return Err("the endpoint's address cannot carry a user name or password".to_owned());
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        match bracketed.split_once(']') {
            Some((host, after)) if after.is_empty() || after.starts_with(':') => host,
            _ => return Err("the endpoint's address has an unclosed IPv6 host".to_owned()),
        }
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    if host.is_empty() {
        return Err("the endpoint's address has no host".to_owned());
    }
    let host = host.to_ascii_lowercase();
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1");
    if scheme == "http" && !local {
        return Err(
            "an endpoint on another computer needs https://; plain http:// only to this computer (localhost)"
                .to_owned(),
        );
    }
    let normalized = format!("{scheme}://{rest}");
    Ok(normalized.trim_end_matches('/').to_owned())
}

/// Whether an address names this computer (local models: nothing leaves
/// it).
pub fn local(url: &str) -> bool {
    endpoint(url).is_ok_and(|url| {
        let rest = url
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        let authority = rest
            .split('/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        authority == "localhost"
            || authority.starts_with("localhost:")
            || authority == "127.0.0.1"
            || authority.starts_with("127.0.0.1:")
            || authority.starts_with("[::1]")
    })
}

/// The Anthropic API's base without a trailing `/v1` (an address copied
/// with it works too).
fn anthropic_base(base: &str) -> &str {
    base.strip_suffix("/v1").unwrap_or(base)
}

/// `api_models_request`: `agent`, `url` → `url` (where the models are
/// listed), `headers`, `auth`; or an error for an address that is not
/// allowed.
pub fn models_request(request: &Value) -> Result<Value, String> {
    let agent = text_field(request, "agent");
    let provider =
        provider(agent).ok_or_else(|| format!("“{}” is no API agent", crate::echoed(agent)))?;
    let base = endpoint(text_field(request, "url"))?;
    Ok(match provider {
        Provider::Anthropic => json!({
            "url": format!("{}/v1/models?limit=100", anthropic_base(&base)),
            "headers": {"anthropic-version": ANTHROPIC_VERSION},
            "auth": "x-api-key",
        }),
        Provider::OpenAi => json!({
            "url": format!("{base}/models"),
            "headers": {},
            "auth": "bearer",
        }),
    })
}

/// `api_models`: `agent`, `status`, `body` (of the models request), `error`
/// (a network error) → `ok` (the service answered and took the key),
/// `models` (`id`, `name`), `account` (an account failure: `code`,
/// `reason`) and `text` (why it failed).
pub fn models(request: &Value) -> Value {
    let agent = text_field(request, "agent");
    let status = request.get("status").and_then(Value::as_u64).unwrap_or(0);
    let body = text_field(request, "body");
    let network = text_field(request, "error");
    let fallback = if provider(agent) == Some(Provider::Anthropic) {
        default_models()
    } else {
        Vec::new()
    };
    if status == 0 || (!network.is_empty() && !(200..300).contains(&status)) {
        let text = if network.is_empty() {
            "no answer".to_owned()
        } else {
            cut(network, 500)
        };
        return json!({"ok": false, "models": fallback, "account": null, "text": text});
    }
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let (message, _) = error_of(&parsed, body);
        let account = account::classify(&json!({"status": status, "body": parsed}), &message);
        let text = if message.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("HTTP {status}: {message}")
        };
        return json!({"ok": false, "models": fallback, "account": account, "text": cut(&text, 500)});
    }
    let mut found: Vec<Value> = parsed
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|model| {
            let id = model.get("id").and_then(Value::as_str)?;
            if !valid_model(id) {
                return None;
            }
            let name = model
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(id);
            Some(json!({"id": id, "name": cut(name, 100)}))
        })
        .take(100)
        .collect();
    if found.is_empty() {
        found = fallback;
    }
    json!({"ok": true, "models": found, "account": null, "text": ""})
}

fn default_models() -> Vec<Value> {
    ANTHROPIC_MODELS
        .iter()
        .map(|(id, name)| json!({"id": id, "name": name}))
        .collect()
}

/// `api_default_models`: the models offered before the service listed its
/// own (the Anthropic API's current ones; none for the others).
pub fn offered(agent: &str) -> Value {
    let models = if provider(agent) == Some(Provider::Anthropic) {
        default_models()
    } else {
        Vec::new()
    };
    json!({"models": models})
}

/// An error response's message and its structured part: Anthropic's
/// `{"type": "error", "error": {"type", "message"}}`, OpenAI's `{"error":
/// {"message"}}`, Ollama's `{"error": "..."}`, else the body's text.
fn error_of(parsed: &Value, body: &str) -> (String, Value) {
    let error = parsed.get("error").unwrap_or(&Value::Null);
    let message = match error {
        Value::String(text) => text.clone(),
        Value::Object(fields) => fields
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => parsed
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| body.trim().to_owned(), str::to_owned),
    };
    (cut(&message, 2000), error.clone())
}

/// Whether a refusal says the model cannot call tools (Ollama: "... does
/// not support tools").
fn no_tools(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "does not support tools",
        "doesn't support tools",
        "does not support tool",
        "tools are not supported",
        "tool use is not supported",
        "tool calling is not supported",
        "does not support function calling",
        "function calling is not supported",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

/// What a request of the MCP connection was.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Mcp {
    Initialize,
    Tools,
    /// A tool call of the current round: its index there.
    Call(usize),
    /// A call whose turn was stopped (its id): its result only updates the
    /// panel's row.
    Stopped(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Connecting,
    Ready,
    Failed,
}

/// One call of the model in the current round.
struct Call {
    id: String,
    name: String,
    input: Value,
    /// Set when the input could not be read: the call is not made.
    invalid: Option<String>,
    /// Its result for the model, once it came.
    result: Option<Value>,
}

/// The answer being streamed.
#[derive(Default)]
struct Answer {
    id: u64,
    status: u64,
    // Server-sent events: the current event's name and data.
    event: String,
    data: String,
    // A refusal's body.
    body: String,
    // The Anthropic API's content blocks (by index) and tool inputs.
    blocks: Vec<Value>,
    inputs: HashMap<usize, String>,
    stop_reason: String,
    complete: bool,
    // An OpenAI-compatible answer: its text and calls (index, id, name,
    // arguments).
    text: String,
    calls: Vec<(u64, String, String, String)>,
    finish: String,
    // An error the stream itself carried.
    error: Option<(String, Value)>,
    // The calls whose streamed input was no JSON object (not made).
    invalid: Vec<String>,
}

/// The message being answered.
#[derive(Default)]
struct Turn {
    /// The conversation's length before the message: a refusal before any
    /// answer takes the message out again.
    start: usize,
    rounds: u32,
    answered: bool,
    answer: Option<Answer>,
    calls: Vec<Call>,
}

pub struct ApiStream {
    provider: Provider,
    agent: String,
    name: String,
    base: String,
    model: String,
    session: String,
    resume_failed: bool,
    phase: Phase,
    next_mcp: u64,
    mcp: HashMap<u64, Mcp>,
    system: String,
    tools: Vec<Value>,
    messages: Vec<Value>,
    /// A message given before the tools were there.
    waiting: Vec<Value>,
    turn: Option<Turn>,
    next_http: u64,
}

impl ApiStream {
    /// The stream of the API agent `agent` (`anthropic`, `ollama`,
    /// `lmstudio`, `openai`); `start` sets it up.
    pub fn new(agent: &str) -> Option<ApiStream> {
        Some(ApiStream::blank(provider(agent)?, agent))
    }

    fn blank(provider: Provider, agent: &str) -> ApiStream {
        ApiStream {
            provider,
            agent: agent.to_owned(),
            name: String::new(),
            base: String::new(),
            model: String::new(),
            session: String::new(),
            resume_failed: false,
            phase: Phase::Connecting,
            next_mcp: 0,
            mcp: HashMap::new(),
            system: format!("{PREFACE}\n\n{}", super::guidance()),
            tools: Vec::new(),
            messages: Vec::new(),
            waiting: Vec::new(),
            turn: None,
            next_http: 0,
        }
    }

    /// A control of the application: `start` (`name`, `url`, `model`,
    /// `session`, `resume`, `transcript`), `message` (`text`, `context`,
    /// `images`), `interrupt`, `http_line` (`id`, `status`, `line`) or
    /// `http_end` (`id`, `status`, `error`).
    pub fn control(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        match text_field(request, "op") {
            "start" => self.start(request),
            "message" => self.message(request),
            "interrupt" => Ok(self.interrupt()),
            "http_line" => Ok(self.http_line(request)),
            "http_end" => Ok(self.http_end(request)),
            other => Err(format!(
                "unknown API agent control “{}”",
                crate::echoed(other)
            )),
        }
    }

    fn start(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        let base = endpoint(text_field(request, "url"))?;
        let model = text_field(request, "model").trim();
        let model = if model.is_empty() && self.provider == Provider::Anthropic {
            ANTHROPIC_DEFAULT_MODEL
        } else {
            model
        };
        if model.is_empty() {
            return Err(format!(
                "no model is chosen, and {} listed none: choose one in the panel",
                cut(&base, 200)
            ));
        }
        if !valid_model(model) {
            return Err("the model's name has characters a model name has not".to_owned());
        }
        let session = text_field(request, "session");
        if session.is_empty()
            || session.len() > 128
            || !session
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("the conversation needs its session id".to_owned());
        }
        let name = text_field(request, "name");
        let resume = request
            .get("resume")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let restored = if resume {
            restore(self.provider, text_field(request, "transcript"))
        } else {
            None
        };
        *self = ApiStream {
            name: cut(
                if name.is_empty() {
                    self.agent.as_str()
                } else {
                    name
                },
                80,
            ),
            base,
            model: model.to_owned(),
            session: session.to_owned(),
            resume_failed: resume && restored.is_none(),
            messages: restored.unwrap_or_default(),
            ..ApiStream::blank(self.provider, &self.agent)
        };
        let initialize = self.mcp_request(
            Mcp::Initialize,
            "initialize",
            json!({"protocolVersion": crate::PROTOCOL_VERSION, "capabilities": {},
                   "clientInfo": {"name": format!("MitCad chat ({})", self.name),
                                  "version": env!("CARGO_PKG_VERSION")}}),
        );
        Ok(vec![initialize])
    }

    fn mcp_request(&mut self, asked: Mcp, method: &str, params: Value) -> Value {
        let id = self.next_mcp;
        self.next_mcp += 1;
        self.mcp.insert(id, asked);
        mcp_line(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    /// The panel events of one line MitCad's listener wrote.
    pub fn feed(&mut self, line: &str) -> Vec<Value> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Vec::new();
        }
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(line) else {
            return vec![json!({"kind": "output", "text": cut(line, 500)})];
        };
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            // A notification (the comments resource) or an error without
            // an id: nothing to do.
            return Vec::new();
        };
        if message.get("method").is_some() {
            // The listener asks the client nothing; answered as unknown.
            return vec![mcp_line(json!({"jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": "MitCad's chat handles no requests"}}))];
        }
        let Some(asked) = self.mcp.remove(&id) else {
            return Vec::new();
        };
        let failure = message
            .get("error")
            .filter(|error| !error.is_null())
            .map(|error| {
                cut(
                    error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("refused"),
                    500,
                )
            });
        let result = message.get("result").cloned().unwrap_or(Value::Null);
        match asked {
            Mcp::Initialize => match failure {
                Some(text) => self.bridge_failed(&text),
                None => {
                    let instructions = result
                        .get("instructions")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !instructions.is_empty() {
                        self.system = format!(
                            "{PREFACE}\n\n{}\n\n{}",
                            cut(instructions, 32000),
                            crate::guide::index()
                        );
                    }
                    let initialized =
                        mcp_line(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
                    let tools = self.mcp_request(Mcp::Tools, "tools/list", json!({}));
                    vec![initialized, tools]
                }
            },
            Mcp::Tools => match failure {
                Some(text) => self.bridge_failed(&text),
                None => {
                    self.tools = result
                        .get("tools")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if self.tools.is_empty() {
                        return self.bridge_failed("MitCad's listener offered no tools");
                    }
                    self.phase = Phase::Ready;
                    let mut events = vec![json!({
                        "kind": "init",
                        "session": self.session,
                        "model": self.model,
                        "bridge": "connected",
                        "bridge_error": null,
                        "resume_failed": self.resume_failed,
                    })];
                    let waiting = std::mem::take(&mut self.waiting);
                    for content in waiting {
                        events.extend(self.begin(content));
                    }
                    events
                }
            },
            Mcp::Call(index) => self.call_result(index, failure, &result),
            Mcp::Stopped(call) => {
                let (error, text, _) = self.tool_result(failure, &result);
                // The panel's row only: the call's place in the
                // conversation was taken by its stop.
                vec![json!({"kind": "tool_result", "id": call,
                            "error": error, "text": cut(&text, MAX_RESULT_CHARS)})]
            }
        }
    }

    fn bridge_failed(&mut self, text: &str) -> Vec<Value> {
        self.phase = Phase::Failed;
        vec![
            json!({"kind": "init", "session": self.session, "model": self.model,
                   "bridge": "failed", "bridge_error": cut(text, 500),
                   "resume_failed": self.resume_failed}),
            json!({"kind": "end", "text": format!("MitCad's tools did not start: {}", cut(text, 500))}),
        ]
    }

    fn message(&mut self, request: &Value) -> Result<Vec<Value>, String> {
        let text = text_field(request, "text").trim();
        if text.is_empty() {
            return Err("a message needs its text".to_owned());
        }
        let context = text_field(request, "context").trim();
        let images = pictures(request)?;
        let content = match self.provider {
            Provider::Anthropic => {
                let mut blocks = Vec::new();
                for (media, data) in &images {
                    blocks.push(json!({"type": "image",
                        "source": {"type": "base64", "media_type": media, "data": data}}));
                }
                blocks.push(json!({"type": "text", "text": cut(text, MAX_TEXT_CHARS)}));
                if !context.is_empty() {
                    blocks.push(json!({"type": "text", "text": cut(context, MAX_TEXT_CHARS)}));
                }
                Value::Array(blocks)
            }
            Provider::OpenAi => {
                let mut parts = vec![json!({"type": "text", "text": cut(text, MAX_TEXT_CHARS)})];
                if !context.is_empty() {
                    parts.push(json!({"type": "text", "text": cut(context, MAX_TEXT_CHARS)}));
                }
                for (media, data) in &images {
                    parts.push(json!({"type": "image_url",
                        "image_url": {"url": format!("data:{media};base64,{data}")}}));
                }
                Value::Array(parts)
            }
        };
        match self.phase {
            Phase::Failed => Err("MitCad's tools did not start for this conversation".to_owned()),
            Phase::Connecting => {
                self.waiting.push(content);
                Ok(Vec::new())
            }
            Phase::Ready if self.turn.is_some() => {
                Err("a message is still being answered".to_owned())
            }
            Phase::Ready => Ok(self.begin(content)),
        }
    }

    /// A message's turn: the user's message in the conversation and the
    /// first request.
    fn begin(&mut self, content: Value) -> Vec<Value> {
        let start = self.messages.len();
        self.push_user(content);
        self.turn = Some(Turn {
            start,
            ..Turn::default()
        });
        vec![self.request()]
    }

    /// A user message; with the Anthropic API one after another user
    /// message (a stopped or refused one) joins it.
    fn push_user(&mut self, content: Value) {
        if self.provider == Provider::Anthropic
            && let Some(last) = self.messages.last_mut()
            && last.get("role").and_then(Value::as_str) == Some("user")
        {
            let mut joined = match last.get("content") {
                Some(Value::Array(blocks)) => blocks.clone(),
                Some(Value::String(text)) => vec![json!({"type": "text", "text": text})],
                _ => Vec::new(),
            };
            if let Value::Array(blocks) = content {
                joined.extend(blocks);
            }
            last["content"] = Value::Array(joined);
            return;
        }
        self.messages
            .push(json!({"role": "user", "content": content}));
    }

    /// The next request of the turn: the whole conversation, streamed.
    fn request(&mut self) -> Value {
        let id = self.next_http;
        self.next_http += 1;
        if let Some(turn) = self.turn.as_mut() {
            turn.rounds += 1;
            turn.answer = Some(Answer {
                id,
                ..Answer::default()
            });
            turn.calls.clear();
        }
        match self.provider {
            Provider::Anthropic => {
                let tools: Vec<Value> = self
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool.get("name").cloned().unwrap_or(Value::Null),
                            "description": tool.get("description").cloned().unwrap_or(json!("")),
                            "input_schema": tool.get("inputSchema").cloned()
                                .unwrap_or_else(|| json!({"type": "object"})),
                        })
                    })
                    .collect();
                let body = json!({
                    "model": self.model,
                    "max_tokens": MAX_TOKENS,
                    "stream": true,
                    "system": self.system,
                    "tools": tools,
                    "messages": self.messages,
                });
                json!({
                    "kind": "http", "id": id, "method": "POST",
                    "url": format!("{}/v1/messages", anthropic_base(&self.base)),
                    "headers": {"content-type": "application/json", "accept": "text/event-stream",
                                "anthropic-version": ANTHROPIC_VERSION},
                    "auth": "x-api-key",
                    "body": body.to_string(),
                })
            }
            Provider::OpenAi => {
                let tools: Vec<Value> = self
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({"type": "function", "function": {
                            "name": tool.get("name").cloned().unwrap_or(Value::Null),
                            "description": tool.get("description").cloned().unwrap_or(json!("")),
                            "parameters": tool.get("inputSchema").cloned()
                                .unwrap_or_else(|| json!({"type": "object"})),
                        }})
                    })
                    .collect();
                let mut messages = vec![json!({"role": "system", "content": self.system})];
                messages.extend(self.messages.iter().cloned());
                let body = json!({
                    "model": self.model,
                    "stream": true,
                    "tools": tools,
                    "messages": messages,
                });
                json!({
                    "kind": "http", "id": id, "method": "POST",
                    "url": format!("{}/chat/completions", self.base),
                    "headers": {"content-type": "application/json", "accept": "text/event-stream"},
                    "auth": "bearer",
                    "body": body.to_string(),
                })
            }
        }
    }

    fn current(&mut self, id: u64) -> Option<&mut Answer> {
        self.turn
            .as_mut()
            .and_then(|turn| turn.answer.as_mut())
            .filter(|answer| answer.id == id)
    }

    fn http_line(&mut self, request: &Value) -> Vec<Value> {
        let id = request
            .get("id")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let status = request.get("status").and_then(Value::as_u64).unwrap_or(0);
        let line = text_field(request, "line")
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        let provider = self.provider;
        let Some(answer) = self.current(id) else {
            return Vec::new();
        };
        if status != 0 {
            answer.status = status;
        }
        if !(200..300).contains(&answer.status) {
            // A refusal: its body, read at the end.
            if answer.body.len() < MAX_ERROR_BODY {
                answer.body.push_str(&line);
                answer.body.push('\n');
            }
            return Vec::new();
        }
        if line.is_empty() {
            let event = std::mem::take(&mut answer.event);
            let data = std::mem::take(&mut answer.data);
            if data.is_empty() {
                return Vec::new();
            }
            return match provider {
                Provider::Anthropic => anthropic_event(answer, &event, &data),
                Provider::OpenAi => openai_event(answer, &data),
            };
        }
        if line.starts_with(':') {
            return Vec::new();
        }
        let (field, value) = line.split_once(':').unwrap_or((line.as_str(), ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => answer.event = value.to_owned(),
            "data" if answer.data.len() + value.len() < MAX_EVENT_DATA => {
                if !answer.data.is_empty() {
                    answer.data.push('\n');
                }
                answer.data.push_str(value);
            }
            _ => {}
        }
        Vec::new()
    }

    fn http_end(&mut self, request: &Value) -> Vec<Value> {
        let id = request
            .get("id")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let status = request.get("status").and_then(Value::as_u64).unwrap_or(0);
        let network = text_field(request, "error").to_owned();
        let provider = self.provider;
        let mut events = Vec::new();
        let Some(answer) = self.current(id) else {
            return events;
        };
        if status != 0 {
            answer.status = status;
        }
        // The last event when the stream ended without its blank line.
        if (200..300).contains(&answer.status) && !answer.data.is_empty() {
            let event = std::mem::take(&mut answer.event);
            let data = std::mem::take(&mut answer.data);
            events.extend(match provider {
                Provider::Anthropic => anthropic_event(answer, &event, &data),
                Provider::OpenAi => openai_event(answer, &data),
            });
        }
        let Some(turn) = self.turn.as_mut() else {
            return events;
        };
        let Some(answer) = turn.answer.take() else {
            return events;
        };
        if !(200..300).contains(&answer.status) {
            events.extend(not_made(&answer, NOT_MADE_FAILED));
            events.extend(self.refused(&answer, &network));
            return events;
        }
        if let Some((message, details)) = answer.error.clone() {
            events.extend(not_made(&answer, NOT_MADE_FAILED));
            events.extend(self.failed(&message, &details, 0));
            return events;
        }
        let complete = match provider {
            Provider::Anthropic => answer.complete || !answer.stop_reason.is_empty(),
            Provider::OpenAi => answer.complete || !answer.finish.is_empty(),
        };
        if !complete {
            let text = if network.is_empty() {
                "The answer ended before it was complete.".to_owned()
            } else {
                format!(
                    "The answer ended before it was complete: {}",
                    cut(&network, 500)
                )
            };
            events.extend(not_made(&answer, NOT_MADE_FAILED));
            events.extend(self.finish("error", true, text));
            return events;
        }
        events.extend(match provider {
            Provider::Anthropic => self.anthropic_answered(answer),
            Provider::OpenAi => self.openai_answered(answer),
        });
        events
    }

    /// The service refused the request or did not answer.
    fn refused(&mut self, answer: &Answer, network: &str) -> Vec<Value> {
        if answer.status == 0 {
            let text = if network.is_empty() {
                format!("{} did not answer.", self.name)
            } else {
                format!("{} did not answer: {}", self.name, cut(network, 500))
            };
            return self.finish("error", true, text);
        }
        let parsed: Value = serde_json::from_str(answer.body.trim()).unwrap_or(Value::Null);
        let (message, details) = error_of(&parsed, &answer.body);
        self.failed(&message, &details, answer.status)
    }

    /// A refusal's message: the model cannot call tools, the account cannot
    /// be used, or another error.
    fn failed(&mut self, message: &str, details: &Value, status: u64) -> Vec<Value> {
        if no_tools(message) {
            let text = format!(
                "This model cannot edit the design: {} does not support tool calls. Choose a model that does.",
                self.model
            );
            let mut events =
                vec![json!({"kind": "status", "subtype": "no_tools", "text": cut(message, 500)})];
            events.extend(self.finish("no_tools", true, text));
            return events;
        }
        let account = account::classify(&json!({"status": status, "error": details}), message);
        let text = match (status, message.is_empty()) {
            (0, _) => message.to_owned(),
            (_, true) => format!("{} refused the request (HTTP {status}).", self.name),
            (_, false) => format!(
                "{} refused the request (HTTP {status}): {message}",
                self.name
            ),
        };
        match account {
            Some(account) => {
                let mut events = vec![json!({"kind": "account_error", "account": account,
                                             "text": cut(&text, MAX_TEXT_CHARS)})];
                let mut end = self.finish("error", true, text);
                for event in &mut end {
                    if event.get("kind").and_then(Value::as_str) == Some("turn_end") {
                        event["account_error"] = json!(true);
                    }
                }
                events.extend(end);
                events
            }
            None => self.finish("error", true, text),
        }
    }

    /// The turn ends: a refused first request takes its message out of the
    /// conversation again (it is written again or differently); the
    /// conversation is given to keep.
    fn finish(&mut self, subtype: &str, error: bool, text: String) -> Vec<Value> {
        if let Some(turn) = self.turn.take()
            && error
            && !turn.answered
            && turn.start <= self.messages.len()
        {
            self.messages.truncate(turn.start);
        }
        let mut events = vec![json!({
            "kind": "turn_end",
            "error": error,
            "subtype": subtype,
            "text": cut(&text, MAX_TEXT_CHARS),
            "session": self.session,
        })];
        events.extend(self.transcript());
        events
    }

    /// The conversation to keep for resuming: pictures left out.
    fn transcript(&self) -> Vec<Value> {
        let messages: Vec<Value> = self.messages.iter().map(without_pictures).collect();
        let content = json!({
            "format": TRANSCRIPT_FORMAT,
            "provider": provider_name(self.provider),
            "messages": messages,
        })
        .to_string();
        if content.len() > MAX_TRANSCRIPT {
            return vec![json!({"kind": "status", "subtype": "transcript_too_large", "text": ""})];
        }
        vec![json!({"kind": "transcript", "session": self.session, "content": content})]
    }

    fn anthropic_answered(&mut self, answer: Answer) -> Vec<Value> {
        let cut_short = answer.stop_reason == "max_tokens";
        // Calls shown as they streamed that are not made: their rows end.
        let dropped = if cut_short {
            not_made(&answer, NOT_MADE_CUT)
        } else if answer.stop_reason == "refusal" {
            not_made(&answer, NOT_MADE_REFUSED)
        } else {
            Vec::new()
        };
        let invalid_inputs = answer.invalid;
        let mut blocks: Vec<Value> = answer
            .blocks
            .into_iter()
            .filter(|block| !block.is_null())
            .collect();
        if cut_short {
            // A call cut at the output limit has no whole input: not made,
            // and not kept (each kept call needs its result).
            blocks.retain(|block| block.get("type").and_then(Value::as_str) != Some("tool_use"));
        }
        let calls: Vec<Call> = blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
            .map(|block| {
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                let id = cut(
                    block.get("id").and_then(Value::as_str).unwrap_or_default(),
                    200,
                );
                let invalid = (!input.is_object() || invalid_inputs.contains(&id))
                    .then(|| "The call's input was not a JSON object.".to_owned());
                Call {
                    id,
                    name: cut(
                        block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        100,
                    ),
                    input,
                    invalid,
                    result: None,
                }
            })
            .collect();
        if !blocks.is_empty() {
            self.messages
                .push(json!({"role": "assistant", "content": blocks}));
            if let Some(turn) = self.turn.as_mut() {
                turn.answered = true;
            }
        }
        if answer.stop_reason == "refusal" {
            let mut events = dropped;
            events.extend(self.finish(
                "refusal",
                true,
                "The model declined this request.".to_owned(),
            ));
            return events;
        }
        if cut_short {
            let mut events = dropped;
            events.extend(self.finish(
                "max_tokens",
                true,
                "The answer was cut at the model's output limit.".to_owned(),
            ));
            return events;
        }
        if calls.is_empty() {
            return self.finish("success", false, String::new());
        }
        // Their rows were shown as their inputs streamed.
        self.run(calls, false)
    }

    fn openai_answered(&mut self, answer: Answer) -> Vec<Value> {
        let mut calls = Vec::new();
        let mut listed = Vec::new();
        for (index, (_, id, name, arguments)) in answer.calls.iter().enumerate() {
            let id = if id.is_empty() {
                format!("call_{}_{index}", answer.id)
            } else {
                cut(id, 200)
            };
            let parsed: Result<Value, _> = if arguments.trim().is_empty() {
                Ok(json!({}))
            } else {
                serde_json::from_str(arguments)
            };
            let (input, invalid) = match parsed {
                Ok(Value::Object(object)) => (Value::Object(object), None),
                Ok(_) | Err(_) => (
                    Value::Null,
                    Some("The call's arguments were not a JSON object.".to_owned()),
                ),
            };
            listed.push(json!({"id": id, "type": "function",
                               "function": {"name": name, "arguments": arguments}}));
            calls.push(Call {
                id,
                name: cut(name, 100),
                input,
                invalid,
                result: None,
            });
        }
        let mut message = Map::new();
        message.insert("role".to_owned(), json!("assistant"));
        message.insert(
            "content".to_owned(),
            if answer.text.is_empty() && !listed.is_empty() {
                Value::Null
            } else {
                json!(answer.text)
            },
        );
        if !listed.is_empty() {
            message.insert("tool_calls".to_owned(), Value::Array(listed));
        }
        self.messages.push(Value::Object(message));
        if let Some(turn) = self.turn.as_mut() {
            turn.answered = true;
        }
        if calls.is_empty() {
            if answer.finish == "length" {
                return self.finish(
                    "max_tokens",
                    true,
                    "The answer was cut at the model's output limit.".to_owned(),
                );
            }
            return self.finish("success", false, String::new());
        }
        self.run(calls, true)
    }

    /// The round's calls: each through MitCad's listener (the approvals
    /// apply), one that could not be read answered at once.
    fn run(&mut self, calls: Vec<Call>, show: bool) -> Vec<Value> {
        let mut events = Vec::new();
        let mut requests = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            if show {
                events.push(json!({"kind": "tool", "id": call.id, "name": call.name,
                                   "server": "mitcad", "input": call.input}));
            }
            if call.invalid.is_none() {
                requests.push((index, call.name.clone(), call.input.clone()));
            }
        }
        if let Some(turn) = self.turn.as_mut() {
            turn.calls = calls;
        }
        let mut invalid = Vec::new();
        if let Some(turn) = self.turn.as_ref() {
            for (index, call) in turn.calls.iter().enumerate() {
                if let Some(reason) = &call.invalid {
                    invalid.push((index, reason.clone()));
                }
            }
        }
        for (index, reason) in invalid {
            events.extend(self.answer_call(
                index,
                true,
                reason.clone(),
                error_content(self.provider, &reason),
            ));
        }
        for (index, name, input) in requests {
            let line = self.mcp_request(
                Mcp::Call(index),
                "tools/call",
                json!({"name": name, "arguments": input}),
            );
            events.push(line);
        }
        events.extend(self.round_done());
        events
    }

    /// A tool call's result from the listener.
    fn call_result(&mut self, index: usize, failure: Option<String>, result: &Value) -> Vec<Value> {
        let (error, text, content) = self.tool_result(failure, result);
        let mut events = self.answer_call(index, error, text, content);
        events.extend(self.round_done());
        events
    }

    /// A call's result: whether it failed, its text for the panel and its
    /// content for the model.
    fn tool_result(&self, failure: Option<String>, result: &Value) -> (bool, String, Value) {
        if let Some(text) = failure {
            return (true, text.clone(), error_content(self.provider, &text));
        }
        let error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut texts = Vec::new();
        let mut blocks = Vec::new();
        for block in result
            .get("content")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    texts.push(text.to_owned());
                    blocks.push(json!({"type": "text", "text": cut_tool_text(text)}));
                }
                Some("image") => {
                    texts.push("[image]".to_owned());
                    let media = block
                        .get("mimeType")
                        .and_then(Value::as_str)
                        .unwrap_or("image/png");
                    let data = block
                        .get("data")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if matches!(
                        media,
                        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
                    ) && !data.is_empty()
                    {
                        blocks.push(json!({"type": "image",
                            "source": {"type": "base64", "media_type": media, "data": data}}));
                    }
                }
                _ => {}
            }
        }
        if texts.is_empty()
            && let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null())
        {
            let text = structured.to_string();
            blocks.push(json!({"type": "text", "text": cut_tool_text(&text)}));
            texts.push(text);
        }
        let text = texts.join("\n");
        let content = match self.provider {
            Provider::Anthropic => {
                if blocks.is_empty() {
                    blocks.push(json!({"type": "text", "text": "(no content)"}));
                }
                Value::Array(blocks)
            }
            Provider::OpenAi => {
                // Text only: a tool message of these services carries no
                // pictures.
                let mut plain = cut_tool_text(&text);
                if plain.contains("[image]") {
                    plain = plain.replace(
                        "[image]",
                        "[a picture, which this connection does not pass to the model]",
                    );
                }
                if plain.is_empty() {
                    plain = "(no content)".to_owned();
                }
                json!(plain)
            }
        };
        (error, text, content)
    }

    fn answer_call(
        &mut self,
        index: usize,
        error: bool,
        text: String,
        content: Value,
    ) -> Vec<Value> {
        let Some(turn) = self.turn.as_mut() else {
            return Vec::new();
        };
        let Some(call) = turn.calls.get_mut(index) else {
            return Vec::new();
        };
        if call.result.is_some() {
            return Vec::new();
        }
        call.result = Some(json!({"error": error, "content": content}));
        vec![json!({"kind": "tool_result", "id": call.id, "error": error,
                    "text": cut(&text, MAX_RESULT_CHARS)})]
    }

    /// Once every call of the round has its result: the results in the
    /// conversation, then the next request (or the end at the limit).
    fn round_done(&mut self) -> Vec<Value> {
        let Some(turn) = self.turn.as_ref() else {
            return Vec::new();
        };
        if turn.answer.is_some()
            || turn.calls.is_empty()
            || turn.calls.iter().any(|call| call.result.is_none())
        {
            return Vec::new();
        }
        let rounds = turn.rounds;
        self.append_results();
        if rounds >= MAX_ROUNDS {
            return self.finish(
                "max_rounds",
                true,
                format!("The model made {MAX_ROUNDS} requests for this message without finishing; it was stopped."),
            );
        }
        vec![self.request()]
    }

    /// The round's results as the conversation's next message(s).
    fn append_results(&mut self) {
        let Some(turn) = self.turn.as_mut() else {
            return;
        };
        let calls = std::mem::take(&mut turn.calls);
        match self.provider {
            Provider::Anthropic => {
                let results: Vec<Value> = calls
                    .iter()
                    .map(|call| {
                        let result = call.result.clone().unwrap_or_else(
                            || json!({"error": true, "content": [{"type": "text", "text": STOPPED}]}),
                        );
                        json!({"type": "tool_result", "tool_use_id": call.id,
                               "content": result["content"],
                               "is_error": result["error"].as_bool().unwrap_or(false)})
                    })
                    .collect();
                self.messages
                    .push(json!({"role": "user", "content": results}));
            }
            Provider::OpenAi => {
                for call in &calls {
                    let content = call
                        .result
                        .as_ref()
                        .map_or_else(|| json!(STOPPED), |result| result["content"].clone());
                    self.messages
                        .push(json!({"role": "tool", "tool_call_id": call.id, "content": content}));
                }
            }
        }
    }

    /// Stop: the request in flight ends, calls still running keep their
    /// place with a stopped result, and the turn ends at once. The message
    /// stays in the conversation (the next one follows it).
    fn interrupt(&mut self) -> Vec<Value> {
        let Some(turn) = self.turn.as_mut() else {
            if self.waiting.is_empty() {
                return Vec::new();
            }
            self.waiting.clear();
            return vec![
                json!({"kind": "turn_end", "error": true, "subtype": "interrupted",
                               "text": "", "session": self.session}),
            ];
        };
        let mut events = Vec::new();
        if let Some(answer) = turn.answer.take() {
            events.push(json!({"kind": "http_abort", "id": answer.id}));
            events.extend(not_made(&answer, NOT_MADE_STOPPED));
        }
        turn.answered = true;
        let ids: Vec<String> = turn.calls.iter().map(|call| call.id.clone()).collect();
        if !ids.is_empty() {
            for asked in self.mcp.values_mut() {
                if let Mcp::Call(index) = asked {
                    let id = ids.get(*index).cloned().unwrap_or_default();
                    *asked = Mcp::Stopped(id);
                }
            }
            self.append_results();
        }
        events.extend(self.finish("interrupted", true, String::new()));
        events
    }
}

/// The text of a tool result for the model, cut at its limit with a note.
fn cut_tool_text(text: &str) -> String {
    let kept = cut(text, MAX_TOOL_TEXT);
    if kept.len() < text.len() {
        format!(
            "{kept}\n[cut: {} more bytes; ask for less, e.g. a narrower query]",
            text.len() - kept.len()
        )
    } else {
        kept
    }
}

fn error_content(provider: Provider, text: &str) -> Value {
    match provider {
        Provider::Anthropic => json!([{"type": "text", "text": text}]),
        Provider::OpenAi => json!(text),
    }
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "anthropic",
        Provider::OpenAi => "openai",
    }
}

/// The failed results of an answer's calls that are not made (it was
/// stopped, cut or failed): each was shown with `tool_start` as it
/// streamed, and its row ends with this result (mitcad#1317).
fn not_made(answer: &Answer, reason: &str) -> Vec<Value> {
    answer
        .blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(|block| {
            json!({"kind": "tool_result",
                   "id": cut(block.get("id").and_then(Value::as_str).unwrap_or_default(), 200),
                   "error": true, "text": reason})
        })
        .collect()
}

fn mcp_line(message: Value) -> Value {
    json!({"kind": "mcp", "line": message.to_string() + "\n"})
}

/// One server-sent event of the Anthropic Messages API.
fn anthropic_event(answer: &mut Answer, event: &str, data: &str) -> Vec<Value> {
    let Ok(message) = serde_json::from_str::<Value>(data) else {
        return vec![json!({"kind": "output", "text": cut(data, 500)})];
    };
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(event)
        .to_owned();
    let index = message
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < 4096);
    match kind.as_str() {
        "content_block_start" => {
            let (Some(index), Some(block)) = (index, message.get("content_block")) else {
                return Vec::new();
            };
            if answer.blocks.len() <= index {
                answer.blocks.resize(index + 1, Value::Null);
            }
            answer.blocks[index] = block.clone();
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    answer.inputs.insert(index, String::new());
                    vec![json!({"kind": "tool_start",
                                "id": cut(block.get("id").and_then(Value::as_str).unwrap_or_default(), 200),
                                "name": cut(block.get("name").and_then(Value::as_str).unwrap_or_default(), 100)})]
                }
                Some("text") => {
                    let text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![json!({"kind": "text", "text": cut(text, MAX_TEXT_CHARS)})]
                    }
                }
                _ => Vec::new(),
            }
        }
        "content_block_delta" => {
            let (Some(index), Some(delta)) = (index, message.get("delta")) else {
                return Vec::new();
            };
            let Some(block) = answer
                .blocks
                .get_mut(index)
                .filter(|block| block.is_object())
            else {
                return Vec::new();
            };
            let piece = |name: &str| {
                delta
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => {
                    let text = piece("text");
                    append(block, "text", &text);
                    if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![json!({"kind": "text", "text": cut(&text, MAX_TEXT_CHARS)})]
                    }
                }
                Some("input_json_delta") => {
                    let partial = piece("partial_json");
                    if let Some(input) = answer.inputs.get_mut(&index)
                        && input.len() + partial.len() < MAX_EVENT_DATA
                    {
                        input.push_str(&partial);
                    }
                    Vec::new()
                }
                Some("thinking_delta") => {
                    append(block, "thinking", &piece("thinking"));
                    Vec::new()
                }
                Some("signature_delta") => {
                    append(block, "signature", &piece("signature"));
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
        "content_block_stop" => {
            let Some(index) = index else {
                return Vec::new();
            };
            let Some(text) = answer.inputs.remove(&index) else {
                return Vec::new();
            };
            let Some(block) = answer
                .blocks
                .get_mut(index)
                .filter(|block| block.is_object())
            else {
                return Vec::new();
            };
            let input = if text.trim().is_empty() {
                block.get("input").cloned().unwrap_or_else(|| json!({}))
            } else {
                serde_json::from_str(&text).unwrap_or(Value::Null)
            };
            // The kept block needs an object; a call without one is not
            // made (Call::invalid).
            block["input"] = if input.is_object() {
                input.clone()
            } else {
                json!({})
            };
            let id = cut(
                block.get("id").and_then(Value::as_str).unwrap_or_default(),
                200,
            );
            let name = cut(
                block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                100,
            );
            if !input.is_object() {
                answer.invalid.push(id.clone());
            }
            vec![
                json!({"kind": "tool", "id": id, "name": name, "server": "mitcad", "input": input}),
            ]
        }
        "message_delta" => {
            if let Some(reason) = message
                .pointer("/delta/stop_reason")
                .and_then(Value::as_str)
            {
                answer.stop_reason = cut(reason, 60);
            }
            Vec::new()
        }
        "message_stop" => {
            answer.complete = true;
            Vec::new()
        }
        "error" => {
            let error = message.get("error").cloned().unwrap_or(Value::Null);
            let text = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the service reported an error")
                .to_owned();
            answer.error = Some((cut(&text, 2000), error));
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn append(block: &mut Value, field: &str, text: &str) {
    let mut joined = block
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    joined.push_str(text);
    block[field] = json!(joined);
}

/// One server-sent event of an OpenAI-compatible chat completions stream.
fn openai_event(answer: &mut Answer, data: &str) -> Vec<Value> {
    if data.trim() == "[DONE]" {
        answer.complete = true;
        return Vec::new();
    }
    let Ok(chunk) = serde_json::from_str::<Value>(data) else {
        return vec![json!({"kind": "output", "text": cut(data, 500)})];
    };
    if let Some(error) = chunk.get("error").filter(|error| !error.is_null()) {
        let (text, details) = error_of(&chunk, data);
        let text = if text.is_empty() {
            "the service reported an error".to_owned()
        } else {
            text
        };
        answer.error = Some((
            text,
            if details.is_null() {
                error.clone()
            } else {
                details
            },
        ));
        return Vec::new();
    }
    let Some(choice) = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return Vec::new();
    };
    let mut events = Vec::new();
    let delta = choice
        .get("delta")
        .or_else(|| choice.get("message"))
        .unwrap_or(&Value::Null);
    if let Some(text) = delta.get("content").and_then(Value::as_str)
        && !text.is_empty()
    {
        answer.text.push_str(text);
        events.push(json!({"kind": "text", "text": cut(text, MAX_TEXT_CHARS)}));
    }
    for (position, call) in delta
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let index = call
            .get("index")
            .and_then(Value::as_u64)
            .unwrap_or(position as u64);
        let slot = match answer.calls.iter().position(|entry| entry.0 == index) {
            Some(slot) => slot,
            None => {
                if answer.calls.len() >= 64 {
                    continue;
                }
                answer
                    .calls
                    .push((index, String::new(), String::new(), String::new()));
                answer.calls.len() - 1
            }
        };
        let entry = &mut answer.calls[slot];
        if let Some(id) = call.get("id").and_then(Value::as_str)
            && !id.is_empty()
        {
            entry.1 = id.to_owned();
        }
        let function = call.get("function").unwrap_or(&Value::Null);
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            entry.2.push_str(name);
        }
        match function.get("arguments") {
            Some(Value::String(piece)) => {
                if entry.3.len() + piece.len() < MAX_EVENT_DATA {
                    entry.3.push_str(piece);
                }
            }
            Some(object @ Value::Object(_)) => entry.3 = object.to_string(),
            _ => {}
        }
    }
    if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
        answer.finish = cut(reason, 60);
    }
    events
}

/// A kept conversation without its pictures (a view's picture is large and
/// was of that moment).
fn without_pictures(message: &Value) -> Value {
    fn strip(value: &Value) -> Value {
        match value {
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|item| match item.get("type").and_then(Value::as_str) {
                        Some("image") | Some("image_url") => {
                            json!({"type": "text", "text": "[a picture of the view, not kept]"})
                        }
                        _ => strip(item),
                    })
                    .collect(),
            ),
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| {
                        let kept = if key == "content" {
                            strip(value)
                        } else {
                            value.clone()
                        };
                        (key.clone(), kept)
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    strip(message)
}

/// A kept conversation read back: its messages, when it is this provider's
/// and well formed.
fn restore(provider: Provider, content: &str) -> Option<Vec<Value>> {
    if content.is_empty() || content.len() > MAX_TRANSCRIPT {
        return None;
    }
    let kept: Value = serde_json::from_str(content).ok()?;
    if kept.get("format").and_then(Value::as_str) != Some(TRANSCRIPT_FORMAT)
        || kept.get("provider").and_then(Value::as_str) != Some(provider_name(provider))
    {
        return None;
    }
    let messages = kept.get("messages")?.as_array()?.clone();
    let allowed: &[&str] = match provider {
        Provider::Anthropic => &["user", "assistant"],
        Provider::OpenAi => &["user", "assistant", "tool"],
    };
    messages
        .iter()
        .all(|message| {
            message
                .get("role")
                .and_then(Value::as_str)
                .is_some_and(|role| allowed.contains(&role))
        })
        .then_some(messages)
}
