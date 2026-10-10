// SPDX-License-Identifier: MIT
//! MCP 2025-11-25 over newline-delimited stdio, without a geometry dependency.
//!
//! The backend owns documents and validates their operations and filesystem
//! paths. This crate owns protocol envelopes, discovery and tool argument shapes.
//! Calls run synchronously; cancellation of an active kernel call is not offered.

mod catalog;
mod framing;

use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

/// The initialization-based MCP revision implemented by this server.
pub const PROTOCOL_VERSION: &str = "2025-11-25";
/// Maximum bytes in one incoming message, excluding its newline delimiter.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Resource containing the model's command and query reference.
pub const COMMANDS_URI: &str = "mitcad://reference/commands";

const COMMANDS_REFERENCE: &str = include_str!("../../model/src/api/commands.md");

/// Data returned by a document tool. Errors remain visible to the model.
pub struct ToolResult {
    pub value: Value,
    pub is_error: bool,
}

/// Document operations supplied by the application or a mock geometry backend.
pub trait Backend {
    /// An error here is a tool execution failure, rather than a protocol error.
    fn call(&mut self, name: &str, arguments: &Value) -> Result<ToolResult, String>;
}

/// Read requests until EOF and flush one compact JSON response per line.
///
/// Oversized messages are drained through their newline without allocating an
/// unbounded buffer. Diagnostics belong on stderr, never on `writer`.
pub fn serve<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    backend: impl Backend,
) -> io::Result<()> {
    let mut session = Session::new(backend);
    let mut frame = Vec::new();
    while let Some(status) = framing::read_frame(&mut reader, &mut frame, MAX_FRAME_BYTES)? {
        let response = match status {
            framing::FrameStatus::TooLarge => {
                Some(error(None, -32600, "Message exceeds the 8 MiB input limit"))
            }
            framing::FrameStatus::Complete => match serde_json::from_slice(&frame) {
                Ok(message) => session.handle(message),
                Err(_) => Some(error(None, -32700, "Invalid JSON message")),
            },
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut writer, &response).map_err(io::Error::other)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Uninitialized,
    AwaitingInitialized,
    Ready,
}

struct Session<B> {
    phase: Phase,
    backend: B,
}

impl<B: Backend> Session<B> {
    fn new(backend: B) -> Self {
        Self {
            phase: Phase::Uninitialized,
            backend,
        }
    }

    fn handle(&mut self, message: Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error(None, -32600, "Expected one JSON-RPC object"));
        };
        let id = object.get("id").filter(|id| valid_id(id));
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(id, -32600, "jsonrpc must be 2.0"));
        }
        // No server-to-client requests are issued, so there is no response to
        // consume. Never answer a response with another response.
        if !object.contains_key("method")
            && (object.contains_key("result") || object.contains_key("error"))
        {
            return None;
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Some(error(id, -32600, "method must be a string"));
        };
        let notification = !object.contains_key("id");
        if !notification && id.is_none() {
            return Some(error(None, -32600, "id must be a string or integer"));
        }
        if object.contains_key("result") || object.contains_key("error") {
            return (!notification)
                .then(|| error(id, -32600, "A request cannot contain result or error"));
        }
        let empty = json!({});
        let params = object.get("params").unwrap_or(&empty);
        if !params.is_object() {
            return (!notification).then(|| error(id, -32602, "params must be an object"));
        }
        if notification {
            if method == "notifications/initialized" && self.phase == Phase::AwaitingInitialized {
                self.phase = Phase::Ready;
            }
            // Request-only methods, unknown notifications, and cancellation
            // notifications for completed or uncancellable calls do no work.
            return None;
        }
        let id = id.expect("validated request id");
        let result = match method {
            "initialize" => return Some(self.initialize(id, params)),
            "ping" => json!({}),
            "tools/list"
            | "tools/call"
            | "resources/list"
            | "resources/read"
            | "resources/templates/list" => {
                if self.phase != Phase::Ready {
                    return Some(error(
                        Some(id),
                        -32600,
                        "Send initialize and notifications/initialized before using tools or resources",
                    ));
                }
                match method {
                    "tools/list" => {
                        if let Some(response) = reject_cursor(id, params) {
                            return Some(response);
                        }
                        json!({"tools": catalog::tools()})
                    }
                    "tools/call" => return Some(self.call(id, params)),
                    "resources/list" => {
                        if let Some(response) = reject_cursor(id, params) {
                            return Some(response);
                        }
                        json!({"resources": [{
                            "uri": COMMANDS_URI,
                            "name": "Mitcad document API",
                            "description": "JSON command and query reference, identities and metric units",
                            "mimeType": "text/markdown"
                        }]})
                    }
                    "resources/templates/list" => {
                        if let Some(response) = reject_cursor(id, params) {
                            return Some(response);
                        }
                        json!({"resourceTemplates": []})
                    }
                    "resources/read" => {
                        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                            return Some(error(Some(id), -32602, "uri must be a string"));
                        };
                        if uri != COMMANDS_URI {
                            return Some(error(Some(id), -32002, "Unknown resource URI"));
                        }
                        json!({"contents": [{
                            "uri": COMMANDS_URI,
                            "mimeType": "text/markdown",
                            "text": COMMANDS_REFERENCE
                        }]})
                    }
                    _ => unreachable!(),
                }
            }
            // In particular, a modern client's server/discover probe gets a
            // deterministic method-not-found reply before legacy initialization.
            _ => return Some(error(Some(id), -32601, "Unknown method")),
        };
        Some(success(id, result))
    }

    fn initialize(&mut self, id: &Value, params: &Value) -> Value {
        if self.phase != Phase::Uninitialized {
            return error(Some(id), -32600, "Server is already initialized");
        }
        if params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .is_none()
            || !params.get("capabilities").is_some_and(Value::is_object)
            || !params.get("clientInfo").is_some_and(|info| {
                info.is_object()
                    && info.get("name").and_then(Value::as_str).is_some()
                    && info.get("version").and_then(Value::as_str).is_some()
            })
        {
            return error(
                Some(id),
                -32602,
                "initialize requires protocolVersion, capabilities, and clientInfo with name and version",
            );
        }
        // MCP legacy negotiation returns a supported version when the client's
        // proposed revision differs. The client decides whether it can proceed.
        self.phase = Phase::AwaitingInitialized;
        success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "mitcad", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Local headless CAD documents. Read mitcad://reference/commands for model operations. Use the explicit document handle on every call. Files are confined to the configured workspace. Commands are single objects; lengths default to millimetres. Documents last until closed or the server exits."
            }),
        )
    }

    fn call(&mut self, id: &Value, params: &Value) -> Value {
        if params.get("task").is_some() {
            return error(
                Some(id),
                -32602,
                "Task-augmented tool calls are not supported",
            );
        }
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error(Some(id), -32602, "tools/call requires a tool name");
        };
        let Some(tool) = catalog::tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        else {
            return error(Some(id), -32602, "Unknown tool name");
        };
        let empty = json!({});
        let arguments = params.get("arguments").unwrap_or(&empty);
        if !arguments.is_object() {
            return error(Some(id), -32602, "arguments must be an object");
        }
        if let Err(message) = catalog::validate(arguments, &tool["inputSchema"], "arguments") {
            return error(Some(id), -32602, &message);
        }
        match self.backend.call(name, arguments) {
            Ok(result) => success(id, tool_content(result.value, result.is_error)),
            Err(message) => success(id, tool_error(message)),
        }
    }
}

fn valid_id(id: &Value) -> bool {
    id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()
}

fn reject_cursor(id: &Value, params: &Value) -> Option<Value> {
    params
        .get("cursor")
        .map(|_| error(Some(id), -32602, "This list is not paginated; omit cursor"))
}

fn success(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: Option<&Value>, code: i32, message: &str) -> Value {
    // This MCP revision permits an omitted error ID for an unreadable request;
    // a null ID is not a valid MCP RequestId.
    let mut response = json!({"jsonrpc": "2.0", "error": {"code": code, "message": message}});
    if let Some(id) = id {
        response["id"] = id.clone();
    }
    response
}

fn tool_error(message: String) -> Value {
    tool_content(json!({"error": message}), true)
}

fn tool_content(value: Value, is_error: bool) -> Value {
    // The 2025-11-25 structuredContent field is an object. Wrap a backend's
    // scalar or array while keeping its JSON data intact and the text identical.
    let value = if value.is_object() {
        value
    } else {
        json!({"value": value})
    };
    json!({
        "content": [{"type": "text", "text": value.to_string()}],
        "structuredContent": value,
        "isError": is_error
    })
}

#[cfg(test)]
mod tests;
