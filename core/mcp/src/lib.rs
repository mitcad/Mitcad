// SPDX-License-Identifier: MIT
//! MCP 2025-11-25 protocol for stdio and live GUI tools, without geometry.
//!
//! The backend owns documents and validates their operations and filesystem
//! paths. This crate owns protocol envelopes, discovery and tool argument shapes.
//! The live profile defers tool execution to the application's document owner.
//! Calls run synchronously; cancellation of an active kernel call is not offered.

pub mod assistant_setup;
mod catalog;
mod framing;
// Compact command results, command arrays (mitcad#689).
mod changes;
pub use changes::{
    TimelineMode, command_document, entry_failed, evaluated_features, recomputed, snapshot_queries,
};
mod hints;
pub use hints::{command_hint_query, command_hints, command_hints_enabled};
// The command reference in parts, errors that give the choices (mitcad#714).
mod reference;
pub use reference::{
    ECHO_CHARS, INTERNAL_MARKER, Vocabulary, echoed, missing_name, reference_losses,
    reference_text, unknown_command, unknown_query,
};
// The guides on how to model well, behind model_reference's guide and as
// mitcad://guide/<id> resources (mitcad#1508).
mod guide;
// The AI chat panel's own logic: agent streams, approvals, undo merge,
// history (mitcad#947).
pub mod chat;

// Comments for agents (mitcad#1119): the comments resource, its
// subscription, the author of comment commands and the resolve policy.
mod comments;
pub use comments::{COMMENTS_URI, RESOLVE_REFUSED};
// Guided tutorials (mitcad#2012): ui_guide's resource of the guide step.
mod ui_guide;
pub use ui_guide::UI_GUIDE_URI;

// Commands' JSON Schemas on demand through model_reference, with the
// server's restrictions (mitcad#1897).
mod schemas;
pub use schemas::{command_schema, schema_commands};
// References between the entries of a model_command array (mitcad#1897):
// "as" names an entry's result, {"ref": "<name>.<path>"} takes a value of
// it in a later entry.
mod array_refs;
pub use array_refs::{check_refs, has_refs, resolve_entry};
// The defaults MCP gives a new feature where the design file's default
// keeps designs saved before a choice as they were (mitcad#2162: a new
// loft merges its tangent edges).
mod new_feature_defaults;
pub use new_feature_defaults::new_feature_defaults;
// The print libraries for live agents when the user allows it
// (mitcad#2224): which calls use them, the refusal without the
// preference, and the binding to the open design's project.
pub mod print_library;
// The live queries that run as model jobs, off the window's UI thread
// (mitcad#2540).
pub mod query_jobs;

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};

/// The initialization-based MCP revision implemented by this server.
pub const PROTOCOL_VERSION: &str = "2025-11-25";
/// Maximum bytes in one request or response, excluding its newline delimiter.
/// Kept in sync with the live transport's `mcp/LocalEndpoint.hpp`.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
// Leave room for a compact error that preserves the request id and, for
// an applied command, acknowledges that the document already changed.
const RESPONSE_RESERVE_BYTES: usize = 4096;
/// The largest image returned inline by default, in its encoded bytes
/// (`ui_screenshot`'s and `model_render`'s `max_bytes`; mitcad#1188,
/// mitcad#1486).
pub const IMAGE_INLINE_BYTES: u64 = catalog::SCREENSHOT_INLINE_BYTES;
/// Resource containing the model's command and query reference.
pub const COMMANDS_URI: &str = "mitcad://reference/commands";

const COMMANDS_REFERENCE: &str = include_str!("../../model-api/src/commands.md");

/// The longest server instructions, in characters (mitcad#1506): Claude
/// Code before v2.1.296 keeps only the first 2,048 and later versions the
/// first 4,096, without telling the user. What applies to one tool is in
/// that tool's description, read when it is called.
pub const INSTRUCTIONS_CHARS: usize = 2000;

/// The headless server's instructions (mitcad#1506): only what applies
/// across tools, the most important first, in labelled lines (mitcad#1897).
const HEADLESS_INSTRUCTIONS: &str = "Documents: local headless CAD documents. document_create or document_open gives a handle; use the explicit document handle on every call. Documents last until closed or the server exits; files are confined to the configured workspace.\n\
Comments: a design can hold comments, notes pinned to features, faces, edges and points for an agent, some with images (query comment_attachment shows one): read the open ones first with model_query {\"query\": \"comments\", \"status\": \"open\"}, and after the change each asks for, put {\"cmd\": \"address_comment\", \"id\": ..., \"resolution\": \"<what was done, briefly>\"} in the same model_command array as that change; resolving comments is left to the user.\n\
Reference: model_reference gives commands, queries, feature types, their fields and commands' JSON Schemas (no arguments: its contents and every permitted name). How to model well here: model_reference {\"guide\": \"index\"}; unexpected results: {\"guide\": \"bug-reporting\"}.\n\
Units: JSON numbers are millimeters and radians; write \"90 deg\" for degrees. They are millimeters also in a design in another length unit (the document query's units.length): convert the user's lengths, or write them as text, read in that unit (\"0.875\", \"0.875 in\").\n\
Modeling: Place a part or a sketch on an existing face, construction plane, axis or point where there is one (a primitive's at, a sketch's plane), so that it follows the model; a fixed plane ({\"origin\", \"normal\"}) stays where it is written. Unless the user asks otherwise, make every sketch fully constrained and dimensioned: dimensions for the sizes that matter (named parameters where a value is used again) and constraints for the rest, tied to the origin or existing geometry, until the sketch query's dof is 0.\n\
Commands: a model_command array runs in order as one undo step and stops at the first refusal (undoing the entries before it) or failed feature; a query is one object. Each tool's description holds its own rules.";

/// The live server's instructions (mitcad#1506): only what applies across
/// tools, the most important first, in labelled lines (mitcad#1897).
const LIVE_INSTRUCTIONS: &str = "Control the documents open in the Mitcad application, alongside its user.\n\
Documents: document_list gives every handle and the active flag. Pass document to target a specific design; omitting it uses the active design (a chat agent's own, or the last one it made with document_new).\n\
Comments first: the user pins notes for you on the model, some with images (query comment_attachment shows one); ui_state's comments.open counts the open ones. Read them before other work (mitcad://comments, or model_query {\"query\": \"comments\", \"status\": \"open\"}); after the change one asks for, address it in the same model_command array, {\"cmd\": \"address_comment\", \"id\": \"C3\", \"resolution\": \"<what was done>\"}, so both undo together; resolving is left to the user.\n\
Reference: model_reference gives commands, queries, feature types, their fields and commands' JSON Schemas (no arguments: its contents and every permitted name). How to model well here: model_reference {\"guide\": \"index\"}; unexpected results: {\"guide\": \"bug-reporting\"}.\n\
Units: JSON numbers are millimeters and radians; write \"90 deg\" for degrees. They are millimeters also in a design in another length unit (the document query's units.length): convert the user's lengths, or write them as text, read in that unit (\"0.875\", \"0.875 in\").\n\
Modeling: Place a part or a sketch on an existing face, construction plane, axis or point where there is one (a primitive's at, a sketch's plane), so that it follows the model; a fixed plane ({\"origin\", \"normal\"}) stays where it is written. Unless the user asks otherwise, make every sketch fully constrained and dimensioned: dimensions for the sizes that matter (named parameters where a value is used again) and constraints for the rest, tied to the origin or existing geometry, until the sketch query's dof is 0.\n\
Commands: a model_command array runs in order as one undo step and stops at the first refusal (undoing the entries before it) or failed feature. Each tool's description holds its own rules.";

/// The instructions `initialize` gives: one text per profile, so that a
/// later discovery answer can give the same (mitcad#1506).
fn instructions(profile: Profile) -> &'static str {
    match profile {
        Profile::Headless => HEADLESS_INSTRUCTIONS,
        Profile::Live => LIVE_INSTRUCTIONS,
    }
}

/// Data returned by a document tool. Errors remain visible to the model.
pub struct ToolResult {
    pub value: Value,
    pub is_error: bool,
}

/// Document operations supplied by the application or a mock geometry backend.
pub trait Backend {
    /// An error here is a tool execution failure, rather than a protocol error.
    fn call(&mut self, name: &str, arguments: &Value) -> Result<ToolResult, String>;

    /// The query, command and feature type names MCP permits, which
    /// `model_reference` lists (mitcad#714); none by default.
    fn vocabulary(&self) -> Vocabulary {
        Vocabulary::default()
    }
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

#[derive(Clone, Copy)]
enum Profile {
    Headless,
    Live,
}

enum Dispatch {
    Response(Option<Value>),
    Tool {
        id: Value,
        name: String,
        arguments: Value,
    },
}

impl Dispatch {
    fn response(value: Value) -> Self {
        Self::Response(Some(value))
    }
}

struct Session<B> {
    phase: Phase,
    backend: B,
    profile: Profile,
    vocabulary: Vocabulary,
    /// The client's name (`clientInfo.name`) as comment commands' author
    /// (mitcad#1119).
    author: String,
    /// The user's preference that agents may resolve comments (mitcad#1119);
    /// the live application sets it, the headless server never does.
    may_resolve_comments: bool,
    /// The user's preference that agents may use the print libraries
    /// (mitcad#2224); the live application sets it, the headless server
    /// never does.
    may_use_print_libraries: bool,
    /// Live MCP's input tools (mitcad#1735): ui_click, ui_type and
    /// ui_dialog, which the application allows only in a run started with
    /// --mcp-folder; never in the headless server.
    input_tools: bool,
    /// The resources the client subscribed to (live profile).
    subscriptions: BTreeSet<String>,
    /// The `comments` query's answer as the application last gave it
    /// (live profile): the `mitcad://comments` resource.
    comments: Option<Value>,
    /// The guide step's state as the application last gave it (live
    /// profile, mitcad#2012): the `mitcad://ui-guide` resource.
    guide_step: Option<Value>,
}

impl<B: Backend> Session<B> {
    fn new(backend: B) -> Self {
        let vocabulary = backend.vocabulary();
        Self {
            phase: Phase::Uninitialized,
            backend,
            profile: Profile::Headless,
            vocabulary,
            author: comments::DEFAULT_AUTHOR.to_owned(),
            may_resolve_comments: false,
            may_use_print_libraries: false,
            input_tools: false,
            subscriptions: BTreeSet::new(),
            comments: None,
            guide_step: None,
        }
    }

    fn handle(&mut self, message: Value) -> Option<Value> {
        let response = match self.prepare(message) {
            Dispatch::Response(response) => response,
            Dispatch::Tool {
                id,
                name,
                arguments,
            } => Some(match self.backend.call(&name, &arguments) {
                Ok(mut result) => {
                    if result.is_error {
                        explain_refusal(&name, &arguments, &mut result.value, &self.vocabulary);
                        split_error(&mut result.value);
                    }
                    // A preview image as an image block (mitcad#1486), and
                    // a comment's image (mitcad#2011).
                    if name == "model_render" && !result.is_error {
                        success(&id, image_content(result.value))
                    } else if asks_attachment(&name, &arguments) && !result.is_error {
                        success(&id, attachment_content(result.value))
                    } else {
                        success(&id, tool_content(result.value, result.is_error))
                    }
                }
                Err(message) => {
                    let mut value = json!({"error": message});
                    explain_refusal(&name, &arguments, &mut value, &self.vocabulary);
                    split_error(&mut value);
                    success(&id, tool_content(value, true))
                }
            }),
        };
        response.map(bounded_response)
    }

    fn tools(&self) -> Vec<Value> {
        match self.profile {
            Profile::Headless => catalog::tools(),
            Profile::Live if self.input_tools => {
                let mut tools = catalog::live_tools();
                tools.extend(catalog::input_tools());
                tools
            }
            Profile::Live => catalog::live_tools(),
        }
    }

    fn prepare(&mut self, message: Value) -> Dispatch {
        let Some(object) = message.as_object() else {
            return Dispatch::response(error(None, -32600, "Expected one JSON-RPC object"));
        };
        let id = object.get("id").filter(|id| valid_id(id));
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Dispatch::response(error(id, -32600, "jsonrpc must be 2.0"));
        }
        // No server-to-client requests are issued, so there is no response to
        // consume. Never answer a response with another response.
        if !object.contains_key("method")
            && (object.contains_key("result") || object.contains_key("error"))
        {
            return Dispatch::Response(None);
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Dispatch::response(error(id, -32600, "method must be a string"));
        };
        let notification = !object.contains_key("id");
        if !notification && id.is_none() {
            return Dispatch::response(error(None, -32600, "id must be a string or integer"));
        }
        if object.contains_key("result") || object.contains_key("error") {
            return Dispatch::Response(
                (!notification)
                    .then(|| error(id, -32600, "A request cannot contain result or error")),
            );
        }
        let empty = json!({});
        let params = object.get("params").unwrap_or(&empty);
        if !params.is_object() {
            return Dispatch::Response(
                (!notification).then(|| error(id, -32602, "params must be an object")),
            );
        }
        if notification {
            if method == "notifications/initialized" && self.phase == Phase::AwaitingInitialized {
                self.phase = Phase::Ready;
            }
            // Request-only methods, unknown notifications, and cancellation
            // notifications for completed or uncancellable calls do no work.
            return Dispatch::Response(None);
        }
        let id = id.expect("validated request id");
        if !fits_json(id, MAX_FRAME_BYTES - RESPONSE_RESERVE_BYTES) {
            return Dispatch::response(error(
                None,
                -32600,
                "Request ID leaves no room for a bounded response",
            ));
        }
        // Subscriptions are the live profile's (mitcad#1119): its comments
        // change while a client is connected.
        let live = matches!(self.profile, Profile::Live);
        let result = match method {
            "initialize" => return Dispatch::response(self.initialize(id, params)),
            "ping" => json!({}),
            "resources/subscribe" | "resources/unsubscribe" if !live => {
                return Dispatch::response(error(Some(id), -32601, "Unknown method"));
            }
            "tools/list"
            | "tools/call"
            | "resources/list"
            | "resources/read"
            | "resources/subscribe"
            | "resources/unsubscribe"
            | "resources/templates/list" => {
                if self.phase != Phase::Ready {
                    return Dispatch::response(error(
                        Some(id),
                        -32600,
                        "Send initialize and notifications/initialized before using tools or resources",
                    ));
                }
                match method {
                    "tools/list" => {
                        if let Some(response) = reject_cursor(id, params) {
                            return Dispatch::response(response);
                        }
                        json!({"tools": self.tools()})
                    }
                    "tools/call" => return self.prepare_call(id, params),
                    "resources/list" => {
                        if let Some(response) = reject_cursor(id, params) {
                            return Dispatch::response(response);
                        }
                        let mut resources = vec![json!({
                            "uri": COMMANDS_URI,
                            "name": "Mitcad document API",
                            "description": "JSON command and query reference, identities and metric units",
                            "mimeType": "text/markdown"
                        })];
                        if live {
                            resources.push(comments::resource());
                            resources.push(ui_guide::resource());
                        }
                        // The guides mirrored (mitcad#1508).
                        resources.extend(guide::resources());
                        json!({"resources": resources})
                    }
                    "resources/subscribe" | "resources/unsubscribe" => {
                        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                            return Dispatch::response(error(
                                Some(id),
                                -32602,
                                "uri must be a string",
                            ));
                        };
                        if uri != COMMANDS_URI
                            && uri != COMMENTS_URI
                            && uri != UI_GUIDE_URI
                            && guide::read(uri).is_none()
                        {
                            return Dispatch::response(error(
                                Some(id),
                                -32002,
                                "Unknown resource URI",
                            ));
                        }
                        // The reference and the guides never change while
                        // connected; the comments (mitcad#1119) and the
                        // guide step (mitcad#2012) notify.
                        if method == "resources/subscribe" {
                            self.subscriptions.insert(uri.to_owned());
                        } else {
                            self.subscriptions.remove(uri);
                        }
                        json!({})
                    }
                    "resources/templates/list" => {
                        if let Some(response) = reject_cursor(id, params) {
                            return Dispatch::response(response);
                        }
                        json!({"resourceTemplates": []})
                    }
                    "resources/read" => {
                        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                            return Dispatch::response(error(
                                Some(id),
                                -32602,
                                "uri must be a string",
                            ));
                        };
                        if live && uri == COMMENTS_URI {
                            return Dispatch::response(match &self.comments {
                                Some(answer) => success(id, comments::contents(answer)),
                                None => error(
                                    Some(id),
                                    -32002,
                                    "The application has not given the comments yet; retry",
                                ),
                            });
                        }
                        if live && uri == UI_GUIDE_URI {
                            let state = self.guide_step.clone().unwrap_or_else(ui_guide::idle);
                            return Dispatch::response(success(id, ui_guide::contents(&state)));
                        }
                        if let Some(contents) = guide::read(uri) {
                            return Dispatch::response(success(id, contents));
                        }
                        if uri != COMMANDS_URI {
                            return Dispatch::response(error(
                                Some(id),
                                -32002,
                                "Unknown resource URI",
                            ));
                        }
                        // As agents see it (mitcad#1509).
                        json!({"contents": [{
                            "uri": COMMANDS_URI,
                            "mimeType": "text/markdown",
                            "text": reference::reference_text(&self.vocabulary)
                        }]})
                    }
                    _ => unreachable!(),
                }
            }
            // In particular, a modern client's server/discover probe gets a
            // deterministic method-not-found reply before legacy initialization.
            _ => return Dispatch::response(error(Some(id), -32601, "Unknown method")),
        };
        Dispatch::response(success(id, result))
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
        // Its comment commands carry its name (mitcad#1119).
        self.author = comments::author_name(params["clientInfo"]["name"].as_str().unwrap_or(""));
        // The live comments resource notifies its subscribers (mitcad#1119).
        let resources = match self.profile {
            Profile::Headless => json!({}),
            Profile::Live => json!({"subscribe": true}),
        };
        success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}, "resources": resources},
                "serverInfo": {"name": "mitcad", "version": env!("CARGO_PKG_VERSION")},
                "instructions": instructions(self.profile)
            }),
        )
    }

    fn prepare_call(&mut self, id: &Value, params: &Value) -> Dispatch {
        if params.get("task").is_some() {
            return Dispatch::response(error(
                Some(id),
                -32602,
                "Task-augmented tool calls are not supported",
            ));
        }
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Dispatch::response(error(Some(id), -32602, "tools/call requires a tool name"));
        };
        let Some(tool) = self
            .tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        else {
            return Dispatch::response(error(Some(id), -32602, "Unknown tool name"));
        };
        let empty = json!({});
        let arguments = params.get("arguments").unwrap_or(&empty);
        if !arguments.is_object() {
            return Dispatch::response(error(Some(id), -32602, "arguments must be an object"));
        }
        if let Err(message) = catalog::validate(arguments, &tool["inputSchema"], "arguments") {
            return Dispatch::response(error(Some(id), -32602, &message));
        }
        // What the schema cannot say: ui_select's items or comment
        // (mitcad#1119).
        if let Err(message) = catalog::check(name, arguments) {
            return Dispatch::response(error(Some(id), -32602, &message));
        }
        // The reference is the protocol's own, in both servers (mitcad#714).
        if name == "model_reference" {
            // A command's schema says the server's restrictions
            // (mitcad#1897).
            let live = matches!(self.profile, Profile::Live);
            let content = match reference::answer_in(arguments, &self.vocabulary, live) {
                Ok(answer) => tool_content(answer, false),
                Err(message) => tool_error(message),
            };
            return Dispatch::response(success(id, content));
        }
        let mut arguments = mcp_defaults(name, arguments);
        // Comment commands carry the client's name; resolving waits for the
        // user's preference (mitcad#1119).
        if name == "model_command"
            && let Err(refusal) =
                comments::apply_policy(&mut arguments, &self.author, self.may_resolve_comments)
        {
            return Dispatch::response(success(id, tool_content(refusal, true)));
        }
        // The print libraries wait for the user's preference (mitcad#2224);
        // the headless server never uses them.
        if let Err(refusal) = print_library::apply_policy(
            name,
            &arguments,
            self.may_use_print_libraries,
            matches!(self.profile, Profile::Live),
        ) {
            return Dispatch::response(success(id, tool_content(refusal, true)));
        }
        Dispatch::Tool {
            id: id.clone(),
            name: name.to_owned(),
            arguments,
        }
    }
}

/// What MCP calls default to where the model's own default is too large
/// for a tool response (mitcad#689): `print_thickness` answers its summary
/// only unless `triangles` is given.
fn mcp_defaults(name: &str, arguments: &Value) -> Value {
    let mut arguments = arguments.clone();
    if name == "model_query"
        && let Some(query) = arguments.get_mut("query").and_then(Value::as_object_mut)
        && query.get("query").and_then(Value::as_str) == Some("print_thickness")
        && !query.contains_key("triangles")
    {
        query.insert("triangles".to_owned(), json!(false));
    }
    // A profile's area and centre for agents only (mitcad#1587): the
    // application's own profiles queries leave them out.
    if name == "model_query"
        && let Some(query) = arguments.get_mut("query").and_then(Value::as_object_mut)
        && query.get("query").and_then(Value::as_str) == Some("profiles")
        && !query.contains_key("measures")
    {
        query.insert("measures".to_owned(), json!(true));
    }
    arguments
}

/// A refused `model_command` that misses fields gets the reference of its
/// feature type or command (mitcad#714).
fn explain_refusal(name: &str, arguments: &Value, value: &mut Value, vocabulary: &Vocabulary) {
    if name == "model_command" {
        reference::explain_refusal(arguments, value, vocabulary);
    }
}

struct DeferredBackend;

impl Backend for DeferredBackend {
    fn call(&mut self, _: &str, _: &Value) -> Result<ToolResult, String> {
        Err("live tools must be completed by the application".to_owned())
    }
}

/// A validated message for the live application's asynchronous dispatcher.
pub enum PreparedMessage {
    /// Protocol reply, or no reply for a notification or client response.
    Response(Option<Value>),
    /// One tool invocation to complete with [`LiveSession::finish`].
    Tool { name: String, arguments: Value },
}

impl PreparedMessage {
    /// JSON dispatch plan for the application bridge. Protocol responses
    /// remain serialized text so a host JSON parser cannot round request IDs.
    pub fn into_plan(self) -> Value {
        match self {
            Self::Response(response) => {
                json!({"response": response.map(|response| response.to_string())})
            }
            Self::Tool { name, arguments } => json!({"tool": name, "arguments": arguments}),
        }
    }
}

/// MCP lifecycle and one pending tool call, without owning a CAD document.
/// The application retains ownership and executes tools on its model worker.
pub struct LiveSession {
    session: Session<DeferredBackend>,
    /// The request ID, the tool and the arguments of the call being
    /// completed.
    pending: Option<(Value, String, Value)>,
}

impl Default for LiveSession {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveSession {
    pub fn new() -> Self {
        Self::with_vocabulary(Vocabulary::default())
    }

    /// A session whose `model_reference` lists these names (mitcad#714).
    pub fn with_vocabulary(vocabulary: Vocabulary) -> Self {
        let mut session = Session::new(DeferredBackend);
        session.profile = Profile::Live;
        session.vocabulary = vocabulary;
        Self {
            session,
            pending: None,
        }
    }

    /// Parse one JSON frame using the same errors as the stdio transport.
    pub fn prepare_text(&mut self, message: &str) -> Result<PreparedMessage, String> {
        if self.pending.is_some() {
            return Err("a live MCP tool call is already pending".to_owned());
        }
        if message.len() > MAX_FRAME_BYTES {
            return Ok(PreparedMessage::Response(Some(error(
                None,
                -32600,
                "Message exceeds the 8 MiB input limit",
            ))));
        }
        match serde_json::from_str(message) {
            Ok(message) => self.prepare(message),
            Err(_) => Ok(PreparedMessage::Response(Some(error(
                None,
                -32700,
                "Invalid JSON message",
            )))),
        }
    }

    /// Refuses another message until the previous tool has been completed.
    pub fn prepare(&mut self, message: Value) -> Result<PreparedMessage, String> {
        if self.pending.is_some() {
            return Err("a live MCP tool call is already pending".to_owned());
        }
        Ok(match self.session.prepare(message) {
            Dispatch::Response(response) => {
                PreparedMessage::Response(response.map(bounded_response))
            }
            Dispatch::Tool {
                id,
                name,
                arguments,
            } => {
                self.pending = Some((id, name.clone(), arguments.clone()));
                PreparedMessage::Tool { name, arguments }
            }
        })
    }

    /// The application's comments (mitcad#1119): the `comments` query's
    /// answer for all statuses, which `resources/read` of `mitcad://comments`
    /// returns from now on. When it differs from the last one and the client
    /// subscribed to the resource, the `notifications/resources/updated`
    /// message to send it.
    pub fn set_comments(&mut self, answer: Value) -> Option<Value> {
        let session = &mut self.session;
        if session.comments.as_ref() == Some(&answer) {
            return None;
        }
        session.comments = Some(answer);
        (session.phase == Phase::Ready && session.subscriptions.contains(COMMENTS_URI))
            .then(|| comments::updated(COMMENTS_URI))
    }

    /// The guide step's state (mitcad#2012): what `resources/read` of
    /// `mitcad://ui-guide` returns from now on. When it differs from the last
    /// one and the client subscribed to the resource, the
    /// `notifications/resources/updated` message to send it.
    pub fn set_guide_step(&mut self, state: Value) -> Option<Value> {
        let session = &mut self.session;
        if session.guide_step.as_ref() == Some(&state) {
            return None;
        }
        session.guide_step = Some(state);
        (session.phase == Phase::Ready && session.subscriptions.contains(UI_GUIDE_URI))
            .then(|| comments::updated(UI_GUIDE_URI))
    }

    /// The user's preference Preferences › AI Assistants › Agents may resolve
    /// comments (mitcad#1119): off, `resolve_comment` is refused.
    pub fn set_may_resolve_comments(&mut self, allowed: bool) {
        self.session.may_resolve_comments = allowed;
    }

    /// The user's preference Preferences › AI Assistants › Agents may use
    /// my print libraries (mitcad#2224): off, the print library queries and
    /// commands ([`print_library`]) are refused in words naming it.
    pub fn set_may_use_print_libraries(&mut self, allowed: bool) {
        self.session.may_use_print_libraries = allowed;
    }

    /// Whether the application allows the input tools (mitcad#1735):
    /// ui_click, ui_type and ui_dialog are listed and accepted only then (a
    /// run started with --mcp-folder, not the chat's own connection).
    pub fn set_input_tools(&mut self, allowed: bool) {
        self.session.input_tools = allowed;
    }

    /// Wrap the application's result and release the pending request ID.
    /// `is_error` is the application's refusal; an applied model command
    /// is an error only by [`command_outcome`].
    pub fn finish(&mut self, mut value: Value, is_error: bool) -> Result<Value, String> {
        let (id, name, arguments) = self
            .pending
            .take()
            .ok_or_else(|| "no live MCP tool call is pending".to_owned())?;
        let is_error = if !is_error && name == "model_command" && value.is_object() {
            command_outcome(&mut value)
        } else {
            if is_error {
                explain_refusal(&name, &arguments, &mut value, &self.session.vocabulary);
                split_error(&mut value);
            }
            is_error
        };
        // The live model_render's pictures too (mitcad#1487).
        let content = if matches!(name.as_str(), "ui_screenshot" | "model_render") && !is_error {
            image_content(value)
        } else if asks_attachment(&name, &arguments) && !is_error {
            // A comment's image (mitcad#2011).
            attachment_content(value)
        } else {
            tool_content(value, is_error)
        };
        Ok(bounded_response(success(&id, content)))
    }
}

/// A screenshot as an MCP image block after its text (mitcad#689): the
/// application's `image_base64` leaves the JSON, which keeps the size,
/// the format and `mime_type`. A result without an image (one written to
/// a file) stays text.
fn image_content(mut value: Value) -> Value {
    let mut images = Vec::new();
    if let Some(Value::String(data)) = value
        .as_object_mut()
        .and_then(|fields| fields.remove("image_base64"))
    {
        images.push(data);
    }
    // model_render's views (mitcad#1487): an image block per view, in
    // their order.
    if let Some(views) = value.get_mut("views").and_then(Value::as_array_mut) {
        for view in views {
            if let Some(Value::String(data)) = view
                .as_object_mut()
                .and_then(|fields| fields.remove("image_base64"))
            {
                images.push(data);
            }
        }
    }
    if images.is_empty() {
        return tool_content(value, false);
    }
    let mime = value["mime_type"]
        .as_str()
        .unwrap_or("image/png")
        .to_owned();
    let mut content = vec![json!({"type": "text", "text": value.to_string()})];
    content.extend(
        images
            .into_iter()
            .map(|data| json!({"type": "image", "data": data, "mimeType": mime})),
    );
    json!({
        "content": content,
        "structuredContent": value,
        "isError": false
    })
}

/// Whether a call is `model_query` of `comment_attachment` (mitcad#2011).
fn asks_attachment(name: &str, arguments: &Value) -> bool {
    name == "model_query"
        && arguments.pointer("/query/query").and_then(Value::as_str) == Some("comment_attachment")
}

/// An image of a comment or a reply as an MCP image block after its text
/// (mitcad#2011), as a screenshot's is: the answer's `data` (the file in
/// base64) leaves the JSON, which keeps the image's fields, its
/// `media_type` the block's `mimeType`. An answer without data stays text.
fn attachment_content(mut value: Value) -> Value {
    let data = value
        .get_mut("result")
        .and_then(Value::as_object_mut)
        .and_then(|fields| fields.remove("data"));
    let Some(Value::String(data)) = data else {
        return tool_content(value, false);
    };
    let mime = value["result"]["media_type"]
        .as_str()
        .unwrap_or("image/png")
        .to_owned();
    json!({
        "content": [
            {"type": "text", "text": value.to_string()},
            {"type": "image", "data": data, "mimeType": mime}
        ],
        "structuredContent": value,
        "isError": false
    })
}

/// The outcome of a model command the model applied (mitcad#401): `value`
/// is the tool's result object, with the model's answer in `result`.
///
/// The answer's `failed_features` (every feature in error, see the
/// command reference) go to the top level, an empty list when none failed.
/// The command failed (true: `isError`) only when a feature it added or
/// edited fails; a failure elsewhere in the timeline is listed, not the
/// command's error. That error's `error` text says the feature stays in
/// the timeline and how to remove it (mitcad#1481).
pub fn command_outcome(value: &mut Value) -> bool {
    let failed = value
        .pointer("/result/failed_features")
        .filter(|failed| failed.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    let own: Vec<&Value> = failed
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["edited"] == true)
        .collect();
    let is_error = !own.is_empty();
    if is_error {
        // The short cause first, the exact one beside it (mitcad#1897):
        // each feature's `message` where it has one, its `error` in
        // `error_detail`, and their places.
        let detail = failed_feature_error(value, &own, false);
        let text = failed_feature_error(value, &own, true);
        let places: Vec<Value> = own
            .iter()
            .filter_map(|f| f["places"].as_array())
            .flatten()
            .cloned()
            .collect();
        value["error"] = json!(text);
        if detail != text {
            value["error_detail"] = json!(detail);
        }
        add_places(value, places);
    }
    value["failed_features"] = failed;
    is_error
}

/// The error text of a command whose added or edited features fail
/// (mitcad#1481): the feature is applied and stays, as in the UI, so the
/// text says so and how to remove it. An array's entry is named by its
/// index. With `short`, each feature's cause as a person reads it (its
/// `message`, where it has one), else as agents read it (its `error`).
fn failed_feature_error(value: &Value, own: &[&Value], short: bool) -> String {
    let added = value.pointer("/result/uid");
    let mut text = own
        .iter()
        .map(|f| {
            let uid = f["uid"].as_str().unwrap_or("?");
            let how = if added.and_then(Value::as_str) == Some(uid) {
                "added"
            } else {
                "changed"
            };
            let said = if short { f["message"].as_str() } else { None };
            let cause = said
                .or_else(|| f["error"].as_str())
                .unwrap_or("unknown error");
            format!(
                "{uid} ({}) was {how} but failed: {cause}. It stays in the timeline; edit it, \
                 or remove it with undo or delete_feature",
                f["name"].as_str().unwrap_or(uid),
            )
        })
        .collect::<Vec<_>>()
        .join(". ");
    if value.get("results").is_some() {
        let entry = value["stopped_at"]
            .as_u64()
            .or_else(|| value["ran"].as_u64().and_then(|ran| ran.checked_sub(1)));
        if let Some(i) = entry {
            text = format!("commands[{i}]: {text}");
        }
        if value.get("stopped_at").is_some() {
            text.push_str("; the entries after it did not run");
        }
    }
    text
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
    // A refusal with its places (mitcad#1612, the model's `FeatureError`
    // as text): the short cause in `error`, the exact one in
    // `error_detail`, and its places beside them, not the tail
    // (mitcad#1897).
    let mut value = json!({"error": message});
    split_error(&mut value);
    tool_content(value, true)
}

// Errors in MCP results (mitcad#1897, the command reference's "Errors"):
// the short cause a person reads first, the exact one beside it.

/// A tool error's `error` that carries failures' tails (mitcad#1612,
/// `PLACED_TAIL` in `core/model-base/src/notice.rs`) split in three: `error`
/// becomes the short cause as a person reads it (the text without the
/// tails), `error_detail` the same with each tail's detail, the names, ids
/// and coordinates, in place of the message it explains (the text `error`
/// held before, word for word; only where it differs), and the tails'
/// places are added to `places`. An `error` without a tail is left as it
/// is. The keys' order puts the short cause before its detail in the text.
pub(crate) fn split_error(value: &mut Value) {
    let Some(text) = value.get("error").and_then(Value::as_str) else {
        return;
    };
    let parts = error_parts(text);
    if !parts.tailed {
        return;
    }
    value["error"] = json!(parts.message);
    if parts.detail != parts.message {
        value["error_detail"] = json!(parts.detail);
    }
    add_places(value, parts.places);
}

/// `places` added to a result's `places`, each once.
fn add_places(value: &mut Value, places: Vec<Value>) {
    if places.is_empty() {
        return;
    }
    let mut all = value
        .get("places")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for place in places {
        if !all.contains(&place) {
            all.push(place);
        }
    }
    value["places"] = Value::Array(all);
}

/// A model text read into what a person reads and what agents read.
struct ErrorParts {
    /// The text without its tails.
    message: String,
    /// The text with each tail's detail in place of its message.
    detail: String,
    places: Vec<Value>,
    /// It had a tail.
    tailed: bool,
}

/// The parts of a model text that may carry failures' tails: the text
/// before and after each tail is the person's (as the application's
/// `userText`); each tail's detail in place of the message it explains is
/// the agents' (the model's `FeatureError::agent_text`, for a crate that does
/// not depend on the model); and their places. Other text as it is.
fn error_parts(text: &str) -> ErrorParts {
    const TAIL: &str = "\n[placed: ";
    let mut message = String::new();
    let mut detail = String::new();
    let mut places = Vec::new();
    let mut tailed = false;
    let mut rest = text;
    while let Some(at) = rest.find(TAIL) {
        let json = &rest[at + TAIL.len()..];
        let mut stream = serde_json::Deserializer::from_str(json).into_iter::<Value>();
        let Some((inner, used)) = (match stream.next() {
            Some(Ok(inner)) => Some((inner, stream.byte_offset())),
            _ => None,
        })
        .filter(|(inner, used)| inner.is_object() && json[*used..].starts_with(']')) else {
            break;
        };
        tailed = true;
        let before = &rest[..at];
        message.push_str(before);
        let said = inner["message"].as_str().unwrap_or_default();
        let exact = inner["detail"].as_str().unwrap_or_default();
        match before.strip_suffix(said) {
            _ if exact.is_empty() => detail.push_str(before),
            Some(head) => {
                detail.push_str(head);
                detail.push_str(exact);
            }
            None => {
                detail.push_str(before);
                detail.push_str(" (");
                detail.push_str(exact);
                detail.push(')');
            }
        }
        if let Some(found) = inner["places"].as_array() {
            places.extend(found.iter().cloned());
        }
        rest = &json[used + 1..];
    }
    message.push_str(rest);
    detail.push_str(rest);
    ErrorParts {
        message,
        detail,
        places,
        tailed,
    }
}

/// The agents' text of a model text that may carry failures' tails
/// (mitcad#1612): each tail's detail in place of the message it explains,
/// and their places ([`error_parts`]).
#[cfg(test)]
fn agent_text(text: &str) -> (String, Vec<Value>) {
    let parts = error_parts(text);
    (parts.detail, parts.places)
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

/// Count the actual escaped JSON bytes without allocating a serialized
/// copy. Stop as soon as the transport budget is exceeded.
fn fits_json(value: &Value, maximum: usize) -> bool {
    struct Budget(usize);
    impl Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(io::Error::other("MCP response size limit"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(maximum), value).is_ok()
}

/// The model can return images and histories much larger than a transport
/// frame. Report that as a tool error, without losing the connection or
/// concealing a mutation that has already been applied (mitcad#167).
fn bounded_response(response: Value) -> Value {
    if fits_json(&response, MAX_FRAME_BYTES) {
        return response;
    }
    let id = response.get("id");
    if let Some(original) = response["result"]["structuredContent"].as_object() {
        let applied = original.get("applied").and_then(Value::as_bool);
        let message = if applied == Some(true) {
            "The operation was applied, but its result exceeds the 8 MiB response limit. Do not retry it; query smaller parts of the document."
        } else {
            "The result exceeds the 8 MiB response limit. Request a smaller result."
        };
        let mut compact = json!({
            "error": message,
            "error_code": "response_too_large",
            "result_omitted": true,
            "original_is_error": response["result"]["isError"].as_bool().unwrap_or(false),
        });
        if let Some(applied) = applied {
            compact["applied"] = json!(applied);
        }
        if let Some(document) = original.get("document").and_then(Value::as_str)
            && document.len() <= 128
        {
            compact["document"] = json!(document);
        }
        if let Some(id) = id {
            let compact = success(id, tool_content(compact, true));
            if fits_json(&compact, MAX_FRAME_BYTES) {
                return compact;
            }
        }
    }
    let compact = error(id, -32000, "Response exceeds the 8 MiB limit");
    if fits_json(&compact, MAX_FRAME_BYTES) {
        compact
    } else {
        // Invalid envelopes can supply huge ids before normal request
        // validation. No tool has run for those messages.
        error(None, -32600, "Request ID exceeds the response size limit")
    }
}

#[cfg(test)]
mod guide_tests;
#[cfg(test)]
mod reference_tests;
#[cfg(test)]
mod tests;
