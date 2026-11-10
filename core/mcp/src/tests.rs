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
            "model_reference",
            "model_render"
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
    // The print plates and a slicer's 3MF (mitcad#1490).
    assert_plate_fields(
        tools
            .iter()
            .find(|tool| tool["name"] == "model_export")
            .unwrap(),
    );
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
        // The mesh and replacing a file (mitcad#1569).
        (
            "model_export",
            json!({"document": "doc1", "path": "part.stl", "format": "stl", "overwrite": "yes"}),
        ),
        (
            "model_export",
            json!({"document": "doc1", "path": "part.stl", "format": "stl", "refinement": 1}),
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
    // The reference as agents see it (mitcad#1509).
    let text = response["result"]["contents"][0]["text"].as_str().unwrap();
    assert_eq!(text, reference_text(&Vocabulary::default()));
    assert!(text.starts_with("# MitCad document API\n"));
    assert!(text.contains("\n## Feature definitions\n"));
    assert!(!text.contains(INTERNAL_MARKER));
    assert!(text.len() < COMMANDS_REFERENCE.len());
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
    assert!(instructions.contains("documents open in the Mitcad application"));
    assert!(instructions.contains("Pass document to target a specific design"));
    assert!(instructions.contains("omitting it uses the active design"));
    assert!(instructions.contains("document_list"));
    // References before fixed planes (mitcad#1137).
    assert!(
        instructions.contains("a fixed plane ({\"origin\", \"normal\"}) stays where it is written")
    );
    // Sketches fully constrained and dimensioned (owner 2.11.2026).
    assert!(instructions.contains("make every sketch fully constrained and dimensioned"));
    assert!(instructions.contains("until the sketch query's dof is 0"));
    assert!(!instructions.contains("headless"));
    // A new design beside the open ones (mitcad#1266); its folders, closing
    // and disconnecting are in the tools' descriptions (mitcad#1506).
    assert!(instructions.contains("the last one it made with document_new"));
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
            "model_render",
            "document_save",
            "document_open",
            "ui_state",
            "ui_log",
            "ui_screenshot",
            "ui_select",
            "ui_command",
            "ui_camera",
            "document_new"
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
        assert!(
            !tool["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("document"))
        );
    }
    // Headless clients still choose an explicit document; only live focus
    // supplies a default target.
    for tool in crate::catalog::tools() {
        if tool["inputSchema"]["properties"].get("document").is_some() {
            assert!(
                tool["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("document"))
            );
        }
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
    assert_eq!(save["inputSchema"]["required"], json!([]));
    // The live open optionally targets a window: it replaces that document.
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
    assert_eq!(export["inputSchema"]["required"], json!(["path", "format"]));
    // The print plates and a slicer's 3MF (mitcad#1490), as headless.
    assert_plate_fields(export);
}

/// `model_export` names the print plates and the slicers and takes the
/// 3MF and print bed fields of `export` (mitcad#1490, mitcad#1491).
fn assert_plate_fields(export: &Value) {
    let description = export["description"].as_str().unwrap();
    for text in ["plates", "file_per_plate", "bambu_studio", "Print plates"] {
        assert!(description.contains(text), "{text}: {description}");
    }
    let properties = &export["inputSchema"]["properties"];
    assert_eq!(
        properties["plates"]["enum"],
        json!(["one_file", "file_per_plate"])
    );
    assert_eq!(
        properties["dialect"]["enum"],
        json!([
            "bambu_studio",
            "orcaslicer",
            "qidi_studio",
            "prusaslicer",
            "cura"
        ])
    );
    for field in [
        "plate",
        "on_bed",
        "center",
        "drop",
        "auto_orient",
        "bed_face",
        "spacing",
        "occurrence",
        "occurrences",
        "whole",
    ] {
        assert!(properties.get(field).is_some(), "{field}");
    }
    assert_export_mesh_and_overwrite(export);
}

/// `model_export` takes the `export` command's `refinement` and replaces
/// existing files with `overwrite`, as `document_save` (mitcad#1569).
fn assert_export_mesh_and_overwrite(export: &Value) {
    let description = export["description"].as_str().unwrap();
    for text in ["overwrite: true", "refinement", "Mesh refinement"] {
        assert!(description.contains(text), "{text}: {description}");
    }
    assert!(!description.contains("is refused"), "{description}");
    let properties = &export["inputSchema"]["properties"];
    assert_eq!(properties["overwrite"]["type"], "boolean");
    assert_eq!(properties["overwrite"]["default"], false);
    let refinement = &properties["refinement"];
    assert_eq!(refinement["type"], json!(["string", "object"]));
    for field in ["deviation", "angle", "face_triangles", "max_edge"] {
        assert!(refinement["properties"].get(field).is_some(), "{field}");
    }
    let schema = &export["inputSchema"];
    // With a document handle, which the headless server requires.
    let check = |arguments: &Value| {
        let mut arguments = arguments.clone();
        arguments["document"] = json!("doc1");
        catalog::validate(&arguments, schema, "arguments")
    };
    for arguments in [
        json!({"path": "a.stl", "format": "stl", "refinement": "high", "overwrite": true}),
        json!({"path": "a.stl", "format": "stl",
               "refinement": {"deviation": 0.03, "angle": 0.052}}),
        json!({"path": "a.stl", "format": "stl",
               "refinement": {"deviation": 0.03, "angle": 0.052,
                              "face_triangles": 0, "max_edge": 2}}),
    ] {
        if let Err(error) = check(&arguments) {
            panic!("{arguments}: {error}");
        }
    }
    for (arguments, error) in [
        (
            json!({"path": "a.stl", "format": "stl", "overwrite": "yes"}),
            "arguments.overwrite",
        ),
        (
            json!({"path": "a.stl", "format": "stl", "refinement": 0.03}),
            "arguments.refinement",
        ),
        (
            json!({"path": "a.stl", "format": "stl", "refinement": ""}),
            "arguments.refinement",
        ),
        (
            json!({"path": "a.stl", "format": "stl", "refinement": {"tolerance": 0.03}}),
            "Unknown argument arguments.refinement.tolerance",
        ),
    ] {
        let refused = check(&arguments).unwrap_err();
        assert!(refused.contains(error), "{arguments}: {refused}");
    }
}

#[test]
fn live_ui_tools_validate_their_arguments_before_the_application_runs_them() {
    let mut session = ready_live();
    for (name, arguments) in [
        ("ui_state", json!({"document": ""})),
        ("ui_state", json!({"document": 2})),
        ("ui_log", json!({"since": -1})),
        ("ui_log", json!({"limit": "10"})),
        ("ui_screenshot", json!({"max_size": 10})),
        ("ui_screenshot", json!({"window": "yes"})),
        ("ui_screenshot", json!({"format": "gif"})),
        ("ui_screenshot", json!({"quality": 0})),
        ("ui_screenshot", json!({"width": 8, "height": 600})),
        ("ui_screenshot", json!({"aspect": 20})),
        ("ui_screenshot", json!({"path": ""})),
        ("ui_screenshot", json!({"max_bytes": 100})),
        ("ui_screenshot", json!({"max_bytes": 6 * 1024 * 1024})),
        ("ui_screenshot", json!({"max_bytes": "small"})),
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
        ("ui_camera", json!({"eye": [0, 0]})),
        ("ui_camera", json!({"eye": [0, 0, 1, 2]})),
        ("ui_camera", json!({"target": [0, "a", 1]})),
        ("ui_camera", json!({"height": 0})),
        ("ui_camera", json!({"fov": 200})),
        ("ui_camera", json!({"projection": "fisheye"})),
        ("ui_camera", json!({"box": {"min": [0, 0, 0]}})),
        ("ui_camera", json!({"frame": "all"})),
    ] {
        let response =
            live_response(session.prepare(call(name, arguments.clone())).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{name} {arguments}");
    }
    for (name, arguments) in [
        ("ui_state", json!({})),
        ("ui_state", json!({"document": "live-2"})),
        ("ui_camera", json!({"document": "live-2"})),
        ("ui_command", json!({"id": "sketch.finish"})),
        ("model_query", json!({"query": {"query": "bodies"}})),
        (
            "document_open",
            json!({"path": "x.mitcad", "document": "live-2"}),
        ),
        ("ui_log", json!({"since": 12, "limit": 50})),
        ("ui_screenshot", json!({"window": true, "max_size": 800})),
        (
            "ui_screenshot",
            json!({"path": "shots/view.jpg", "format": "jpeg", "quality": 80, "width": 800,
                   "height": 600, "overwrite": true}),
        ),
        ("ui_screenshot", json!({"aspect": 1.5, "max_size": 1024})),
        (
            "ui_screenshot",
            json!({"max_bytes": 500000, "format": "jpeg"}),
        ),
        (
            "ui_screenshot",
            json!({"path": "shots/view.png", "max_bytes": 4096}),
        ),
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
fn live_document_new_takes_an_optional_name_and_folder() {
    // A new design beside the open ones (mitcad#1266): the application
    // checks the folder and the name.
    let mut session = ready_live();
    let response = live_response(
        session
            .prepare(request(json!(3), "tools/list", json!({})))
            .unwrap(),
    )
    .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    let new = tools
        .iter()
        .find(|tool| tool["name"] == "document_new")
        .unwrap();
    assert_eq!(
        new["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["document", "folder", "name"]
    );
    assert_eq!(new["inputSchema"]["required"], json!([]));
    assert_eq!(new["annotations"]["destructiveHint"], false);
    let description = new["description"].as_str().unwrap();
    assert!(description.contains("New Design") && description.contains("never replaces"));
    // ui_command's refusal of New Design points here.
    let command = tools
        .iter()
        .find(|tool| tool["name"] == "ui_command")
        .unwrap();
    assert!(
        command["description"]
            .as_str()
            .unwrap()
            .contains("document_new")
    );
    for arguments in [
        json!({"name": ""}),
        json!({"folder": ""}),
        json!({"path": "x.mitcad"}),
        json!({"name": 7}),
        json!({"name": "n".repeat(1025)}),
    ] {
        let response = live_response(
            session
                .prepare(call("document_new", arguments.clone()))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32602, "{arguments}");
    }
    for arguments in [
        json!({}),
        json!({"name": "nipple"}),
        json!({"name": "nipple", "folder": "parts", "document": "live-1"}),
    ] {
        assert!(matches!(
            session.prepare(call("document_new", arguments.clone())).unwrap(),
            PreparedMessage::Tool { name, arguments: given }
                if name == "document_new" && given == arguments
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
        json!({"document": "", "command": {"cmd": "undo"}}),
        json!({"document": "live-1", "command": []}),
        json!({"document": "live-1", "command": {}, "extra": true}),
    ] {
        let response =
            live_response(session.prepare(call("model_command", arguments)).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(session.finish(json!({}), false).is_err());
    }
    // The live profile deliberately permits the active-document fallback.
    assert!(matches!(
        session
            .prepare(call("model_command", json!({"command": {"cmd": "undo"}})))
            .unwrap(),
        PreparedMessage::Tool { name, arguments }
            if name == "model_command" && arguments == json!({"command": {"cmd": "undo"}})
    ));
    session.finish(json!({}), false).unwrap();
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
        reference_text(&Vocabulary::default())
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

// The error text of a command whose added or edited feature fails says
// that the feature stays in the timeline and how to remove it
// (mitcad#1481); a failure elsewhere adds no error text.
#[test]
fn command_outcome_says_a_failed_feature_stays_and_how_to_remove_it() {
    let fillet = json!({"uid": "F57", "name": "Fillet3", "error": "fillet failed",
                        "edited": true, "new": true});
    let added = json!({"uid": "F57", "name": "Fillet3", "error": "F57: fillet failed",
                       "failed_features": [fillet]});
    let mut value = json!({"applied": true, "result": added});
    assert!(command_outcome(&mut value));
    assert_eq!(
        value["error"],
        "F57 (Fillet3) was added but failed: fillet failed. It stays in the timeline; \
         edit it, or remove it with undo or delete_feature"
    );

    // An edit_feature (its result has no uid).
    let mut value = json!({"applied": true,
                           "result": {"parameters": [], "failed_features": [fillet]}});
    assert!(command_outcome(&mut value));
    assert_eq!(
        value["error"],
        "F57 (Fillet3) was changed but failed: fillet failed. It stays in the timeline; \
         edit it, or remove it with undo or delete_feature"
    );

    // An array stopped at its entry 1, which added the feature.
    let mut value = json!({"applied": true, "ran": 2, "stopped_at": 1,
                           "results": [{"uid": "F56", "name": "Box1"}, added],
                           "result": added});
    assert!(command_outcome(&mut value));
    assert_eq!(
        value["error"],
        "commands[1]: F57 (Fillet3) was added but failed: fillet failed. It stays in the \
         timeline; edit it, or remove it with undo or delete_feature; the entries after it \
         did not run"
    );

    // The array's last entry: nothing after it to stop.
    let mut value = json!({"applied": true, "ran": 2,
                           "results": [{"uid": "F56", "name": "Box1"}, added],
                           "result": added});
    assert!(command_outcome(&mut value));
    assert_eq!(
        value["error"],
        "commands[1]: F57 (Fillet3) was added but failed: fillet failed. It stays in the \
         timeline; edit it, or remove it with undo or delete_feature"
    );

    // A failure elsewhere: no error text.
    let mut value = json!({"applied": true,
                           "result": {"failed_features": [failed_feature("F3", false)]}});
    assert!(!command_outcome(&mut value));
    assert!(value.get("error").is_none());
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
        json!({"document": "doc1", "command": {"cmd": "recompute"}, "hints": "false"}),
        json!({"document": "doc1", "command": {"cmd": "recompute"}, "hints": null}),
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

/// The application's screenshot of exactly `bytes` encoded bytes within
/// that max_bytes, as `mcpScreenshot` answers it.
fn budget_screenshot(bytes: u64) -> Value {
    let base64 = "A".repeat(bytes.div_ceil(3) as usize * 4);
    json!({"format": "png", "mime_type": "image/png", "of": "view", "width": 4096,
           "height": 2048, "full_width": 4096, "full_height": 2048, "bytes": bytes,
           "downscaled_for_bytes": true, "max_bytes": bytes, "image_base64": base64})
}

#[test]
fn a_screenshot_budget_counts_the_encoded_image_and_a_refusal_keeps_the_session() {
    // max_bytes counts the encoded image (mitcad#1188): the response is its
    // base64 and under 2 KiB more, up to the 5 MiB default within the
    // 8 MiB frame.
    let mut session = ready_live();
    for bytes in [
        catalog::SCREENSHOT_MIN_BYTES,
        500_000,
        catalog::SCREENSHOT_INLINE_BYTES,
    ] {
        session
            .prepare(call("ui_screenshot", json!({"max_bytes": bytes})))
            .unwrap();
        let response = session.finish(budget_screenshot(bytes), false).unwrap();
        assert_eq!(response["result"]["isError"], false, "{bytes}");
        let data = response["result"]["content"][1]["data"].as_str().unwrap();
        assert_eq!(data.len() as u64, bytes.div_ceil(3) * 4);
        let size = response.to_string().len();
        assert!(size > data.len(), "{bytes}");
        assert!(size - data.len() < 2048, "{bytes}: {size}");
        assert!(size <= MAX_FRAME_BYTES);
    }
    // An image that cannot fit is a compact tool error; the next call runs.
    session
        .prepare(call("ui_screenshot", json!({"max_bytes": 4096})))
        .unwrap();
    let refused = json!({"error": "The image of the view takes 5000 bytes as PNG ...",
                         "error_code": "screenshot_too_large", "max_bytes": 4096,
                         "smallest_bytes": 5000, "smallest_width": 64, "smallest_height": 32,
                         "format": "png"});
    let response = session.finish(refused.clone(), true).unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(response["result"]["content"].as_array().unwrap().len(), 1);
    assert_eq!(response["result"]["structuredContent"], refused);
    assert!(matches!(
        session.prepare(call("ui_state", json!({}))).unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let response = session.finish(json!({"mode": "model"}), false).unwrap();
    assert_eq!(response["result"]["isError"], false);
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

// Comments for agents (mitcad#1119): the live comments resource and its
// subscription, ui_select and ui_camera with a comment, the author of
// comment commands and the resolve policy.

fn initialize_as(client: &str) -> Value {
    request(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": client, "version": "1"}
        }),
    )
}

fn ready_live_as(client: &str) -> LiveSession {
    let mut session = LiveSession::new();
    let response = live_response(session.prepare(initialize_as(client)).unwrap()).unwrap();
    assert!(response.get("result").is_some());
    assert!(live_response(session.prepare(initialized()).unwrap()).is_none());
    session
}

/// A `comments` query's answer as the model gives it.
fn comments_answer(status: &str, resolution: Option<&str>) -> Value {
    let open = usize::from(status == "open");
    let revision = if status == "open" { 1 } else { 2 };
    json!({
        "comments": [{
            "id": "C1",
            "text": "make this hole an M6 thread",
            "target": {"body": "F2.b0", "face": "F2:side(c1[c4,c2])"},
            "status": status,
            "orphaned": false,
            "author": "user",
            "resolution": resolution,
            "replies": [],
            "created": "2026-11-01T17:59:00Z",
            "label": "Hole1 › side face",
            "place": [10.0, 5.0, 2.5]
        }],
        "open": open,
        "revision": revision
    })
}

fn live_tool(prepared: PreparedMessage) -> (String, Value) {
    match prepared {
        PreparedMessage::Tool { name, arguments } => (name, arguments),
        PreparedMessage::Response(response) => panic!("expected a tool call, got {response:?}"),
    }
}

#[test]
fn live_comments_resource_is_listed_and_read_from_the_application() {
    let mut session = LiveSession::new();
    let response = live_response(session.prepare(initialize()).unwrap()).unwrap();
    assert_eq!(
        response["result"]["capabilities"],
        json!({"tools": {}, "resources": {"subscribe": true}})
    );
    // The instructions send the agent to the open comments first.
    let instructions = response["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("mitcad://comments"));
    assert!(instructions.contains("address_comment"));
    assert!(instructions.contains("same model_command array"));
    assert!(instructions.contains("ui_state's comments.open"));
    assert!(live_response(session.prepare(initialized()).unwrap()).is_none());
    let list = live_response(
        session
            .prepare(request(json!(3), "resources/list", json!({})))
            .unwrap(),
    )
    .unwrap();
    let uris: Vec<_> = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap())
        .collect();
    // Then the guides (mitcad#1508).
    assert_eq!(uris[..2], [COMMANDS_URI, COMMENTS_URI]);
    assert!(
        uris[2..]
            .iter()
            .all(|uri| uri.starts_with("mitcad://guide/"))
    );
    assert_eq!(
        list["result"]["resources"][1]["mimeType"],
        "application/json"
    );
    let read = request(json!(4), "resources/read", json!({"uri": COMMENTS_URI}));
    // Not given yet: an error, no tool call.
    let response = live_response(session.prepare(read.clone()).unwrap()).unwrap();
    assert_eq!(response["error"]["code"], -32002);
    // The application gives the comments query's answer.
    assert!(
        session
            .set_comments(comments_answer("open", None))
            .is_none()
    );
    let response = live_response(session.prepare(read).unwrap()).unwrap();
    let contents = &response["result"]["contents"][0];
    assert_eq!(contents["uri"], COMMENTS_URI);
    assert_eq!(contents["mimeType"], "application/json");
    let text: Value = serde_json::from_str(contents["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, comments_answer("open", None));
    assert_eq!(text["comments"][0]["label"], "Hole1 › side face");
    assert!(session.finish(json!({}), false).is_err());
    // The headless server has neither the resource nor subscriptions.
    let mut headless = ready();
    let response = headless
        .handle(request(json!(5), "resources/list", json!({})))
        .unwrap();
    assert!(
        !response["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|resource| resource["uri"] == COMMENTS_URI)
    );
    let response = headless
        .handle(request(
            json!(6),
            "resources/read",
            json!({"uri": COMMENTS_URI}),
        ))
        .unwrap();
    assert_eq!(response["error"]["code"], -32002);
    let response = headless
        .handle(request(
            json!(7),
            "resources/subscribe",
            json!({"uri": COMMENTS_URI}),
        ))
        .unwrap();
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn live_comments_subscription_is_notified_on_add_and_on_address() {
    let mut session = ready_live();
    // Before the subscription a change sends nothing.
    assert!(
        session
            .set_comments(json!({"comments": [], "open": 0, "revision": 0}))
            .is_none()
    );
    for (params, code) in [
        (json!({}), -32602),
        (json!({"uri": 3}), -32602),
        (json!({"uri": "mitcad://elsewhere"}), -32002),
    ] {
        let response = live_response(
            session
                .prepare(request(json!(8), "resources/subscribe", params))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], code);
    }
    let response = live_response(
        session
            .prepare(request(
                json!(9),
                "resources/subscribe",
                json!({"uri": COMMENTS_URI}),
            ))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response, json!({"jsonrpc": "2.0", "id": 9, "result": {}}));
    let updated = json!({
        "jsonrpc": "2.0",
        "method": "notifications/resources/updated",
        "params": {"uri": COMMENTS_URI}
    });
    // The user adds a comment.
    assert_eq!(
        session.set_comments(comments_answer("open", None)),
        Some(updated.clone())
    );
    // The same answer again (a refresh that changed nothing): no message.
    assert!(
        session
            .set_comments(comments_answer("open", None))
            .is_none()
    );
    // The agent addresses it.
    assert_eq!(
        session.set_comments(comments_answer("addressed", Some("M6 thread added"))),
        Some(updated)
    );
    let response = live_response(
        session
            .prepare(request(
                json!(10),
                "resources/read",
                json!({"uri": COMMENTS_URI}),
            ))
            .unwrap(),
    )
    .unwrap();
    let text: Value =
        serde_json::from_str(response["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["comments"][0]["status"], "addressed");
    assert_eq!(text["comments"][0]["resolution"], "M6 thread added");
    assert_eq!(text["open"], 0);
    // Unsubscribed: no more messages.
    live_response(
        session
            .prepare(request(
                json!(11),
                "resources/unsubscribe",
                json!({"uri": COMMENTS_URI}),
            ))
            .unwrap(),
    )
    .unwrap();
    assert!(
        session
            .set_comments(comments_answer("resolved", Some("M6 thread added")))
            .is_none()
    );
    // A session that has not finished initializing is not notified.
    let mut early = LiveSession::new();
    live_response(early.prepare(initialize()).unwrap()).unwrap();
    assert!(early.set_comments(comments_answer("open", None)).is_none());
}

#[test]
fn live_ui_state_counts_the_open_comments() {
    let mut session = ready_live();
    let response = live_response(
        session
            .prepare(request(json!(3), "tools/list", json!({})))
            .unwrap(),
    )
    .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    let state = tools
        .iter()
        .find(|tool| tool["name"] == "ui_state")
        .unwrap();
    assert!(
        state["description"]
            .as_str()
            .unwrap()
            .contains("number of open comments (comments.open")
    );
    // The application's count comes back as it gave it.
    let (name, _) = live_tool(session.prepare(call("ui_state", json!({}))).unwrap());
    assert_eq!(name, "ui_state");
    let response = session
        .finish(json!({"mode": "idle", "comments": {"open": 2}}), false)
        .unwrap();
    assert_eq!(
        response["result"]["structuredContent"]["comments"]["open"],
        2
    );
    assert_eq!(response["result"]["isError"], false);
}

#[test]
fn live_ui_select_and_ui_camera_take_a_comment() {
    let mut session = ready_live();
    for (name, arguments) in [
        ("ui_select", json!({"document": "live-1", "comment": "C3"})),
        (
            "ui_select",
            json!({"document": "live-1", "comment": "C3", "reveal": "fit"}),
        ),
        ("ui_camera", json!({"comment": "C3"})),
        (
            "ui_camera",
            json!({"comment": "C3", "aspect": 1.5, "frame": "none"}),
        ),
    ] {
        let (tool, given) = live_tool(session.prepare(call(name, arguments.clone())).unwrap());
        assert_eq!(tool, name);
        assert_eq!(given, arguments);
        session.finish(json!({}), false).unwrap();
    }
    for (name, arguments) in [
        // items or comment, one of them.
        ("ui_select", json!({"document": "live-1"})),
        (
            "ui_select",
            json!({"document": "live-1", "comment": "C3", "items": []}),
        ),
        ("ui_select", json!({"document": "live-1", "comment": ""})),
        ("ui_select", json!({"document": "live-1", "comment": 3})),
        // The comment is what ui_camera frames.
        (
            "ui_camera",
            json!({"comment": "C3", "box": {"min": [0, 0, 0], "max": [1, 1, 1]}}),
        ),
        ("ui_camera", json!({"comment": "C3", "frame": "selection"})),
        ("ui_camera", json!({"comment": "x".repeat(257)})),
    ] {
        let response =
            live_response(session.prepare(call(name, arguments.clone())).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{name} {arguments}");
        assert!(session.finish(json!({}), false).is_err());
    }
}

#[test]
fn comment_commands_are_written_by_the_mcp_client() {
    let mut session = ready_live_as("claude-code");
    let command = json!([
        {"cmd": "add_comment", "text": "check this wall", "target": {"feature": "F7"}, "author": "user"},
        {"cmd": "reply_comment", "id": "C1", "text": "done"},
        {"cmd": "address_comment", "id": "C2", "resolution": "Hole1 is now M6", "author": "someone"},
        {"cmd": "reopen_comment", "id": "C3", "text": "not yet"},
        {"cmd": "reopen_comment", "id": "C4"},
        {"cmd": "edit_comment", "id": "C5", "text": "new text"},
        {"cmd": "add_feature", "def": {"type": "box"}}
    ]);
    let (_, arguments) = live_tool(
        session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command": command}),
            ))
            .unwrap(),
    );
    let entries = arguments["command"].as_array().unwrap();
    for (i, entry) in entries.iter().take(4).enumerate() {
        assert_eq!(entry["author"], "claude-code", "{i}");
    }
    // A reopen without its reply, an edit and other commands are left as
    // they are.
    assert!(entries[4].get("author").is_none());
    assert!(entries[5].get("author").is_none());
    assert!(entries[6].get("author").is_none());
    assert_eq!(entries[0]["text"], "check this wall");
    session.finish(json!({"applied": true}), false).unwrap();
    // One command object too.
    let (_, arguments) = live_tool(
        session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command":
                    {"cmd": "add_comment", "text": "mine", "target": {"parameter": "width"}}}),
            ))
            .unwrap(),
    );
    assert_eq!(arguments["command"]["author"], "claude-code");
    session.finish(json!({"applied": true}), false).unwrap();
    // A client never writes as the user; the name is one printable line.
    for (client, author) in [
        ("user", "agent"),
        ("USER", "agent"),
        ("  ", "agent"),
        ("codex\n mcp\tclient", "codex mcp client"),
    ] {
        let mut session = ready_live_as(client);
        let (_, arguments) = live_tool(
            session
                .prepare(call(
                    "model_command",
                    json!({"document": "live-1", "command":
                        {"cmd": "add_comment", "text": "x", "target": {"feature": "F1"}, "author": "user"}}),
                ))
                .unwrap(),
        );
        assert_eq!(arguments["command"]["author"], author, "{client:?}");
    }
    let long = "n".repeat(500);
    assert_eq!(comments::author_name(&long).chars().count(), 80);
    // The headless server names its client the same way.
    let mut headless = Session::new(MockBackend::default());
    headless.handle(initialize_as("claude-code")).unwrap();
    assert!(headless.handle(initialized()).is_none());
    headless
        .handle(call(
            "model_command",
            json!({"document": "D1", "command":
                {"cmd": "address_comment", "id": "C1", "resolution": "done", "author": "user"}}),
        ))
        .unwrap();
    assert_eq!(
        headless.backend.calls[0].1["command"]["author"],
        "claude-code"
    );
}

#[test]
fn resolve_comment_is_refused_unless_the_user_allows_agents_to_resolve() {
    let mut session = ready_live_as("claude-code");
    let resolve = json!({"cmd": "resolve_comment", "id": "C1"});
    let response = live_response(
        session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command": resolve.clone()}),
            ))
            .unwrap(),
    )
    .unwrap();
    // A tool error, not a call the application runs.
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["error"],
        RESOLVE_REFUSED
    );
    assert_eq!(response["result"]["structuredContent"]["applied"], false);
    assert!(session.finish(json!({}), false).is_err());
    // In an array, nothing of it runs and the entry is named.
    let response = live_response(
        session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command": [
                    {"cmd": "address_comment", "id": "C1", "resolution": "done"},
                    resolve.clone()
                ]}),
            ))
            .unwrap(),
    )
    .unwrap();
    let refusal = &response["result"]["structuredContent"];
    assert_eq!(refusal["entry"], 1);
    assert_eq!(refusal["undone"], 0);
    assert!(
        refusal["error"]
            .as_str()
            .unwrap()
            .starts_with(&format!("commands[1]: {RESOLVE_REFUSED}"))
    );
    // Reopening stays allowed.
    live_tool(
        session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command": {"cmd": "reopen_comment", "id": "C1"}}),
            ))
            .unwrap(),
    );
    session.finish(json!({"applied": true}), false).unwrap();
    // With the preference on, the application runs it.
    session.set_may_resolve_comments(true);
    let (name, arguments) = live_tool(
        session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command": resolve.clone()}),
            ))
            .unwrap(),
    );
    assert_eq!(name, "model_command");
    assert_eq!(arguments["command"], resolve);
    session.finish(json!({"applied": true}), false).unwrap();
    // Off again: refused again.
    session.set_may_resolve_comments(false);
    assert!(
        live_response(
            session
                .prepare(call(
                    "model_command",
                    json!({"document": "live-1", "command": resolve.clone()}),
                ))
                .unwrap(),
        )
        .is_some()
    );
    // The headless server has no such preference: it always refuses.
    let mut headless = ready();
    let response = headless
        .handle(call(
            "model_command",
            json!({"document": "D1", "command": resolve}),
        ))
        .unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert!(headless.backend.calls.is_empty());
}

#[test]
fn ui_command_lists_the_commands_and_the_descriptions_say_how_ids_are_found() {
    // The chat agent guessed command ids and sketch ids (mitcad#1269): the
    // live catalog lists the commands with ui_command's search, and both
    // servers' model_command says how sketch ids are given out (in the
    // descriptions since mitcad#1506).
    let mut session = LiveSession::new();
    let response = live_response(session.prepare(initialize()).unwrap()).unwrap();
    let instructions = response["result"]["instructions"].as_str().unwrap();
    assert!(!instructions.contains("app/COMMANDS.md"));
    assert!(live_response(session.prepare(initialized()).unwrap()).is_none());
    let response = live_response(
        session
            .prepare(request(json!(3), "tools/list", json!({})))
            .unwrap(),
    )
    .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    let command = tools
        .iter()
        .find(|tool| tool["name"] == "ui_command")
        .unwrap();
    let description = command["description"].as_str().unwrap();
    assert!(description.contains("ui_command with search"));
    assert!(description.contains("an unknown id is refused with the nearest ones"));
    let model_command = tools
        .iter()
        .find(|tool| tool["name"] == "model_command")
        .unwrap()["description"]
        .as_str()
        .unwrap();
    assert!(model_command.contains("points, curves and texts share one numbering"));
    assert!(model_command.contains("A point input [x, y] is always a new point"));
    let properties = &command["inputSchema"]["properties"];
    assert_eq!(properties["search"]["type"], "string");
    let search = properties["search"]["description"].as_str().unwrap();
    assert!(search.contains("whether an agent may run it"));
    assert!(search.contains("\"\" lists all"));
    assert!(
        properties["id"]["description"]
            .as_str()
            .unwrap()
            .contains("search lists them")
    );
    // The application lists them.
    for arguments in [
        json!({"document": "live-1", "search": ""}),
        json!({"document": "live-1", "search": "fillet edge"}),
        json!({"document": "live-1"}),
    ] {
        assert!(matches!(
            session.prepare(call("ui_command", arguments.clone())).unwrap(),
            PreparedMessage::Tool { name, arguments: given }
                if name == "ui_command" && given == arguments
        ));
        session.finish(json!({"commands": []}), false).unwrap();
    }
    // A listing runs nothing.
    for arguments in [
        json!({"document": "live-1", "search": "new", "id": "file.new_design"}),
        json!({"document": "live-1", "search": "", "edit": "F1"}),
        json!({"document": "live-1", "search": "x", "inputs": {}}),
        json!({"document": "live-1", "search": 3}),
        json!({"document": "live-1", "search": "n".repeat(1025)}),
    ] {
        let response = live_response(
            session
                .prepare(call("ui_command", arguments.clone()))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32602, "{arguments}");
    }
    // The headless server's model_command says how sketch ids are given
    // out, and nothing names the application's commands.
    let mut headless = Session::new(MockBackend::default());
    let response = headless.handle(initialize()).unwrap();
    let instructions = response["result"]["instructions"].as_str().unwrap();
    assert!(!instructions.contains("ui_command"));
    let tools = catalog::tools();
    let model_command = tools
        .iter()
        .find(|tool| tool["name"] == "model_command")
        .unwrap()["description"]
        .as_str()
        .unwrap();
    assert!(model_command.contains("points, curves and texts share one numbering"));
    assert!(model_command.contains(
        "do not guess the ids a command earlier in the same model_command array will make"
    ));
    assert!(!model_command.contains("ui_command"));
}

/// The text of `tool`'s description in `tools`.
fn description_of<'a>(tools: &'a [Value], tool: &str) -> &'a str {
    tools
        .iter()
        .find(|candidate| candidate["name"] == tool)
        .unwrap_or_else(|| panic!("no tool {tool}"))["description"]
        .as_str()
        .unwrap()
}

#[test]
fn the_instructions_arrive_whole_and_the_moved_rules_are_in_their_tools() {
    // Claude Code keeps only the first 2,048 characters of the instructions
    // (4,096 since v2.1.296) and cut 4,780 of them without a word
    // (mitcad#1506). Both profiles' instructions, as the client decodes
    // them from the initialize result, fit within the shorter cut.
    let mut live = LiveSession::new();
    let response = live_response(live.prepare(initialize()).unwrap()).unwrap();
    let wire = serde_json::to_string(&response).unwrap();
    let received: Value = serde_json::from_str(&wire).unwrap();
    let live_text = received["result"]["instructions"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut headless = Session::new(MockBackend::default());
    let wire = serde_json::to_string(&headless.handle(initialize()).unwrap()).unwrap();
    let received: Value = serde_json::from_str(&wire).unwrap();
    let headless_text = received["result"]["instructions"]
        .as_str()
        .unwrap()
        .to_owned();
    for (profile, text) in [("live", &live_text), ("headless", &headless_text)] {
        let length = text.chars().count();
        assert!(
            length <= INSTRUCTIONS_CHARS,
            "the {profile} instructions are {length} characters, more than {INSTRUCTIONS_CHARS}"
        );
        // What applies across tools, the most important first.
        let mut previous = 0;
        for phrase in [
            "handle",
            "address_comment",
            "model_reference",
            "JSON numbers are millimeters and radians; write \"90 deg\" for degrees",
            "the document query's units.length",
            "Place a part or a sketch on an existing face",
            "make every sketch fully constrained and dimensioned",
            "model_command array runs in order as one undo step and stops at the first refusal",
        ] {
            let at = text
                .find(phrase)
                .unwrap_or_else(|| panic!("{profile}: no {phrase:?}"));
            assert!(at >= previous, "{profile}: {phrase:?} is out of order");
            previous = at;
        }
    }
    // Every tool description arrives whole too.
    let live_tools = catalog::live_tools();
    let headless_tools = catalog::tools();
    for tool in live_tools.iter().chain(&headless_tools) {
        let length = tool["description"].as_str().unwrap().chars().count();
        assert!(
            length <= catalog::DESCRIPTION_CHARS,
            "{}'s description is {length} characters",
            tool["name"]
        );
    }
    // Each rule moved out of the instructions is in the description of the
    // tool it is about: "tool: a key phrase of the rule".
    let moved = [
        // How a sketch's ids are given out (mitcad#1269).
        "model_command: points, curves and texts share one numbering",
        "model_command: do not guess the ids a command earlier in the same model_command array",
        "model_command: A point input [x, y] is always a new point",
        "model_command: hold the two ends together afterwards with a coincident constraint",
        // How the application's command ids are found (mitcad#1269).
        "ui_command: ui_command with search",
        "ui_command: an unknown id is refused with the nearest ones",
        // Busy and retry.
        "model_query: busy and retry_after_ms",
        // The sketch session and read-only documents.
        "model_command: go through the sketch session",
        "model_command: wait until the sketch or command panel is finished",
        "model_command: Editing is refused when the document is read-only",
        // What each ui_* tool shows or does.
        "ui_state: mode (idle, sketch, command, pick_plane)",
        "ui_state: Sketch Palette status and warnings",
        "ui_log: latest log lines",
        "ui_screenshot: as the user sees it",
        "ui_select: to show the user something",
        "ui_select: select that comment's target",
        "ui_command: by its id",
        "ui_camera: for inspection screenshots",
        "ui_camera: comment selects that comment's target",
        // The folders of save, open, new and export.
        "document_save: to its own path",
        "document_save: in a folder the user allows agents to use",
        "document_open: from a folder the user allows agents to use",
        "document_open: in place of its document",
        "document_new: beside the open designs",
        "document_new: the current project's top folder",
        "document_new: an untitled design",
        "document_new: your calls without a document handle go to it",
        "document_list: the folders the user allows agents to save to and open from",
        "model_export: a new file in a folder the user allows agents to use",
        "model_export: the document does not change",
        "ui_screenshot: written to that file in a folder the user allows agents to use",
        // Undo labels.
        "model_command: labeled (agent) in the undo text",
        "model_command: undoes with Ctrl+Z",
        // Handles, closing and disconnecting.
        "document_list: Handles become invalid when their document is closed or replaced",
        "document_list: File imports and closing are not available through this connection",
        "document_list: Disconnecting leaves the application's document open, including unsaved",
        // A failed feature stays in the timeline (mitcad#1481).
        "model_command: stays in the timeline",
        // What a command's result lists.
        "model_command: timeline: full gives the whole timeline and bodies",
        // The whole reference as a resource.
        "model_reference: mitcad://reference/commands is the whole reference",
    ];
    for rule in moved {
        let (tool, phrase) = rule.split_once(": ").unwrap();
        assert!(
            description_of(&live_tools, tool).contains(phrase),
            "live {tool}: no {phrase:?}"
        );
    }
    // The headless model_command carries its share.
    for phrase in [
        "points, curves and texts share one numbering",
        "A point input [x, y] is always a new point",
        "stays in the timeline",
        "timeline: full gives the whole timeline and bodies",
    ] {
        assert!(
            description_of(&headless_tools, "model_command").contains(phrase),
            "headless model_command: no {phrase:?}"
        );
    }
    // A chat agent's design, queries as single objects, and the comments
    // resource's notifications are in the schemas and the resource.
    let model_query = live_tools
        .iter()
        .find(|tool| tool["name"] == "model_query")
        .unwrap();
    assert!(
        model_query["inputSchema"]["properties"]["document"]["description"]
            .as_str()
            .unwrap()
            .contains("chat agents remain bound to their own document")
    );
    assert!(
        model_query["inputSchema"]["properties"]["query"]["description"]
            .as_str()
            .unwrap()
            .contains("One JSON query object")
    );
    assert!(
        comments::resource()["description"]
            .as_str()
            .unwrap()
            .contains("subscribe to hear when they change")
    );
    // Nothing moved is lost from the headless instructions it stayed in.
    assert!(headless_text.contains("Documents last until closed or the server exits"));
    assert!(headless_text.contains("files are confined to the configured workspace"));
}

/// `model_render` (mitcad#1486): the picture as an image block after the
/// text, which keeps everything else; its arguments checked by the schema;
/// read-only. With views (mitcad#1487) an image block per view.
#[test]
fn model_render_answers_an_image_block() {
    let mut session = ready();
    session.backend.result = Some(json!({
        "document": "D1", "mime_type": "image/png", "width": 4, "height": 3,
        "camera": {"eye": [1, -1, 1], "target": [0, 0, 0], "up": [0, 0, 1]},
        "image_base64": "iVBORw0KGgo="
    }));
    let response = session
        .handle(call(
            "model_render",
            json!({"document": "D1", "view": "top", "size": [40, 30]}),
        ))
        .unwrap();
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["content"][1]["type"], "image");
    assert_eq!(result["content"][1]["data"], "iVBORw0KGgo=");
    assert_eq!(result["content"][1]["mimeType"], "image/png");
    assert!(result["structuredContent"].get("image_base64").is_none());
    assert_eq!(
        result["structuredContent"]["camera"]["up"],
        json!([0, 0, 1])
    );
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, result["structuredContent"]);
    // A refusal stays text.
    session.backend.result = Some(json!({"error": "no open document D9"}));
    session.backend.is_error = true;
    let response = session
        .handle(call("model_render", json!({"document": "D9"})))
        .unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(response["result"]["content"].as_array().unwrap().len(), 1);
    // The schema's limits.
    for bad in [
        json!({}),
        json!({"document": "D1", "size": [8, 8]}),
        json!({"document": "D1", "size": [100]}),
        json!({"document": "D1", "projection": "fisheye"}),
        json!({"document": "D1", "max_bytes": 10}),
        json!({"document": "D1", "view": 3}),
        json!({"document": "D1", "unknown": true}),
        json!({"document": "D1", "explode": -1}),
        json!({"document": "D1", "explode": 11}),
        json!({"document": "D1", "colors": ["F2.b0"]}),
        json!({"document": "D1", "highlight": []}),
        json!({"document": "D1", "section": [0, 0, 1]}),
        json!({"document": "D1", "views": []}),
        json!({"document": "D1", "views": vec!["iso"; 9]}),
    ] {
        let response = session.handle(call("model_render", bad.clone())).unwrap();
        assert_eq!(response["error"]["code"], -32602, "{bad}");
    }
    // Several views: an image block per view, in order, each view's
    // fields kept.
    session.backend.is_error = false;
    session.backend.result = Some(json!({
        "document": "D1", "mime_type": "image/png", "bytes": 8,
        "views": [
            {"camera": {"eye": [1, 0, 0]}, "bytes": 4, "image_base64": "AAAA"},
            {"camera": {"eye": [0, 1, 0]}, "bytes": 4, "image_base64": "BBBB"}
        ]
    }));
    let response = session
        .handle(call(
            "model_render",
            json!({"document": "D1", "views": ["iso", {"view": "top", "size": [64, 48]}]}),
        ))
        .unwrap();
    let result = &response["result"];
    let content = result["content"].as_array().unwrap();
    assert_eq!(content.len(), 3);
    assert_eq!(content[1]["data"], "AAAA");
    assert_eq!(content[2]["data"], "BBBB");
    let views = result["structuredContent"]["views"].as_array().unwrap();
    assert!(views.iter().all(|v| v.get("image_base64").is_none()));
    assert_eq!(views[1]["camera"]["eye"], json!([0, 1, 0]));
    let tools = catalog::tools();
    let render = tools.iter().find(|t| t["name"] == "model_render").unwrap();
    assert_eq!(render["annotations"]["readOnlyHint"], true);
    for option in ["colors", "highlight", "section", "explode", "views"] {
        assert!(
            render["inputSchema"]["properties"].get(option).is_some(),
            "{option}"
        );
    }
    // The live server's: the same options without the ray tracer's, the
    // document optional (mitcad#1487).
    let live = catalog::live_tools();
    let render = live.iter().find(|t| t["name"] == "model_render").unwrap();
    let properties = render["inputSchema"]["properties"].as_object().unwrap();
    assert!(properties.contains_key("views") && properties.contains_key("section"));
    assert!(!properties.contains_key("quality") && !properties.contains_key("samples"));
    assert_eq!(render["inputSchema"]["required"], json!([]));
    assert!(
        render["description"]
            .as_str()
            .unwrap()
            .contains("whatever the window's view shows")
    );
    assert!(
        render["description"]
            .as_str()
            .unwrap()
            .contains("overlay draws")
            && properties.contains_key("overlay")
    );
}

/// The live `model_render` (mitcad#1487): the application's pictures as
/// image blocks after the text, as the headless server's.
#[test]
fn live_model_render_answers_image_blocks() {
    let mut session = ready_live();
    assert!(matches!(
        session
            .prepare(call("model_render", json!({"view": "iso"})))
            .unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let response = session
        .finish(
            json!({"document": "W1", "mime_type": "image/png", "bytes": 4,
                   "camera": {"eye": [1, 1, 1]}, "image_base64": "iVBORw=="}),
            false,
        )
        .unwrap();
    let content = response["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[1]["data"], "iVBORw==");
    assert!(
        response["result"]["structuredContent"]
            .get("image_base64")
            .is_none()
    );
    // The ray tracer's arguments are the headless server's.
    let refused = live_response(
        session
            .prepare(call("model_render", json!({"quality": "raytrace"})))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refused["error"]["code"], -32602);
}

/// `model_render`'s overlay (mitcad#1488): a name or an object of a known
/// type with its own settings passes the schema, and the summary reaches
/// the structured content; anything else is refused before the backend.
#[test]
fn model_render_takes_an_overlay() {
    let mut session = ready();
    session.backend.result = Some(json!({
        "document": "D1", "mime_type": "image/png", "image_base64": "iVBORw0KGgo=",
        "overlay": {"type": "overhang", "overhang_area": 400.0}
    }));
    for good in [
        json!("overhang"),
        json!({"type": "overhang", "angle": 60, "up": [0, 0.5, 1]}),
        json!({"type": "thickness", "minimum": 1.2}),
        json!({"type": "draft", "pull": [0, 0, 1], "angle": 2, "color": "#00ff00"}),
        json!({"type": "failed"}),
    ] {
        let response = session
            .handle(call(
                "model_render",
                json!({"document": "D1", "overlay": good.clone()}),
            ))
            .unwrap();
        let result = &response["result"];
        assert_eq!(result["isError"], false, "{good}");
        assert_eq!(
            result["structuredContent"]["overlay"]["overhang_area"],
            400.0
        );
        assert_eq!(result["content"][1]["type"], "image");
    }
    let calls = session.backend.calls.len();
    for bad in [
        json!(3),
        json!({"angle": 45}),
        json!({"type": "curvature"}),
        json!({"type": "overhang", "depth": 1}),
        json!({"type": "overhang", "angle": 95}),
        json!({"type": "draft", "pull": [0, 1]}),
        json!({"type": "failed", "color": "#00ff0000"}),
    ] {
        let response = session
            .handle(call(
                "model_render",
                json!({"document": "D1", "overlay": bad.clone()}),
            ))
            .unwrap();
        assert_eq!(response["error"]["code"], -32602, "{bad}");
    }
    assert_eq!(session.backend.calls.len(), calls);
    let tools = catalog::tools();
    let render = tools.iter().find(|t| t["name"] == "model_render").unwrap();
    let description = render["description"].as_str().unwrap();
    assert!(description.contains("overlay draws what Print Analysis' functions find"));
    assert!(description.contains("quality raytrace"));
    assert!(render["inputSchema"]["properties"]["overlay"]["properties"]["up"].is_object());
}

/// A model refusal with its places (mitcad#1612): agents read its detail,
/// with the names, in place of the plain message, and its places.
#[test]
fn a_refusal_s_tail_becomes_its_detail_and_places() {
    let tail = r#"{"message":"an edge is listed more than once","detail":"edge E{F2:a|F2:b} is listed more than once","places":[{"kind":"edge","name":"E{F2:a|F2:b}"}]}"#;
    let text = format!("commands[1]: an edge is listed more than once\n[placed: {tail}]");
    let (agents, places) = agent_text(&text);
    assert_eq!(
        agents,
        "commands[1]: edge E{F2:a|F2:b} is listed more than once"
    );
    assert_eq!(
        places,
        vec![json!({"kind": "edge", "name": "E{F2:a|F2:b}"})]
    );
    let error = tool_error(text);
    assert_eq!(
        error["structuredContent"]["error"],
        "commands[1]: edge E{F2:a|F2:b} is listed more than once"
    );
    assert_eq!(error["structuredContent"]["places"][0]["kind"], "edge");
    // Plain text as it is.
    assert_eq!(
        agent_text("fillet failed"),
        ("fillet failed".to_owned(), Vec::new())
    );
}
