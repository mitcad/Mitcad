// SPDX-License-Identifier: MIT
//! The AI chat panel's own logic for the application (mitcad#947): its JSON
//! requests and the agents' output streams turned into panel events
//! (`core/mcp/src/chat.rs`). The application runs the processes and keeps
//! the files.

use mitcad_mcp::chat;

/// One request of the chat's logic, JSON in and out; a failure is an
/// answer with `error`.
pub fn agent_chat(request: &str) -> String {
    chat::request_text(request)
}

/// The output stream of one agent process.
pub struct ChatStream(chat::Stream);

pub fn new_chat_stream(agent: &str) -> Result<Box<ChatStream>, crate::ApiError> {
    chat::Stream::new(agent)
        .map(|stream| Box::new(ChatStream(stream)))
        .map_err(crate::ApiError)
}

impl ChatStream {
    /// The panel events of one output line (without its newline), as a
    /// JSON array.
    pub fn feed_line(&mut self, line: &str) -> String {
        serde_json::Value::Array(self.0.feed(line)).to_string()
    }

    /// A control of the stream (Codex: start, message, interrupt), JSON
    /// in; its panel events as a JSON array, or an object with `error`.
    pub fn control(&mut self, request: &str) -> String {
        let events = serde_json::from_str::<serde_json::Value>(request)
            .map_err(|error| format!("invalid chat control: {error}"))
            .and_then(|value| self.0.control(&value));
        match events {
            Ok(events) => serde_json::Value::Array(events).to_string(),
            Err(error) => serde_json::json!({"error": error}).to_string(),
        }
    }
}
