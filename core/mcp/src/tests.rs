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
            "model_export"
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
