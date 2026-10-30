// SPDX-License-Identifier: MIT
use super::*;
use std::io::{BufReader, Cursor};

#[derive(Default)]
struct MockBackend {
    calls: Vec<(String, Value)>,
    failure: Option<String>,
    result: Option<Value>,
    is_error: bool,
}

impl Backend for MockBackend {
    fn call(&mut self, name: &str, arguments: &Value) -> Result<ToolResult, String> {
        self.calls.push((name.to_owned(), arguments.clone()));
        if let Some(message) = &self.failure {
            return Err(message.clone());
        }
        Ok(ToolResult {
            value: self
                .result
                .clone()
                .unwrap_or_else(|| json!({"document": "doc1", "ok": true})),
            is_error: self.is_error,
        })
    }
}

fn request(id: Value, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn initialize() -> Value {
    request(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }),
    )
}

fn initialized() -> Value {
    json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
}

fn ready() -> Session<MockBackend> {
    let mut session = Session::new(MockBackend::default());
    assert!(
        session
            .handle(initialize())
            .unwrap()
            .get("result")
            .is_some()
    );
    assert!(session.handle(initialized()).is_none());
    session
}

fn call(name: &str, arguments: Value) -> Value {
    request(
        json!(2),
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

#[test]
fn initialization_is_required_and_notification_completes_it() {
    let mut session = Session::new(MockBackend::default());
    assert!(session.handle(initialized()).is_none());
    assert_eq!(
        session.handle(call("document_create", json!({}))).unwrap()["error"]["code"],
        -32600
    );
    let response = session.handle(initialize()).unwrap();
    assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(
        response["result"]["capabilities"],
        json!({"tools": {}, "resources": {}})
    );
    assert_eq!(
        session.handle(call("document_create", json!({}))).unwrap()["error"]["code"],
        -32600
    );
    // An ID-bearing request with a notification method cannot change readiness.
    assert_eq!(
        session
            .handle(request(json!(3), "notifications/initialized", json!({})))
            .unwrap()["error"]["code"],
        -32601
    );
    assert!(session.backend.calls.is_empty());
    assert!(session.handle(initialized()).is_none());
    assert_eq!(
        session.handle(call("document_create", json!({}))).unwrap()["result"]["structuredContent"]
            ["document"],
        "doc1"
    );
    assert_eq!(session.backend.calls.len(), 1);
    assert_eq!(
        session.handle(initialize()).unwrap()["error"]["code"],
        -32600
    );
}

#[test]
fn ping_and_modern_probe_work_before_legacy_initialization() {
    let mut session = Session::new(MockBackend::default());
    assert_eq!(
        session
            .handle(request(json!("ping"), "ping", json!({})))
            .unwrap(),
        json!({"jsonrpc": "2.0", "id": "ping", "result": {}})
    );
    let probe = request(
        json!("discover"),
        "server/discover",
        json!({"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}
        }}),
    );
    assert_eq!(session.handle(probe).unwrap()["error"]["code"], -32601);
    assert_eq!(session.phase, Phase::Uninitialized);
    assert!(
        session
            .handle(initialize())
            .unwrap()
            .get("result")
            .is_some()
    );
}

#[test]
fn initialization_negotiates_only_the_implemented_revision() {
    let mut session = Session::new(MockBackend::default());
    let mut message = initialize();
    message["params"]["protocolVersion"] = json!("2024-11-05");
    let response = session.handle(message).unwrap();
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn malformed_initialization_does_not_advance_lifecycle() {
    for params in [
        json!({}),
        json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": []}),
        json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": {}, "clientInfo": {"name": "test"}}),
        json!({"protocolVersion": 42, "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
    ] {
        let mut session = Session::new(MockBackend::default());
        let response = session
            .handle(request(json!(1), "initialize", params))
            .unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert_eq!(session.phase, Phase::Uninitialized);
    }
}

#[test]
fn notification_tools_and_invalid_notification_params_never_call_backend() {
    let mut session = ready();
    for method in ["tools/call", "initialize", "document_create", "unknown"] {
        assert!(
            session
                .handle(json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": {"name": "document_create", "arguments": {}}
                }))
                .is_none()
        );
    }
    assert!(
        session
            .handle(json!({"jsonrpc": "2.0", "method": "tools/call", "params": []}))
            .is_none()
    );
    assert!(session.backend.calls.is_empty());
    assert_eq!(session.phase, Phase::Ready);
}

#[test]
fn cancellation_for_completed_or_unknown_requests_is_ignored() {
    let mut session = ready();
    session.handle(call("document_create", json!({}))).unwrap();
    for params in [
        json!({"requestId": 2}),
        json!({"requestId": "missing"}),
        json!({}),
    ] {
        assert!(
            session
                .handle(
                    json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": params})
                )
                .is_none()
        );
    }
    assert_eq!(session.backend.calls.len(), 1);
}

#[test]
fn invalid_ids_are_rejected_without_becoming_notifications() {
    let mut session = ready();
    for id in [Value::Null, json!(true), json!(1.5), json!({}), json!([])] {
        let response = session
            .handle(request(
                id,
                "tools/call",
                json!({"name": "document_create"}),
            ))
            .unwrap();
        assert_eq!(response["error"]["code"], -32600);
        assert!(response.get("id").is_none());
    }
    assert!(session.backend.calls.is_empty());
}

#[test]
fn string_negative_and_large_integer_ids_are_preserved() {
    let mut session = ready();
    for id in [json!("request-5"), json!(-9), json!(u64::MAX)] {
        let response = session
            .handle(request(id.clone(), "ping", json!({})))
            .unwrap();
        assert_eq!(response["id"], id);
    }
}

#[test]
fn malformed_envelopes_and_batches_do_no_work() {
    let mut session = ready();
    for message in [
        json!([]),
        json!([call("document_create", json!({}))]),
        json!(42),
        json!({"jsonrpc": "1.0", "id": 8, "method": "tools/call"}),
        json!({"jsonrpc": "2.0", "id": 8, "method": 7}),
        json!({"jsonrpc": "2.0", "id": 8, "method": "tools/call", "result": {}}),
    ] {
        assert_eq!(session.handle(message).unwrap()["error"]["code"], -32600);
    }
    assert_eq!(
        session
            .handle(request(json!(8), "tools/call", Value::Null))
            .unwrap()["error"]["code"],
        -32602
    );
    assert!(session.backend.calls.is_empty());
}

#[test]
fn response_messages_do_not_produce_response_loops() {
    let mut session = ready();
    for message in [
        json!({"jsonrpc": "2.0", "id": 6, "result": {}}),
        json!({"jsonrpc": "2.0", "id": 6, "error": {"code": -32601, "message": "Unknown"}}),
    ] {
        assert!(session.handle(message).is_none());
    }
    assert!(session.backend.calls.is_empty());
}

#[test]
fn discovery_contains_typed_tools_without_unimplemented_capabilities() {
    let mut session = ready();
    let response = session
        .handle(request(json!(7), "tools/list", json!({})))
        .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "document_create",
            "document_open",
            "document_list",
            "document_close",
            "model_query",
            "model_command",
            "document_save",
            "model_export",
            "model_reference"
        ]
    );
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
        assert!(tool.get("outputSchema").is_none());
    }
    let query = tools
        .iter()
        .find(|tool| tool["name"] == "model_query")
        .unwrap();
    assert_eq!(query["annotations"]["readOnlyHint"], true);
    let command = tools
        .iter()
        .find(|tool| tool["name"] == "model_command")
        .unwrap();
    assert_eq!(command["annotations"]["destructiveHint"], true);
    assert!(response["result"].get("nextCursor").is_none());
    assert_eq!(
        session
            .handle(request(
                json!(7),
                "tools/list",
                json!({"cursor": "unexpected"})
            ))
            .unwrap()["error"]["code"],
        -32602
    );
}

#[test]
fn malformed_tool_calls_are_protocol_errors() {
    let mut session = ready();
    for params in [
        json!({}),
        json!({"name": 4}),
        json!({"name": "unknown"}),
        json!({"name": "document_create", "arguments": []}),
    ] {
        let response = session
            .handle(request(json!(2), "tools/call", params))
            .unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(response.get("result").is_none());
    }
    assert!(session.backend.calls.is_empty());
}

#[test]
fn unsupported_task_augmentation_never_executes_a_tool() {
    let mut session = ready();
    let response = session
        .handle(request(
            json!(2),
            "tools/call",
            json!({
                "name": "document_create",
                "arguments": {},
                "task": {"ttl": 60000}
            }),
        ))
        .unwrap();
    assert_eq!(response["error"]["code"], -32602);
    assert!(session.backend.calls.is_empty());
}

#[test]
fn invalid_argument_fields_are_rejected_before_backend_execution() {
    let mut session = ready();
    for (name, arguments) in [
        ("model_query", json!({"query": {}})),
        ("model_command", json!({"document": "doc1", "command": []})),
        ("document_create", json!({"unexpected": true})),
        ("document_open", json!({"path": ""})),
        (
            "document_close",
            json!({"document": "doc1", "discard": "yes"}),
        ),
        (
            "model_export",
            json!({"document": "doc1", "path": "part.bad", "format": "bad"}),
        ),
        (
            "model_export",
            json!({"document": "doc1", "path": "part.step", "format": "step", "bodies": [4]}),
        ),
    ] {
        let response = session.handle(call(name, arguments)).unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["message"].is_string());
        assert!(response.get("result").is_none());
    }
    assert!(session.backend.calls.is_empty());
}

#[test]
fn tool_results_and_backend_failures_have_matching_text_and_json() {
    let mut session = ready();
    let response = session.handle(call("document_create", json!({}))).unwrap();
    let decoded: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(decoded, response["result"]["structuredContent"]);
    assert_eq!(response["result"]["isError"], false);

    session.backend.failure =
        Some("Unknown document handle; open or create a document first".to_owned());
    let response = session
        .handle(call(
            "model_query",
            json!({"document": "missing", "query": {}}),
        ))
        .unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["structuredContent"]["error"]
            .as_str()
            .unwrap()
            .contains("open or create")
    );

    session.backend.failure = None;
    session.backend.result = Some(json!({"error": "Native command failed"}));
    session.backend.is_error = true;
    let response = session.handle(call("document_create", json!({}))).unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["error"],
        "Native command failed"
    );
}

#[test]
fn nonobject_backend_data_is_wrapped_for_the_legacy_result_schema() {
    let mut session = ready();
    session.backend.result = Some(json!([1, 2]));
    let response = session.handle(call("document_list", json!({}))).unwrap();
    assert_eq!(
        response["result"]["structuredContent"],
        json!({"value": [1, 2]})
    );
}

#[test]
fn reference_resource_can_be_discovered_and_read_without_backend_work() {
    let mut session = ready();
    let response = session
        .handle(request(json!(3), "resources/list", json!({})))
        .unwrap();
    assert_eq!(response["result"]["resources"][0]["uri"], COMMANDS_URI);
    let response = session
        .handle(request(
            json!(4),
            "resources/read",
            json!({"uri": COMMANDS_URI}),
        ))
        .unwrap();
    assert_eq!(
        response["result"]["contents"][0]["mimeType"],
        "text/markdown"
    );
    assert_eq!(
        response["result"]["contents"][0]["text"],
        COMMANDS_REFERENCE
    );
    assert_eq!(
        session
            .handle(request(
                json!(5),
                "resources/read",
                json!({"uri": "file:///etc/passwd"}),
            ))
            .unwrap()["error"]["code"],
        -32002
    );
    assert_eq!(
        session
            .handle(request(json!(6), "resources/templates/list", json!({})))
            .unwrap()["result"]["resourceTemplates"],
        json!([])
    );
    assert!(session.backend.calls.is_empty());
}

#[test]
fn stdio_recovers_from_invalid_json_and_utf8_and_emits_only_compact_json() {
    let mut input = b"not JSON\n\xff\n".to_vec();
    for message in [
        initialize(),
        initialized(),
        call("document_create", json!({})),
    ] {
        input.extend_from_slice(message.to_string().as_bytes());
        input.push(b'\n');
    }
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, MockBackend::default()).unwrap();
    let responses: Vec<Value> = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    assert_eq!(responses[0]["error"]["code"], -32700);
    assert_eq!(responses[1]["error"]["code"], -32700);
    assert!(responses[0].get("id").is_none());
    assert_eq!(
        responses[3]["result"]["structuredContent"]["document"],
        "doc1"
    );
    assert_eq!(output.last(), Some(&b'\n'));
}

#[test]
fn framing_bounds_allocation_and_recovers_at_the_next_newline() {
    let input = Cursor::new(b"0123456789\nok\n");
    let mut reader = BufReader::with_capacity(3, input);
    let mut frame = Vec::new();
    assert!(matches!(
        framing::read_frame(&mut reader, &mut frame, 4).unwrap(),
        Some(framing::FrameStatus::TooLarge)
    ));
    assert!(frame.len() <= 4);
    assert!(matches!(
        framing::read_frame(&mut reader, &mut frame, 4).unwrap(),
        Some(framing::FrameStatus::Complete)
    ));
    assert_eq!(frame, b"ok");
    assert!(
        framing::read_frame(&mut reader, &mut frame, 4)
            .unwrap()
            .is_none()
    );
}

#[test]
fn framing_handles_exact_limits_crlf_and_unterminated_eof() {
    let mut reader = BufReader::with_capacity(2, Cursor::new(b"abcd\n{}\r\nlast"));
    let mut frame = Vec::new();
    for expected in [b"abcd".as_slice(), b"{}\r", b"last"] {
        assert!(matches!(
            framing::read_frame(&mut reader, &mut frame, 4).unwrap(),
            Some(framing::FrameStatus::Complete)
        ));
        assert_eq!(frame, expected);
    }
    assert!(
        framing::read_frame(&mut reader, &mut frame, 4)
            .unwrap()
            .is_none()
    );
}

#[test]
fn stdio_oversized_frames_are_drained_before_processing_the_next_request() {
    let mut input = vec![b' '; MAX_FRAME_BYTES + 1];
    input.push(b'\n');
    input.extend_from_slice(request(json!(1), "ping", json!({})).to_string().as_bytes());
    input.push(b'\n');
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, MockBackend::default()).unwrap();
    let responses: Vec<Value> = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["error"]["code"], -32600);
    assert_eq!(responses[1]["id"], 1);
    assert_eq!(responses[1]["result"], json!({}));
}

fn live_response(prepared: PreparedMessage) -> Option<Value> {
    match prepared {
        PreparedMessage::Response(response) => response,
        PreparedMessage::Tool { .. } => panic!("expected a protocol response, not a tool call"),
    }
}

fn ready_live() -> LiveSession {
    let mut session = LiveSession::new();
    assert!(
        live_response(session.prepare(initialize()).unwrap())
            .unwrap()
            .get("result")
            .is_some()
    );
    assert!(live_response(session.prepare(initialized()).unwrap()).is_none());
    session
}

#[test]
fn live_profile_requires_initialization_and_describes_application_ownership() {
    let mut session = LiveSession::new();
    let response =
        live_response(session.prepare(call("document_list", json!({}))).unwrap()).unwrap();
    assert_eq!(response["error"]["code"], -32600);
    assert!(session.finish(json!({}), false).is_err());
    let response = live_response(session.prepare(initialize()).unwrap()).unwrap();
    let instructions = response["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("document already open"));
    assert!(instructions.contains("document_list"));
    assert!(instructions.contains("Disconnecting leaves"));
    assert!(!instructions.contains("headless"));
    assert!(live_response(session.prepare(initialized()).unwrap()).is_none());
    assert!(matches!(
        session.prepare(call("document_list", json!({}))).unwrap(),
        PreparedMessage::Tool { name, arguments }
            if name == "document_list" && arguments == json!({})
    ));
}

#[test]
fn live_catalog_has_only_supported_tools_and_live_handles() {
    let mut session = ready_live();
    let response = live_response(
        session
            .prepare(request(json!(3), "tools/list", json!({})))
            .unwrap(),
    )
    .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "document_list",
            "model_query",
            "model_command",
            "model_export",
            "model_reference",
            "document_save",
            "document_open",
            "ui_state",
            "ui_log",
            "ui_screenshot",
            "ui_select",
            "ui_command",
            "ui_camera"
        ]
    );
    for tool in tools
        .iter()
        .filter(|tool| tool["inputSchema"]["properties"].get("document").is_some())
    {
        let description = tool["inputSchema"]["properties"]["document"]["description"]
            .as_str()
            .unwrap();
        assert!(description.contains("document_list"));
        assert!(!description.contains("document_create"));
    }
    // The live save writes to the document's own path, or with path into a
    // folder the user allows (mitcad#265, mitcad#402); the application
    // checks the folder.
    let save = tools
        .iter()
        .find(|tool| tool["name"] == "document_save")
        .unwrap();
    assert_eq!(
        save["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["document", "overwrite", "path"]
    );
    assert_eq!(save["inputSchema"]["required"], json!(["document"]));
    // The live open takes no handle: it replaces the document.
    let open = tools
        .iter()
        .find(|tool| tool["name"] == "document_open")
        .unwrap();
    assert_eq!(open["inputSchema"]["required"], json!(["path"]));
    assert!(
        open["inputSchema"]["properties"]["path"]["description"]
            .as_str()
            .unwrap()
            .contains("allow")
    );
    for name in ["document_create", "document_close"] {
        let response = live_response(session.prepare(call(name, json!({}))).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(session.finish(json!({}), false).is_err());
    }
    // The live export writes into the folders for designs (mitcad#689).
    let export = tools
        .iter()
        .find(|tool| tool["name"] == "model_export")
        .unwrap();
    assert!(
        export["description"]
            .as_str()
            .unwrap()
            .contains("allows agents")
    );
    // Refused while an edited sketch rolls later features back (mitcad#750).
    assert!(
        export["description"]
            .as_str()
            .unwrap()
            .contains("sketch in the middle of the timeline is edited")
    );
    assert!(
        export["inputSchema"]["properties"]["path"]["description"]
            .as_str()
            .unwrap()
            .contains("allows agents")
    );
    assert_eq!(
        export["inputSchema"]["required"],
        json!(["document", "path", "format"])
    );
}

#[test]
fn live_ui_tools_validate_their_arguments_before_the_application_runs_them() {
    let mut session = ready_live();
    for (name, arguments) in [
        ("ui_log", json!({"since": -1})),
        ("ui_log", json!({"limit": "10"})),
        ("ui_screenshot", json!({"max_size": 10})),
        ("ui_screenshot", json!({"window": "yes"})),
        ("ui_screenshot", json!({"format": "gif"})),
        ("ui_screenshot", json!({"quality": 0})),
        ("ui_screenshot", json!({"width": 8, "height": 600})),
        ("ui_screenshot", json!({"aspect": 20})),
        ("ui_screenshot", json!({"path": ""})),
        ("ui_camera", json!({"aspect": 0})),
        ("ui_select", json!({"document": "live-1"})),
        (
            "ui_select",
            json!({"document": "live-1", "items": [{"kind": "solid", "owner": "F2.b0"}]}),
        ),
        (
            "ui_select",
            json!({"document": "live-1", "items": [{"kind": "face", "owner": "F2.b0", "x": 1}]}),
        ),
        (
            "ui_select",
            json!({"document": "live-1", "items": [], "reveal": "spin"}),
        ),
        ("ui_command", json!({"id": "sketch.finish"})),
        (
            "ui_command",
            json!({"document": "live-1", "finish": "later"}),
        ),
        ("document_save", json!({"document": "live-1", "path": ""})),
        (
            "document_save",
            json!({"document": "live-1", "path": "x.mitcad", "overwrite": "yes"}),
        ),
        ("document_open", json!({})),
        (
            "document_open",
            json!({"path": "x.mitcad", "document": "live-1"}),
        ),
        ("ui_camera", json!({"eye": [0, 0]})),
        ("ui_camera", json!({"eye": [0, 0, 1, 2]})),
        ("ui_camera", json!({"target": [0, "a", 1]})),
        ("ui_camera", json!({"height": 0})),
        ("ui_camera", json!({"fov": 200})),
        ("ui_camera", json!({"projection": "fisheye"})),
        ("ui_camera", json!({"box": {"min": [0, 0, 0]}})),
        ("ui_camera", json!({"frame": "all"})),
        ("ui_camera", json!({"document": "live-1"})),
    ] {
        let response =
            live_response(session.prepare(call(name, arguments.clone())).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{name} {arguments}");
    }
    for (name, arguments) in [
        ("ui_state", json!({})),
        ("ui_log", json!({"since": 12, "limit": 50})),
        ("ui_screenshot", json!({"window": true, "max_size": 800})),
        (
            "ui_screenshot",
            json!({"path": "shots/view.jpg", "format": "jpeg", "quality": 80, "width": 800,
                   "height": 600, "overwrite": true}),
        ),
        ("ui_screenshot", json!({"aspect": 1.5, "max_size": 1024})),
        ("ui_camera", json!({"frame": "shown", "aspect": 1.333})),
        (
            "ui_select",
            json!({"document": "live-1", "items": [{"kind": "sketch_curve", "owner": "F4", "name": "c3"}],
                   "reveal": "fit"}),
        ),
        (
            "ui_command",
            json!({"document": "live-1", "id": "sketch.clean_up_stale_projections",
                   "inputs": {}, "finish": "ok"}),
        ),
        ("document_save", json!({"document": "live-1"})),
        (
            "document_save",
            json!({"document": "live-1", "path": "x.mitcad", "overwrite": true}),
        ),
        (
            "document_open",
            json!({"path": "/designs/x.mitcad", "discard": true}),
        ),
        ("ui_camera", json!({})),
        (
            "ui_camera",
            json!({"eye": [100, -100.5, 80], "target": [0, 0, 0], "up": [0, 0, 1], "height": 90.5,
                   "fov": 30, "projection": "perspective"}),
        ),
        (
            "ui_camera",
            json!({"box": {"min": [0, 0, 0], "max": [10, 10, 10]}, "frame": "none"}),
        ),
        ("ui_camera", json!({"frame": "selection"})),
    ] {
        assert!(matches!(
            session.prepare(call(name, arguments.clone())).unwrap(),
            PreparedMessage::Tool { name: tool, arguments: given } if tool == name && given == arguments
        ));
        session.finish(json!({}), false).unwrap();
    }
}

#[test]
fn live_pending_call_blocks_reentry_and_preserves_its_exact_request_id() {
    let mut session = ready_live();
    let id = json!(u64::MAX);
    let arguments = json!({"document": "live-7", "query": {"query": "bodies"}});
    assert!(matches!(
        session.prepare(request(id.clone(), "tools/call", json!({
            "name": "model_query", "arguments": arguments.clone()
        }))).unwrap(),
        PreparedMessage::Tool { name, arguments: prepared }
            if name == "model_query" && prepared == arguments
    ));
    assert!(
        session
            .prepare(request(json!(99), "ping", json!({})))
            .is_err()
    );
    assert!(session.prepare_text("not JSON").is_err());
    let value = json!({"document": "live-7", "result": [{"uid": "F1.b0"}]});
    let response = session.finish(value.clone(), false).unwrap();
    assert_eq!(response["id"], id);
    assert_eq!(response["result"]["structuredContent"], value);
    assert_eq!(response["result"]["isError"], false);
    assert!(session.finish(json!({}), false).is_err());
    let response = live_response(
        session
            .prepare(request(json!(99), "ping", json!({})))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["id"], 99);
}

#[test]
fn live_failure_result_retains_applied_state_and_releases_pending_call() {
    let mut session = ready_live();
    let mut message = call(
        "model_command",
        json!({
            "document": "live-1", "command": {"cmd": "recompute"}
        }),
    );
    message["id"] = json!("geometry-failed");
    assert!(matches!(
        session.prepare(message).unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let value = json!({"document": "live-1", "applied": true, "error": "failed feature"});
    let response = session.finish(value.clone(), true).unwrap();
    assert_eq!(response["id"], "geometry-failed");
    assert_eq!(response["result"]["structuredContent"], value);
    assert_eq!(response["result"]["isError"], true);
    assert!(matches!(
        session.prepare(call("document_list", json!({}))).unwrap(),
        PreparedMessage::Tool { .. }
    ));
}

#[test]
fn live_notifications_and_invalid_tool_arguments_never_become_pending() {
    let mut session = ready_live();
    for arguments in [
        json!({"command": {"cmd": "undo"}}),
        json!({"document": "live-1", "command": []}),
        json!({"document": "live-1", "command": {}, "extra": true}),
    ] {
        let response =
            live_response(session.prepare(call("model_command", arguments)).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(session.finish(json!({}), false).is_err());
    }
    let mut notification = call(
        "model_command",
        json!({
            "document": "live-1", "command": {"cmd": "undo"}
        }),
    );
    notification.as_object_mut().unwrap().remove("id");
    assert!(live_response(session.prepare(notification).unwrap()).is_none());
    assert!(session.finish(json!({}), false).is_err());
}

#[test]
fn live_sessions_keep_lifecycle_and_pending_requests_independent() {
    let mut first = ready_live();
    let mut second = LiveSession::new();
    first.prepare(call("document_list", json!({}))).unwrap();
    let response =
        live_response(second.prepare(call("document_list", json!({}))).unwrap()).unwrap();
    assert_eq!(response["error"]["code"], -32600);
    assert!(second.finish(json!({}), false).is_err());
    assert_eq!(
        first.finish(json!({"documents": []}), false).unwrap()["id"],
        2
    );
}

#[test]
fn live_text_errors_recover_without_advancing_lifecycle_or_pending_state() {
    let mut session = LiveSession::new();
    let malformed = live_response(session.prepare_text("not JSON").unwrap()).unwrap();
    assert_eq!(malformed["error"]["code"], -32700);
    let oversized = live_response(
        session
            .prepare_text(&" ".repeat(MAX_FRAME_BYTES + 1))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(oversized["error"]["code"], -32600);
    assert!(malformed.get("id").is_none());
    assert!(oversized.get("id").is_none());
    assert!(session.finish(json!({}), false).is_err());
    let response = live_response(session.prepare_text(&initialize().to_string()).unwrap()).unwrap();
    assert!(response.get("result").is_some());
}

#[test]
fn live_resources_reuse_the_model_reference() {
    let mut session = ready_live();
    let response = live_response(
        session
            .prepare(request(
                json!(8),
                "resources/read",
                json!({
                    "uri": COMMANDS_URI
                }),
            ))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        response["result"]["contents"][0]["text"],
        COMMANDS_REFERENCE
    );
    assert!(session.finish(json!({}), false).is_err());
}

#[test]
fn live_dispatch_plans_keep_protocol_responses_as_exact_json_text() {
    let mut session = LiveSession::new();
    let plan = session
        .prepare(request(json!(u64::MAX), "ping", json!({})))
        .unwrap()
        .into_plan();
    let response = plan["response"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(response).unwrap(),
        json!({"jsonrpc": "2.0", "id": u64::MAX, "result": {}})
    );
    assert!(response.contains("18446744073709551615"));
    assert!(plan.get("tool").is_none());
    assert_eq!(
        session.prepare(initialized()).unwrap().into_plan(),
        json!({"response": null})
    );
    let mut session = ready_live();
    assert_eq!(
        session
            .prepare(call("document_list", json!({})))
            .unwrap()
            .into_plan(),
        json!({"tool": "document_list", "arguments": {}})
    );
}

// A model command fails only by the features it added or edited; failed
// features elsewhere are listed beside its result (mitcad#401).

fn failed_feature(uid: &str, edited: bool) -> Value {
    json!({"uid": uid, "name": uid, "error": "fillet failed", "edited": edited, "new": false})
}

#[test]
fn command_outcome_lists_failures_and_fails_only_by_edited_features() {
    let elsewhere = json!([failed_feature("F3", false)]);
    let mut value = json!({"applied": true, "result": {"error": "F3: fillet failed",
                                                       "failed_features": elsewhere}});
    assert!(!command_outcome(&mut value));
    assert_eq!(value["failed_features"], elsewhere);

    let own = json!([failed_feature("F3", false), failed_feature("F5", true)]);
    let mut value = json!({"applied": true, "result": {"failed_features": own}});
    assert!(command_outcome(&mut value));
    assert_eq!(value["failed_features"], own);

    let mut value = json!({"applied": true, "result": {"error": null}});
    assert!(!command_outcome(&mut value));
    assert_eq!(value["failed_features"], json!([]));
}

#[test]
fn live_model_command_is_not_an_error_for_a_failure_elsewhere() {
    let mut session = ready_live();
    let command = |cmd: &str| {
        call(
            "model_command",
            json!({"document": "live-1", "command": {"cmd": cmd}}),
        )
    };
    let elsewhere = json!([failed_feature("F3", false)]);
    assert!(matches!(
        session.prepare(command("add_parameter")).unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let response = session
        .finish(
            json!({"document": "live-1", "applied": true,
                   "result": {"error": "F3: fillet failed", "failed_features": elsewhere}}),
            false,
        )
        .unwrap();
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["structuredContent"]["failed_features"],
        elsewhere
    );

    // The feature the command edited fails: the command's error.
    assert!(matches!(
        session.prepare(command("edit_feature")).unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let own = json!([failed_feature("F3", true)]);
    let response = session
        .finish(
            json!({"document": "live-1", "applied": true,
                   "result": {"error": "F3: fillet failed", "failed_features": own}}),
            false,
        )
        .unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["failed_features"],
        own
    );

    // Other tools' results are passed as they are.
    assert!(matches!(
        session.prepare(call("document_list", json!({}))).unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let response = session.finish(json!({"documents": []}), false).unwrap();
    assert_eq!(
        response["result"]["structuredContent"],
        json!({"documents": []})
    );
}

// Agent modelling (mitcad#689): command arrays, compact command results,
// the print_thickness summary and screenshots as image blocks.

#[test]
fn model_command_takes_an_object_or_an_array_and_a_timeline_mode() {
    let mut session = ready();
    let array = json!([
        {"cmd": "add_parameter", "name": "L", "expression": "40 mm"},
        {"cmd": "add_parameter", "name": "W", "expression": "20 mm"}
    ]);
    for arguments in [
        json!({"document": "doc1", "command": {"cmd": "recompute"}}),
        json!({"document": "doc1", "command": array.clone()}),
        json!({"document": "doc1", "command": array, "timeline": "full"}),
        json!({"document": "doc1", "command": {"cmd": "recompute"}, "timeline": "none"}),
    ] {
        let response = session
            .handle(call("model_command", arguments.clone()))
            .unwrap();
        assert!(response.get("result").is_some(), "{response}");
        assert_eq!(session.backend.calls.last().unwrap().1, arguments);
    }
    for arguments in [
        json!({"document": "doc1", "command": []}),
        json!({"document": "doc1", "command": [{"cmd": "recompute"}, 4]}),
        json!({"document": "doc1", "command": "recompute"}),
        json!({"document": "doc1", "command": {"cmd": "recompute"}, "timeline": "all"}),
    ] {
        let response = session.handle(call("model_command", arguments)).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{response}");
    }
    assert_eq!(session.backend.calls.len(), 4);
    let tools = catalog::tools();
    let command = tools
        .iter()
        .find(|tool| tool["name"] == "model_command")
        .unwrap();
    assert!(
        !command["description"]
            .as_str()
            .unwrap()
            .contains("not supported through this tool. Script arrays")
    );
    assert_eq!(
        command["inputSchema"]["properties"]["command"]["type"],
        json!(["object", "array"])
    );
}

#[test]
fn print_thickness_answers_its_summary_over_mcp_unless_triangles_are_asked() {
    let mut session = ready();
    session
        .handle(call(
            "model_query",
            json!({"document": "doc1", "query": {"query": "print_thickness", "minimum": 0.4}}),
        ))
        .unwrap();
    assert_eq!(
        session.backend.calls.last().unwrap().1["query"],
        json!({"query": "print_thickness", "minimum": 0.4, "triangles": false})
    );
    for query in [
        json!({"query": "print_thickness", "triangles": true, "max_triangles": 100}),
        json!({"query": "print_overhangs"}),
        json!({"query": "bodies"}),
    ] {
        session
            .handle(call(
                "model_query",
                json!({"document": "doc1", "query": query.clone()}),
            ))
            .unwrap();
        assert_eq!(session.backend.calls.last().unwrap().1["query"], query);
    }
    // The live server's query gets the same default.
    let mut live = ready_live();
    assert!(matches!(
        live.prepare(call("model_query", json!({"document": "live-1",
            "query": {"query": "print_thickness"}}))).unwrap(),
        PreparedMessage::Tool { arguments, .. }
            if arguments["query"]["triangles"] == false
    ));
}

#[test]
fn live_screenshots_are_image_blocks_after_their_text() {
    let mut session = ready_live();
    assert!(matches!(
        session
            .prepare(call("ui_screenshot", json!({"max_size": 640})))
            .unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let response = session
        .finish(
            json!({"format": "png", "mime_type": "image/png", "width": 640, "height": 480,
                   "bytes": 4, "image_base64": "iVBORw=="}),
            false,
        )
        .unwrap();
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    let content = result["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    let text: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, result["structuredContent"]);
    assert!(result["structuredContent"].get("image_base64").is_none());
    assert_eq!(result["structuredContent"]["width"], 640);
    assert_eq!(
        content[1],
        json!({"type": "image", "data": "iVBORw==", "mimeType": "image/png"})
    );
    // Written to a file: the path and the size, no image.
    session
        .prepare(call(
            "ui_screenshot",
            json!({"path": "view.jpg", "format": "jpeg"}),
        ))
        .unwrap();
    let written = json!({"format": "jpeg", "mime_type": "image/jpeg", "path": "/agents/view.jpg",
                         "bytes": 1234, "width": 640, "height": 480});
    let response = session.finish(written.clone(), false).unwrap();
    assert_eq!(response["result"]["content"].as_array().unwrap().len(), 1);
    assert_eq!(response["result"]["structuredContent"], written);
}

/// A timeline of `n` features and `bodies` bodies, as the queries answer.
fn snapshot(features: usize, bodies: usize) -> Value {
    let features: Vec<Value> = (1..=features)
        .map(|i| {
            json!({"uid": format!("F{i}"), "name": format!("Feature{i}"), "type": "extrude",
                   "suppressed": false, "status": "ok", "error": null, "sketches": [format!("F{}", i + 1000)]})
        })
        .collect();
    let bodies: Vec<Value> = (1..=bodies)
        .map(|i| {
            json!({"uid": format!("F{i}.b0"), "name": format!("Body{i}"), "visible": true,
                   "material": "steel", "appearance": null, "version": format!("{i:032x}")})
        })
        .collect();
    json!({
        "state": {"features": features.len(), "undo": "Add Feature100", "undo_depth": 100},
        "timeline": {"marker": features.len(), "features": features},
        "bodies": bodies
    })
}

#[test]
fn a_command_that_recomputes_nothing_answers_only_what_it_changed() {
    // About 100 features: set_body_visible changes one body's attributes
    // and recomputes nothing.
    let before = snapshot(100, 40);
    let mut after = before.clone();
    after["bodies"][3]["visible"] = json!(false);
    after["state"]["undo"] = json!("Hide Body4");
    let compact = Value::Object(command_document(
        TimelineMode::Changed,
        Some(&before),
        &after,
        &[],
    ));
    let changes = &compact["changes"];
    assert_eq!(changes["features"], json!([]));
    assert_eq!(changes["removed_features"], json!([]));
    assert_eq!(changes["removed_bodies"], json!([]));
    let bodies = changes["bodies"].as_array().unwrap();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["uid"], "F4.b0");
    assert_eq!(bodies[0]["change"], "changed");
    assert_eq!(bodies[0]["visible"], false);
    assert!(bodies[0].get("version").is_none());
    assert_eq!(changes["feature_count"], 100);
    assert_eq!(changes["body_count"], 40);
    assert_eq!(compact["state"]["undo"], "Hide Body4");
    assert!(compact.get("timeline").is_none() && compact.get("bodies").is_none());
    // The old full answer is there when asked for, and much larger.
    let full = Value::Object(command_document(TimelineMode::Full, None, &after, &[]));
    assert_eq!(full["timeline"], after["timeline"]);
    assert_eq!(full["bodies"].as_array().unwrap().len(), 40);
    assert!(full["bodies"][0].get("version").is_none());
    let (small, large) = (compact.to_string().len(), full.to_string().len());
    assert!(
        small < 1024 && large > 10 * 1024,
        "compact {small} bytes, full {large}"
    );
    let none = Value::Object(command_document(TimelineMode::None, None, &after, &[]));
    assert_eq!(none, json!({"state": after["state"]}));
}

#[test]
fn changes_list_added_changed_recomputed_failed_and_removed_items() {
    let before = snapshot(5, 3);
    let mut after = snapshot(5, 3);
    // F6 added with a body; F2's name changed; F3 recomputed alike; F4
    // left failed from before; F5 removed; F3's body removed and F1's
    // changed.
    let features = after["timeline"]["features"].as_array_mut().unwrap();
    features[1]["name"] = json!("Renamed");
    features.remove(4);
    features.push(
        json!({"uid": "F6", "name": "Box1", "type": "box", "suppressed": false,
                         "status": "ok", "error": null}),
    );
    let mut before = before;
    before["timeline"]["features"][3]["status"] = json!("error");
    after["timeline"]["features"][3]["status"] = json!("error");
    after["bodies"][0]["version"] = json!("ff");
    let bodies = after["bodies"].as_array_mut().unwrap();
    bodies.pop();
    bodies.push(json!({"uid": "F6.b0", "name": "Body6", "visible": true, "version": "1"}));
    let compact = Value::Object(command_document(
        TimelineMode::Changed,
        Some(&before),
        &after,
        &["F3".to_owned(), "F6".to_owned()],
    ));
    let changes = &compact["changes"];
    let listed: Vec<(String, String)> = changes["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["uid"].as_str().unwrap().to_owned(),
                f["change"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("F2".to_owned(), "changed".to_owned()),
            ("F3".to_owned(), "recomputed".to_owned()),
            ("F4".to_owned(), "failed".to_owned()),
            ("F6".to_owned(), "added".to_owned()),
        ]
    );
    assert_eq!(changes["removed_features"], json!(["F5"]));
    let bodies: Vec<(&str, &str)> = changes["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["uid"].as_str().unwrap(), b["change"].as_str().unwrap()))
        .collect();
    assert_eq!(bodies, [("F1.b0", "changed"), ("F6.b0", "added")]);
    assert_eq!(changes["removed_bodies"], json!(["F3.b0"]));
    assert_eq!(changes["marker"], 5);
}

#[test]
fn changes_give_the_boxes_of_the_bodies_a_split_left() {
    // mitcad#714: the snapshot reads every body's box, and the changed and
    // added bodies carry it, so a split's pieces show their sides.
    let (_, bodies_query) = SNAPSHOT_QUERIES
        .iter()
        .find(|(key, _)| *key == "bodies")
        .unwrap();
    let bodies_query: Value = serde_json::from_str(bodies_query).unwrap();
    assert_eq!(bodies_query["bbox"], true);
    assert_eq!(bodies_query["versions"], true);

    let mut before = snapshot(1, 1);
    before["bodies"][0]["bbox"] = json!({"min": [0, 0, 0], "max": [2012, 600, 18]});
    let mut after = snapshot(2, 1);
    after["bodies"][0]["version"] = json!("ff");
    after["bodies"][0]["bbox"] = json!({"min": [0, 0, 0], "max": [1000, 600, 18]});
    after["bodies"].as_array_mut().unwrap().push(
        json!({"uid": "F2.b1", "name": "Body2", "visible": true, "version": "1",
                     "bbox": {"min": [1000, 0, 0], "max": [2012, 600, 18]}}),
    );
    let compact = Value::Object(command_document(
        TimelineMode::Changed,
        Some(&before),
        &after,
        &["F2".to_owned()],
    ));
    let bodies = compact["changes"]["bodies"].as_array().unwrap();
    assert_eq!(bodies.len(), 2);
    assert_eq!(
        (&bodies[0]["uid"], &bodies[0]["change"]),
        (&json!("F1.b0"), &json!("changed"))
    );
    assert_eq!(bodies[0]["bbox"]["max"], json!([1000, 600, 18]));
    assert_eq!(
        (&bodies[1]["uid"], &bodies[1]["change"]),
        (&json!("F2.b1"), &json!("added"))
    );
    assert_eq!(bodies[1]["bbox"]["min"], json!([1000, 0, 0]));
    assert!(bodies.iter().all(|b| b.get("version").is_none()));
}

#[test]
fn array_entries_stop_on_their_own_failed_features_and_recomputes_name_features() {
    assert!(entry_failed(
        &json!({"failed_features": [failed_feature("F5", true)]})
    ));
    assert!(!entry_failed(
        &json!({"failed_features": [failed_feature("F5", false)]})
    ));
    assert!(!entry_failed(&json!({"recomputed": 3, "error": null})));
    assert!(recomputed(&json!({"recomputed": 2})));
    assert!(!recomputed(&json!({"recomputed": 0})));
    let times = [
        json!({"features": [{"uid": "F2", "ms": 1.0}, {"uid": "F3", "ms": 2.0}], "ms": 3.0}),
        json!({"features": [{"uid": "F3", "ms": 1.0}, {"uid": "F4", "ms": 1.0}], "ms": 2.0}),
    ];
    assert_eq!(evaluated_features(&times), ["F2", "F3", "F4"]);
    assert_eq!(TimelineMode::of(&json!({})), Ok(TimelineMode::Changed));
    assert_eq!(
        TimelineMode::of(&json!({"timeline": "full"})),
        Ok(TimelineMode::Full)
    );
    assert_eq!(
        TimelineMode::of(&json!({"timeline": "none"})),
        Ok(TimelineMode::None)
    );
    assert!(TimelineMode::of(&json!({"timeline": "all"})).is_err());
    assert!(TimelineMode::Changed.needs_before() && !TimelineMode::Full.needs_before());
}

#[test]
fn the_validator_takes_a_list_of_types() {
    let schema = json!({"type": ["object", "array"], "items": {"type": "object"}, "minItems": 1});
    assert!(catalog::validate(&json!({"cmd": "x"}), &schema, "command").is_ok());
    assert!(catalog::validate(&json!([{"cmd": "x"}]), &schema, "command").is_ok());
    assert!(catalog::validate(&json!([]), &schema, "command").is_err());
    assert!(catalog::validate(&json!([1]), &schema, "command").is_err());
    assert!(catalog::validate(&json!("x"), &schema, "command").is_err());
}

/// mitcad#935: a client's megabyte strings are refused by the catalog's
/// `maxLength` before the application runs anything, and no error repeats
/// them whole.
#[test]
fn long_client_strings_are_refused_and_never_echoed_whole() {
    let mut session = ready_live();
    let huge = "x".repeat(7 * 1024 * 1024);
    for _ in 0..3 {
        let arguments = json!({"document": "live-1",
            "items": [{"kind": "face", "owner": huge.as_str(), "name": "x"}]});
        let response =
            live_response(session.prepare(call("ui_select", arguments)).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602);
        let message = response["error"]["message"].as_str().unwrap();
        assert!(message.contains("owner must have at most"), "{message}");
        assert!(message.len() < 256, "{}", message.len());
    }
    // The limits of the ids, the names and the paths.
    for (name, arguments) in [
        (
            "ui_command",
            json!({"document": "live-1", "id": "x".repeat(257)}),
        ),
        (
            "ui_command",
            json!({"document": "live-1", "edit": "F".repeat(257)}),
        ),
        (
            "ui_select",
            json!({"document": "x".repeat(257), "items": []}),
        ),
        (
            "ui_select",
            json!({"document": "live-1",
                   "items": [{"kind": "face", "owner": "F2.b0", "name": "n".repeat(4097)}]}),
        ),
        (
            "ui_select",
            json!({"document": "live-1",
                   "items": [{"kind": "body", "owner": "F2.b0", "occurrence": "O".repeat(1025)}]}),
        ),
        ("document_open", json!({"path": "p".repeat(4097)})),
        ("model_reference", json!({"search": "s".repeat(1025)})),
    ] {
        let response =
            live_response(session.prepare(call(name, arguments.clone())).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{name}");
        assert!(response["error"]["message"].as_str().unwrap().len() < 256);
    }
    // At the limits they pass; a limit counts characters, not bytes.
    for (name, arguments) in [
        (
            "ui_command",
            json!({"document": "live-1", "id": "x".repeat(256)}),
        ),
        (
            "ui_select",
            json!({"document": "live-1",
                   "items": [{"kind": "face", "owner": "ä".repeat(1024), "name": "n".repeat(4096)}]}),
        ),
    ] {
        assert!(matches!(
            session.prepare(call(name, arguments)).unwrap(),
            PreparedMessage::Tool { .. }
        ));
        session.finish(json!({}), false).unwrap();
    }
    // An unknown argument's name is cut too.
    let mut unknown = serde_json::Map::new();
    unknown.insert("k".repeat(100_000), json!(1));
    let response = live_response(
        session
            .prepare(call("ui_state", Value::Object(unknown)))
            .unwrap(),
    )
    .unwrap();
    let message = response["error"]["message"].as_str().unwrap();
    assert!(
        message.starts_with("Unknown argument arguments.kkk"),
        "{message}"
    );
    assert!(message.len() < 512, "{}", message.len());
}
