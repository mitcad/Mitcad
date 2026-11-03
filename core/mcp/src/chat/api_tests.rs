// SPDX-License-Identifier: MIT
//! The API agents' stream (mitcad#947, phase 2): MitCad's MCP connection,
//! the Anthropic Messages API's and an OpenAI-compatible endpoint's streamed
//! answers, the tool loop, Stop, refusals, account failures, models that
//! cannot call tools, the kept conversation and the endpoint addresses.

use serde_json::{Value, json};

use super::api::{ApiStream, endpoint, local};
use super::{Stream, request};

const SESSION: &str = "0b5c7c1e-2f1a-4c3d-9e8f-1a2b3c4d5e6f";

fn kinds(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .map(|event| event["kind"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn line_of(event: &Value) -> Value {
    serde_json::from_str(event["line"].as_str().unwrap().trim_end()).unwrap()
}

fn body_of(event: &Value) -> Value {
    serde_json::from_str(event["body"].as_str().unwrap()).unwrap()
}

fn control(stream: &mut ApiStream, value: Value) -> Vec<Value> {
    stream.control(&value).unwrap()
}

/// A stream started and connected to MitCad's listener: its tools are
/// there and it is ready for a message.
fn connected(agent: &str, url: &str, model: &str) -> ApiStream {
    let mut stream = ApiStream::new(agent).unwrap();
    let started = control(
        &mut stream,
        json!({"op": "start", "name": "Test API", "url": url, "model": model, "session": SESSION}),
    );
    assert_eq!(kinds(&started), ["mcp"]);
    let initialize = line_of(&started[0]);
    assert_eq!(initialize["method"], "initialize");
    assert_eq!(initialize["jsonrpc"], "2.0");
    assert_eq!(
        initialize["params"]["clientInfo"]["name"],
        "MitCad chat (Test API)"
    );
    let id = initialize["id"].clone();
    let answered = stream.feed(
        &json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": crate::PROTOCOL_VERSION,
               "instructions": "Use model_command for changes."}})
        .to_string(),
    );
    assert_eq!(kinds(&answered), ["mcp", "mcp"]);
    assert_eq!(line_of(&answered[0])["method"], "notifications/initialized");
    let list = line_of(&answered[1]);
    assert_eq!(list["method"], "tools/list");
    let ready = stream.feed(
        &json!({"jsonrpc": "2.0", "id": list["id"], "result": {"tools": [
            {"name": "model_command", "description": "Apply a command",
             "inputSchema": {"type": "object", "properties": {"command": {}}}},
            {"name": "ui_screenshot", "description": "A picture",
             "inputSchema": {"type": "object"}}]}})
        .to_string(),
    );
    assert_eq!(kinds(&ready), ["init"]);
    assert_eq!(ready[0]["bridge"], "connected");
    assert_eq!(ready[0]["session"], SESSION);
    assert_eq!(ready[0]["resume_failed"], false);
    stream
}

fn sse(stream: &mut ApiStream, id: u64, lines: &[&str]) -> Vec<Value> {
    let mut events = Vec::new();
    for line in lines {
        events.extend(control(
            stream,
            json!({"op": "http_line", "id": id, "status": 200, "line": line}),
        ));
    }
    events
}

fn anthropic_event(name: &str, data: Value) -> Vec<String> {
    vec![
        format!("event: {name}"),
        format!("data: {data}"),
        String::new(),
    ]
}

fn lines(events: &[Vec<String>]) -> Vec<String> {
    events.iter().flatten().cloned().collect()
}

#[test]
fn endpoint_addresses_are_https_or_this_computer() {
    assert_eq!(
        endpoint("https://api.anthropic.com/").unwrap(),
        "https://api.anthropic.com"
    );
    assert_eq!(
        endpoint(" http://localhost:11434/v1 ").unwrap(),
        "http://localhost:11434/v1"
    );
    assert!(endpoint("http://127.0.0.1:1234/v1").is_ok());
    assert!(endpoint("http://[::1]:8080/v1").is_ok());
    assert!(endpoint("HTTP://LOCALHOST:1/v1").is_ok());
    assert!(endpoint("http://example.com/v1").is_err());
    assert!(endpoint("http://localhost.example.com/v1").is_err());
    assert!(endpoint("https://user:secret@example.com/v1").is_err());
    assert!(endpoint("https://example.com/v1?key=1").is_err());
    assert!(endpoint("ftp://example.com").is_err());
    assert!(endpoint("example.com").is_err());
    assert!(endpoint("").is_err());
    assert!(endpoint("https://exa mple.com").is_err());
    assert!(local("http://localhost:11434/v1"));
    assert!(local("http://127.0.0.1:1234"));
    assert!(!local("https://api.anthropic.com"));
    let checked =
        request(&json!({"op": "api_endpoint", "url": "http://localhost:1234/v1/"})).unwrap();
    assert_eq!(checked["url"], "http://localhost:1234/v1");
    assert_eq!(checked["local"], true);
    assert!(request(&json!({"op": "api_endpoint", "url": "http://10.0.0.2/v1"})).is_err());
}

#[test]
fn presets_and_streams_of_the_api_agents() {
    let presets = request(&json!({"op": "api_presets"})).unwrap();
    let ids: Vec<&str> = presets["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| agent["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["anthropic", "ollama", "lmstudio", "openai"]);
    assert_eq!(presets["agents"][1]["url"], "http://localhost:11434/v1");
    assert_eq!(presets["agents"][0]["key"], "required");
    for agent in ids {
        assert!(matches!(Stream::new(agent), Ok(Stream::Api(_))));
    }
    assert!(Stream::new("unknown").is_err());
    let offered = request(&json!({"op": "api_default_models", "agent": "anthropic"})).unwrap();
    assert_eq!(offered["models"][0]["id"], "claude-opus-5-5");
    let none = request(&json!({"op": "api_default_models", "agent": "ollama"})).unwrap();
    assert_eq!(none["models"], json!([]));
}

#[test]
fn start_needs_an_allowed_address_a_model_and_a_session() {
    let mut stream = ApiStream::new("ollama").unwrap();
    let start = |url: &str, model: &str, session: &str| json!({"op": "start", "url": url, "model": model, "session": session});
    assert!(
        stream
            .control(&start("http://example.com/v1", "llama3.1", SESSION))
            .is_err()
    );
    // A local model needs its name; the Anthropic API has a default.
    assert!(
        stream
            .control(&start("http://localhost:11434/v1", "", SESSION))
            .is_err()
    );
    assert!(
        stream
            .control(&start("http://localhost:11434/v1", "--evil", SESSION))
            .is_err()
    );
    assert!(
        stream
            .control(&start("http://localhost:11434/v1", "llama3.1", "a b"))
            .is_err()
    );
    let mut anthropic = ApiStream::new("anthropic").unwrap();
    assert!(
        anthropic
            .control(&start("https://api.anthropic.com", "", SESSION))
            .is_ok()
    );
}

#[test]
fn anthropic_tool_loop_streams_calls_and_keeps_thinking_blocks() {
    let mut stream = connected("anthropic", "https://api.anthropic.com", "");
    let sent = control(
        &mut stream,
        json!({"op": "message", "text": "a 30 x 20 x 10 mm box", "context": "Selected: F1",
               "images": [{"media_type": "image/png", "data": "iVBORw0KGgo="}]}),
    );
    assert_eq!(kinds(&sent), ["http"]);
    let http = &sent[0];
    assert_eq!(http["url"], "https://api.anthropic.com/v1/messages");
    assert_eq!(http["auth"], "x-api-key");
    assert_eq!(http["headers"]["anthropic-version"], "2026-10-21");
    let body = body_of(http);
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["stream"], true);
    assert!(body.get("temperature").is_none());
    assert!(
        body["system"]
            .as_str()
            .unwrap()
            .ends_with("Use model_command for changes.")
    );
    assert_eq!(body["tools"][0]["name"], "model_command");
    assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    let content = &body["messages"][0]["content"];
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["source"]["media_type"], "image/png");
    assert_eq!(content[1]["text"], "a 30 x 20 x 10 mm box");
    assert_eq!(content[2]["text"], "Selected: F1");
    let id = http["id"].as_u64().unwrap();

    let streamed = lines(&[
        anthropic_event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_1"}}),
        ),
        anthropic_event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0,
                   "content_block": {"type": "thinking", "thinking": ""}}),
        ),
        anthropic_event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0,
                   "delta": {"type": "thinking_delta", "thinking": "A box."}}),
        ),
        anthropic_event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0,
                   "delta": {"type": "signature_delta", "signature": "sig=="}}),
        ),
        anthropic_event(
            "content_block_stop",
            json!({"type": "content_block_stop", "index": 0}),
        ),
        anthropic_event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 1,
                   "content_block": {"type": "text", "text": ""}}),
        ),
        anthropic_event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 1,
                   "delta": {"type": "text_delta", "text": "Adding the box."}}),
        ),
        anthropic_event(
            "content_block_stop",
            json!({"type": "content_block_stop", "index": 1}),
        ),
        anthropic_event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 2,
                   "content_block": {"type": "tool_use", "id": "toolu_1", "name": "model_command", "input": {}}}),
        ),
        anthropic_event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 2,
                   "delta": {"type": "input_json_delta", "partial_json": "{\"command\": {\"cmd\""}}),
        ),
        anthropic_event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 2,
                   "delta": {"type": "input_json_delta", "partial_json": ": \"add_feature\"}}"}}),
        ),
        anthropic_event(
            "content_block_stop",
            json!({"type": "content_block_stop", "index": 2}),
        ),
        anthropic_event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}}),
        ),
        vec![": a comment".to_owned()],
        anthropic_event("ping", json!({"type": "ping"})),
        anthropic_event("message_stop", json!({"type": "message_stop"})),
    ]);
    let refs: Vec<&str> = streamed.iter().map(String::as_str).collect();
    let events = sse(&mut stream, id, &refs);
    assert_eq!(kinds(&events), ["text", "tool_start", "tool"]);
    assert_eq!(events[0]["text"], "Adding the box.");
    assert_eq!(events[1]["id"], "toolu_1");
    assert_eq!(events[2]["name"], "model_command");
    assert_eq!(events[2]["input"]["command"]["cmd"], "add_feature");

    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 200}),
    );
    assert_eq!(kinds(&ended), ["mcp"]);
    let call = line_of(&ended[0]);
    assert_eq!(call["method"], "tools/call");
    assert_eq!(call["params"]["name"], "model_command");
    assert_eq!(call["params"]["arguments"]["command"]["cmd"], "add_feature");

    let result = stream.feed(
        &json!({"jsonrpc": "2.0", "id": call["id"], "result": {
            "content": [{"type": "text", "text": "{\"changes\": 1}"},
                        {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"}],
            "isError": false}})
        .to_string(),
    );
    assert_eq!(kinds(&result), ["tool_result", "http"]);
    assert_eq!(result[0]["id"], "toolu_1");
    assert_eq!(result[0]["error"], false);
    assert_eq!(result[0]["text"], "{\"changes\": 1}\n[image]");
    let next = body_of(&result[1]);
    let assistant = &next["messages"][1];
    assert_eq!(assistant["role"], "assistant");
    // The thinking block goes back unchanged, with its signature.
    assert_eq!(assistant["content"][0]["type"], "thinking");
    assert_eq!(assistant["content"][0]["thinking"], "A box.");
    assert_eq!(assistant["content"][0]["signature"], "sig==");
    assert_eq!(
        assistant["content"][2]["input"]["command"]["cmd"],
        "add_feature"
    );
    let results = &next["messages"][2];
    assert_eq!(results["role"], "user");
    assert_eq!(results["content"][0]["type"], "tool_result");
    assert_eq!(results["content"][0]["tool_use_id"], "toolu_1");
    assert_eq!(results["content"][0]["is_error"], false);
    assert_eq!(results["content"][0]["content"][1]["type"], "image");

    let second = result[1]["id"].as_u64().unwrap();
    let streamed = lines(&[
        anthropic_event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": "Done."}}),
        ),
        anthropic_event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}}),
        ),
        anthropic_event("message_stop", json!({"type": "message_stop"})),
    ]);
    let refs: Vec<&str> = streamed.iter().map(String::as_str).collect();
    assert_eq!(kinds(&sse(&mut stream, second, &refs)), ["text"]);
    let end = control(
        &mut stream,
        json!({"op": "http_end", "id": second, "status": 200}),
    );
    assert_eq!(kinds(&end), ["turn_end", "transcript"]);
    assert_eq!(end[0]["error"], false);
    assert_eq!(end[0]["subtype"], "success");
    assert_eq!(end[0]["session"], SESSION);
    let kept: Value = serde_json::from_str(end[1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(kept["provider"], "anthropic");
    assert_eq!(kept["messages"].as_array().unwrap().len(), 4);
    // Pictures are not kept.
    assert!(!end[1]["content"].as_str().unwrap().contains("iVBORw0KGgo="));
    assert_eq!(kept["messages"][0]["content"][0]["type"], "text");
}

#[test]
fn openai_tool_calls_stream_in_pieces() {
    let mut stream = connected("ollama", "http://localhost:11434/v1", "llama3.1");
    let sent = control(&mut stream, json!({"op": "message", "text": "a box"}));
    let http = &sent[0];
    assert_eq!(http["url"], "http://localhost:11434/v1/chat/completions");
    assert_eq!(http["auth"], "bearer");
    let body = body_of(http);
    assert_eq!(body["model"], "llama3.1");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["content"][0]["text"], "a box");
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    let id = http["id"].as_u64().unwrap();
    let chunk = |delta: Value, finish: Value| {
        format!(
            "data: {}",
            json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
        )
    };
    let streamed = [
        chunk(
            json!({"role": "assistant", "content": "Adding it."}),
            Value::Null,
        ),
        String::new(),
        chunk(
            json!({"tool_calls": [{"index": 0, "id": "call_a", "type": "function",
                                   "function": {"name": "model_command", "arguments": "{\"command\":"}}]}),
            Value::Null,
        ),
        String::new(),
        chunk(
            json!({"tool_calls": [{"index": 0, "function": {"arguments": " {\"cmd\": \"undo\"}}"}}]}),
            json!("tool_calls"),
        ),
        String::new(),
        "data: [DONE]".to_owned(),
        String::new(),
    ];
    let refs: Vec<&str> = streamed.iter().map(String::as_str).collect();
    assert_eq!(kinds(&sse(&mut stream, id, &refs)), ["text"]);
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 200}),
    );
    assert_eq!(kinds(&ended), ["tool", "mcp"]);
    assert_eq!(ended[0]["id"], "call_a");
    assert_eq!(ended[0]["input"]["command"]["cmd"], "undo");
    let call = line_of(&ended[1]);
    let result = stream.feed(
        &json!({"jsonrpc": "2.0", "id": call["id"], "result": {
            "content": [{"type": "text", "text": "refused"}], "isError": true}})
        .to_string(),
    );
    assert_eq!(kinds(&result), ["tool_result", "http"]);
    assert_eq!(result[0]["error"], true);
    let next = body_of(&result[1]);
    let assistant = &next["messages"][2];
    assert_eq!(assistant["content"], "Adding it.");
    assert_eq!(assistant["tool_calls"][0]["id"], "call_a");
    assert_eq!(
        assistant["tool_calls"][0]["function"]["arguments"],
        "{\"command\": {\"cmd\": \"undo\"}}"
    );
    assert_eq!(next["messages"][3]["role"], "tool");
    assert_eq!(next["messages"][3]["tool_call_id"], "call_a");
    assert_eq!(next["messages"][3]["content"], "refused");

    // An answer without its end is no whole answer.
    let second = result[1]["id"].as_u64().unwrap();
    sse(
        &mut stream,
        second,
        &[&chunk(json!({"content": "Do"}), Value::Null), ""],
    );
    let cut = control(
        &mut stream,
        json!({"op": "http_end", "id": second, "status": 200, "error": "closed"}),
    );
    assert_eq!(cut[0]["kind"], "turn_end");
    assert_eq!(cut[0]["error"], true);
}

#[test]
fn a_call_with_unreadable_arguments_is_answered_without_running() {
    let mut stream = connected("lmstudio", "http://localhost:1234/v1", "qwen");
    let sent = control(&mut stream, json!({"op": "message", "text": "go"}));
    let id = sent[0]["id"].as_u64().unwrap();
    let data = json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "c1",
        "function": {"name": "model_command", "arguments": "{not json"}}]}, "finish_reason": "tool_calls"}]});
    sse(&mut stream, id, &[&format!("data: {data}"), ""]);
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 200}),
    );
    // Its row, its error result and the next request: no MCP call.
    assert_eq!(kinds(&ended), ["tool", "tool_result", "http"]);
    assert_eq!(ended[1]["error"], true);
}

#[test]
fn a_model_without_tool_calls_is_reported_and_its_message_taken_out() {
    let mut stream = connected("ollama", "http://localhost:11434/v1", "tiny");
    let sent = control(&mut stream, json!({"op": "message", "text": "a box"}));
    let id = sent[0]["id"].as_u64().unwrap();
    control(
        &mut stream,
        json!({"op": "http_line", "id": id, "status": 400,
               "line": "{\"error\":{\"message\":\"registry.ollama.ai/library/tiny:latest does not support tools\",\"type\":\"api_error\"}}"}),
    );
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 400}),
    );
    assert_eq!(kinds(&ended), ["status", "turn_end", "transcript"]);
    assert_eq!(ended[0]["subtype"], "no_tools");
    assert_eq!(ended[1]["subtype"], "no_tools");
    assert_eq!(ended[1]["error"], true);
    assert!(
        ended[1]["text"]
            .as_str()
            .unwrap()
            .starts_with("This model cannot edit the design")
    );
    let again = control(&mut stream, json!({"op": "message", "text": "hello"}));
    let body = body_of(&again[0]);
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][1]["content"][0]["text"], "hello");
}

#[test]
fn account_failures_and_unreachable_services() {
    let mut stream = connected("anthropic", "https://api.anthropic.com", "claude-haiku-5-5");
    let sent = control(&mut stream, json!({"op": "message", "text": "a box"}));
    let id = sent[0]["id"].as_u64().unwrap();
    control(
        &mut stream,
        json!({"op": "http_line", "id": id, "status": 401,
               "line": "{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}"}),
    );
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 401}),
    );
    assert_eq!(kinds(&ended), ["account_error", "turn_end", "transcript"]);
    assert_eq!(ended[0]["account"]["code"], "signed-out");
    assert_eq!(ended[1]["account_error"], true);

    let sent = control(&mut stream, json!({"op": "message", "text": "again"}));
    let id = sent[0]["id"].as_u64().unwrap();
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 0, "error": "Connection refused"}),
    );
    assert_eq!(kinds(&ended), ["turn_end", "transcript"]);
    assert!(
        ended[0]["text"]
            .as_str()
            .unwrap()
            .contains("Connection refused")
    );

    // An error event in the stream (overloaded) ends the turn with it.
    let sent = control(&mut stream, json!({"op": "message", "text": "third"}));
    let id = sent[0]["id"].as_u64().unwrap();
    let streamed = lines(&[anthropic_event(
        "error",
        json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}),
    )]);
    let refs: Vec<&str> = streamed.iter().map(String::as_str).collect();
    sse(&mut stream, id, &refs);
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 200}),
    );
    assert_eq!(ended[0]["kind"], "turn_end");
    assert!(ended[0]["text"].as_str().unwrap().contains("Overloaded"));
}

#[test]
fn stop_ends_the_request_and_the_next_message_follows() {
    let mut stream = connected("anthropic", "https://api.anthropic.com", "");
    let sent = control(&mut stream, json!({"op": "message", "text": "wait for me"}));
    let id = sent[0]["id"].as_u64().unwrap();
    let stopped = control(&mut stream, json!({"op": "interrupt"}));
    assert_eq!(kinds(&stopped), ["http_abort", "turn_end", "transcript"]);
    assert_eq!(stopped[0]["id"], id);
    assert_eq!(stopped[1]["subtype"], "interrupted");
    // Late lines of the ended request change nothing.
    assert!(sse(&mut stream, id, &["data: {}", ""]).is_empty());
    // The next message joins the stopped one (one user message).
    let next = control(&mut stream, json!({"op": "message", "text": "hello again"}));
    let body = body_of(&next[0]);
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["content"][0]["text"], "wait for me");
    assert_eq!(body["messages"][0]["content"][1]["text"], "hello again");
}

#[test]
fn stop_while_a_call_runs_keeps_its_place_with_a_stopped_result() {
    let mut stream = connected("anthropic", "https://api.anthropic.com", "");
    let sent = control(&mut stream, json!({"op": "message", "text": "save"}));
    let id = sent[0]["id"].as_u64().unwrap();
    let streamed = lines(&[
        anthropic_event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0,
                   "content_block": {"type": "tool_use", "id": "toolu_9", "name": "model_command", "input": {}}}),
        ),
        anthropic_event(
            "content_block_stop",
            json!({"type": "content_block_stop", "index": 0}),
        ),
        anthropic_event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}}),
        ),
        anthropic_event("message_stop", json!({"type": "message_stop"})),
    ]);
    let refs: Vec<&str> = streamed.iter().map(String::as_str).collect();
    sse(&mut stream, id, &refs);
    let ended = control(
        &mut stream,
        json!({"op": "http_end", "id": id, "status": 200}),
    );
    let call = line_of(&ended[0]);
    let stopped = control(&mut stream, json!({"op": "interrupt"}));
    assert_eq!(kinds(&stopped), ["turn_end", "transcript"]);
    let kept: Value = serde_json::from_str(stopped[1]["content"].as_str().unwrap()).unwrap();
    let results = &kept["messages"][2]["content"][0];
    assert_eq!(results["tool_use_id"], "toolu_9");
    assert_eq!(results["is_error"], true);
    // Its result later only updates its row.
    let late = stream.feed(
        &json!({"jsonrpc": "2.0", "id": call["id"], "result": {
            "content": [{"type": "text", "text": "{}"}], "isError": false}})
        .to_string(),
    );
    assert_eq!(kinds(&late), ["tool_result"]);
    assert_eq!(late[0]["id"], "toolu_9");
}

#[test]
fn the_kept_conversation_resumes_or_a_new_one_starts() {
    let kept = json!({"format": "mitcad-chat-api", "provider": "openai", "messages": [
        {"role": "user", "content": "first"}, {"role": "assistant", "content": "ok"}]})
    .to_string();
    let mut stream = ApiStream::new("openai").unwrap();
    let started = control(
        &mut stream,
        json!({"op": "start", "url": "https://models.example.com/v1", "model": "m1",
               "session": SESSION, "resume": true, "transcript": kept}),
    );
    let initialize = line_of(&started[0]);
    let answered =
        stream.feed(&json!({"jsonrpc": "2.0", "id": initialize["id"], "result": {}}).to_string());
    let list = line_of(&answered[1]);
    let ready = stream.feed(
        &json!({"jsonrpc": "2.0", "id": list["id"], "result": {"tools": [{"name": "model_query",
               "inputSchema": {"type": "object"}}]}})
        .to_string(),
    );
    assert_eq!(ready[0]["resume_failed"], false);
    let sent = control(&mut stream, json!({"op": "message", "text": "second"}));
    let body = body_of(&sent[0]);
    assert_eq!(body["messages"][1]["content"], "first");
    assert_eq!(body["messages"][3]["content"][0]["text"], "second");

    // Another provider's (or a broken) conversation is not resumed.
    let mut other = ApiStream::new("anthropic").unwrap();
    let started = control(
        &mut other,
        json!({"op": "start", "url": "https://api.anthropic.com", "session": SESSION,
               "resume": true, "transcript": kept}),
    );
    let initialize = line_of(&started[0]);
    let answered =
        other.feed(&json!({"jsonrpc": "2.0", "id": initialize["id"], "result": {}}).to_string());
    let list = line_of(&answered[1]);
    let ready = other.feed(
        &json!({"jsonrpc": "2.0", "id": list["id"], "result": {"tools": [{"name": "model_query"}]}}).to_string(),
    );
    assert_eq!(ready[0]["resume_failed"], true);
}

#[test]
fn a_message_before_the_tools_waits_and_a_failed_listener_ends() {
    let mut stream = ApiStream::new("anthropic").unwrap();
    let started = control(
        &mut stream,
        json!({"op": "start", "url": "https://api.anthropic.com", "session": SESSION}),
    );
    assert!(control(&mut stream, json!({"op": "message", "text": "early"})).is_empty());
    let initialize = line_of(&started[0]);
    let answered =
        stream.feed(&json!({"jsonrpc": "2.0", "id": initialize["id"], "result": {}}).to_string());
    let list = line_of(&answered[1]);
    let ready = stream.feed(
        &json!({"jsonrpc": "2.0", "id": list["id"], "result": {"tools": [{"name": "model_query"}]}}).to_string(),
    );
    assert_eq!(kinds(&ready), ["init", "http"]);
    assert_eq!(
        body_of(&ready[1])["messages"][0]["content"][0]["text"],
        "early"
    );

    let mut failing = ApiStream::new("anthropic").unwrap();
    let started = control(
        &mut failing,
        json!({"op": "start", "url": "https://api.anthropic.com", "session": SESSION}),
    );
    let initialize = line_of(&started[0]);
    let failed = failing.feed(
        &json!({"jsonrpc": "2.0", "id": initialize["id"], "error": {"code": -32600, "message": "refused"}})
            .to_string(),
    );
    assert_eq!(kinds(&failed), ["init", "end"]);
    assert_eq!(failed[0]["bridge"], "failed");
    assert!(
        failing
            .control(&json!({"op": "message", "text": "x"}))
            .is_err()
    );
}

#[test]
fn the_models_of_a_service() {
    let request_of = request(&json!({"op": "api_models_request", "agent": "anthropic",
                                     "url": "https://api.anthropic.com"}))
    .unwrap();
    assert_eq!(
        request_of["url"],
        "https://api.anthropic.com/v1/models?limit=100"
    );
    assert_eq!(request_of["auth"], "x-api-key");
    let local = request(&json!({"op": "api_models_request", "agent": "ollama",
                                "url": "http://localhost:11434/v1"}))
    .unwrap();
    assert_eq!(local["url"], "http://localhost:11434/v1/models");
    assert!(
        request(&json!({"op": "api_models_request", "agent": "claude", "url": "https://x.y"}))
            .is_err()
    );

    let listed = request(&json!({"op": "api_models", "agent": "anthropic", "status": 200,
        "body": "{\"data\":[{\"id\":\"claude-opus-5-5\",\"display_name\":\"Claude Opus 5.5\"}],\"has_more\":false}"}))
    .unwrap();
    assert_eq!(listed["ok"], true);
    assert_eq!(listed["models"][0]["name"], "Claude Opus 5.5");
    let ollama = request(
        &json!({"op": "api_models", "agent": "ollama", "status": 200,
        "body": "{\"object\":\"list\",\"data\":[{\"id\":\"llama3.1:8b\",\"object\":\"model\"}]}"}),
    )
    .unwrap();
    assert_eq!(ollama["models"][0]["id"], "llama3.1:8b");
    let refused = request(&json!({"op": "api_models", "agent": "anthropic", "status": 401,
        "body": "{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}"}))
    .unwrap();
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["account"]["code"], "signed-out");
    // Still the current models to choose from.
    assert_eq!(refused["models"][0]["id"], "claude-opus-5-5");
    let down = request(&json!({"op": "api_models", "agent": "ollama", "status": 0,
                               "error": "Connection refused"}))
    .unwrap();
    assert_eq!(down["ok"], false);
    assert_eq!(down["text"], "Connection refused");
}
