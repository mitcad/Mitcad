// SPDX-License-Identifier: MIT
//! A custom command over the Agent Client Protocol in the chat
//! (mitcad#1198): the handshake of the setup check, the command line, the
//! start with MitCad's bridge as the session's only MCP server, the
//! session and its load, the model selector, the prompts of the messages,
//! the streamed answer, tool calls and stop reasons, Stop
//! (`session/cancel`), and the permissions the agent asks for answered so
//! that the live server's gate stays the one that decides. The messages are
//! those of the protocol's published schema (version 1).

use serde_json::{Value, json};

use super::{ACP_PROTOCOL_VERSION, AcpStream, Stream, request};

const BRIDGE: &str = "/opt/mitcad/mitcad-cli";
const CWD: &str = "/home/u/.local/share/MitCad/chat-agent";

fn start(stream: &mut AcpStream, extra: Value) -> Vec<Value> {
    let mut control = json!({"op": "start", "bridge": BRIDGE, "instance": "4f1d2c",
                             "chat": "a1b2c3", "cwd": CWD});
    for (key, value) in extra.as_object().unwrap() {
        control[key] = value.clone();
    }
    stream.control(&control).unwrap()
}

fn feed(stream: &mut AcpStream, message: Value) -> Vec<Value> {
    stream.feed(&message.to_string())
}

/// The JSON-RPC messages of the `write` events, in order.
fn written(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|event| event["kind"] == "write")
        .map(|event| {
            let line = event["line"].as_str().unwrap();
            assert!(line.ends_with('\n'));
            assert_eq!(line.matches('\n').count(), 1);
            serde_json::from_str(line.trim_end()).unwrap()
        })
        .collect()
}

fn shown(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|event| event["kind"] != "write")
        .cloned()
        .collect()
}

/// `initialize`'s answer: protocol version 1, with the capabilities given.
fn initialized(load_session: bool, image: bool) -> Value {
    json!({"jsonrpc": "2.0", "id": 0, "result": {
        "protocolVersion": 1,
        "agentCapabilities": {"loadSession": load_session,
                              "promptCapabilities": {"image": image, "audio": false,
                                                     "embeddedContext": false}},
        "agentInfo": {"name": "test-agent", "title": "Test agent", "version": "1.0.0"},
        "authMethods": []}})
}

fn model_selector(current: &str) -> Value {
    json!([{"id": "model", "name": "Model", "category": "model", "type": "select",
            "currentValue": current,
            "options": [{"value": "fast-model", "name": "Fast Model"},
                        {"value": "large-model", "name": "Large Model"},
                        {"value": "--bad", "name": "Bad"}]}])
}

fn update(session: &str, update: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "session/update",
           "params": {"sessionId": session, "update": update}})
}

/// A stream started, initialized and with its session `sess_1`.
fn ready(image: bool) -> AcpStream {
    let mut stream = AcpStream::default();
    start(&mut stream, json!({}));
    feed(&mut stream, initialized(true, image));
    feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "sess_1"}}),
    );
    stream
}

/// A message sent: its `session/prompt` request.
fn prompt(stream: &mut AcpStream, text: &str) -> Value {
    let events = stream
        .control(&json!({"op": "message", "text": text}))
        .unwrap();
    let sent = written(&events);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "session/prompt");
    sent[0].clone()
}

#[test]
fn the_handshake_of_the_setup_check() {
    let line = request(&json!({"op": "acp_handshake"})).unwrap()["line"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(line.ends_with('\n'));
    let sent: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(sent["jsonrpc"], "2.0");
    assert_eq!(sent["id"], 0);
    assert_eq!(sent["method"], "initialize");
    assert_eq!(sent["params"]["protocolVersion"], ACP_PROTOCOL_VERSION);
    assert_eq!(sent["params"]["clientInfo"]["name"], "mitcad");

    let answer =
        |output: &str| request(&json!({"op": "acp_handshake_answer", "output": output})).unwrap();
    assert_eq!(answer("")["state"], "waiting");
    // A notification first, then the answer.
    let output = format!(
        "{}\n{}\n",
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {}}),
        initialized(false, false)
    );
    assert_eq!(
        answer(&output),
        json!({"state": "ok", "agent": "Test agent 1.0.0"})
    );
    let found = answer("Usage: some-tool [options]\n");
    assert_eq!(found["state"], "failed");
    assert!(
        found["reason"]
            .as_str()
            .unwrap()
            .contains("Usage: some-tool")
    );
    let line = |message: Value| answer(&(message.to_string() + "\n"));
    // A line still being written is not judged yet.
    assert_eq!(answer("{\"jsonrpc\": \"2.0\", \"id\"")["state"], "waiting");
    let refused = line(
        json!({"jsonrpc": "2.0", "id": 0, "error": {"code": -32601, "message": "Method not found"}}),
    );
    assert_eq!(refused["state"], "failed");
    assert!(
        refused["reason"]
            .as_str()
            .unwrap()
            .contains("Method not found")
    );
    let later = line(json!({"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 2}}));
    assert_eq!(later["state"], "failed");
    assert!(
        later["reason"]
            .as_str()
            .unwrap()
            .contains("protocol version 2")
    );
    // Another id's answer is not the handshake's.
    assert_eq!(
        line(json!({"jsonrpc": "2.0", "id": 5, "result": {}}))["state"],
        "waiting"
    );
}

#[test]
fn the_custom_command_line() {
    let split = |command: &str, arguments: &str| {
        request(&json!({"op": "acp_command", "command": command, "arguments": arguments}))
    };
    assert_eq!(
        split(
            " my-agent ",
            "--acp  --model \"big one\" 'it''s' C:\\Tools\\x \"a\\\"b\""
        )
        .unwrap(),
        json!({"program": "my-agent",
               "arguments": ["--acp", "--model", "big one", "its", "C:\\Tools\\x", "a\"b"]})
    );
    assert_eq!(split("agent", "").unwrap()["arguments"], json!([]));
    assert_eq!(split("agent", "\"\"").unwrap()["arguments"], json!([""]));
    assert!(split("", "--acp").is_err());
    assert!(split("agent", "\"open").is_err());
    assert!(split("agent", "a\u{0}b").is_err());
    let many = ["x"; 65].join(" ");
    assert!(split("agent", &many).is_err());
}

#[test]
fn the_start_gives_the_agent_the_bridge_only_and_no_files_or_terminal() {
    let mut stream = AcpStream::default();
    let first = written(&start(&mut stream, json!({})));
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["jsonrpc"], "2.0");
    assert_eq!(first[0]["method"], "initialize");
    assert_eq!(first[0]["id"], 0);
    let params = &first[0]["params"];
    assert_eq!(params["protocolVersion"], 1);
    assert_eq!(
        params["clientCapabilities"],
        json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false})
    );

    let sent = written(&feed(&mut stream, initialized(false, false)));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "session/new");
    assert_eq!(sent[0]["params"]["cwd"], CWD);
    assert_eq!(
        sent[0]["params"]["mcpServers"],
        json!([{"name": "mitcad", "command": BRIDGE,
                "args": ["mcp", "--connect", "--instance", "4f1d2c", "--chat-session", "a1b2c3"],
                "env": []}])
    );

    let init = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "sess_1",
                                                     "configOptions": model_selector("fast-model")}}),
    );
    assert_eq!(
        init,
        vec![
            json!({"kind": "init", "session": "sess_1", "model": "fast-model",
                   "bridge": "pending", "bridge_error": null, "resume_failed": false}),
            json!({"kind": "models", "models": [
                {"id": "fast-model", "name": "Fast Model", "default": true},
                {"id": "large-model", "name": "Large Model", "default": false}]}),
        ]
    );
}

#[test]
fn the_start_checks_what_goes_on_the_bridge_command() {
    let mut stream = AcpStream::default();
    let base = json!({"op": "start", "bridge": BRIDGE, "instance": "4f1d2c", "chat": "a1b2c3", "cwd": CWD});
    for (key, value) in [
        ("bridge", json!("")),
        ("instance", json!("--evil")),
        ("chat", json!("a b")),
        ("model", json!("--model")),
        ("cwd", json!("")),
    ] {
        let mut control = base.clone();
        control[key] = value;
        assert!(stream.control(&control).is_err(), "{key}");
    }
    let mut resume = base.clone();
    resume["resume"] = json!(true);
    resume["session"] = json!("");
    assert!(stream.control(&resume).is_err());
    assert!(stream.control(&json!({"op": "other"})).is_err());
}

#[test]
fn a_message_before_the_session_waits_and_the_chosen_model_is_set_first() {
    let mut stream = AcpStream::default();
    start(&mut stream, json!({"model": "large-model"}));
    // Written at once: it waits for the session.
    let events = stream
        .control(&json!({"op": "message", "text": "hello", "context": "the selection"}))
        .unwrap();
    assert!(events.is_empty());
    feed(&mut stream, initialized(false, false));
    let events = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "sess_1",
                                                     "configOptions": model_selector("fast-model")}}),
    );
    // The panel shows the chosen model, which is set now.
    assert_eq!(shown(&events)[0]["kind"], "init");
    assert_eq!(shown(&events)[0]["model"], "large-model");
    let sent = written(&events);
    // The model first; the prompt waits for its answer.
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "session/set_config_option");
    assert_eq!(
        sent[0]["params"],
        json!({"sessionId": "sess_1", "configId": "model", "value": "large-model"})
    );
    let events = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 2, "result": {"configOptions": model_selector("large-model")}}),
    );
    let models = shown(&events);
    assert_eq!(models[0]["kind"], "models");
    assert_eq!(models[0]["models"][1]["default"], true);
    let sent = written(&events);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "session/prompt");
    assert_eq!(
        sent[0]["params"],
        json!({"sessionId": "sess_1", "prompt": [{"type": "text", "text": "hello"},
                                                {"type": "text", "text": "the selection"}]})
    );
}

#[test]
fn a_chosen_model_without_a_selector_is_only_logged() {
    let mut stream = AcpStream::default();
    start(&mut stream, json!({"model": "large-model"}));
    feed(&mut stream, initialized(false, false));
    let events = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "sess_1"}}),
    );
    assert_eq!(events[0]["kind"], "init");
    assert_eq!(events[0]["model"], "large-model");
    assert_eq!(events[1]["subtype"], "model_unsupported");
    assert_eq!(
        prompt(&mut stream, "hello")["params"]["sessionId"],
        "sess_1"
    );
}

#[test]
fn a_prompt_streams_its_answer_tools_and_end() {
    let mut stream = ready(false);
    let sent = prompt(&mut stream, "a box");
    let id = sent["id"].clone();
    let chunk = |text: &str| {
        update(
            "sess_1",
            json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}),
        )
    };
    assert_eq!(
        feed(&mut stream, chunk("Adding ")),
        vec![json!({"kind": "text", "text": "Adding "})]
    );
    // Thoughts, plans and another session's updates are not shown.
    assert!(
        feed(
            &mut stream,
            update(
                "sess_1",
                json!({"sessionUpdate": "agent_thought_chunk",
        "content": {"type": "text", "text": "thinking"}})
            )
        )
        .is_empty()
    );
    assert!(
        feed(
            &mut stream,
            update("sess_1", json!({"sessionUpdate": "plan", "entries": []}))
        )
        .is_empty()
    );
    assert!(
        feed(
            &mut stream,
            update(
                "sess_2",
                json!({"sessionUpdate": "agent_message_chunk",
        "content": {"type": "text", "text": "other"}})
            )
        )
        .is_empty()
    );

    let input = json!({"document": "D1", "command": {"cmd": "add_feature"}});
    let started = feed(
        &mut stream,
        update(
            "sess_1",
            json!({"sessionUpdate": "tool_call", "toolCallId": "call_1",
                                 "title": "mcp__mitcad__model_command", "kind": "other",
                                 "status": "pending", "rawInput": input}),
        ),
    );
    assert_eq!(
        started,
        vec![
            json!({"kind": "tool", "id": "call_1", "name": "model_command", "server": "mitcad",
                    "input": input})
        ]
    );
    assert!(
        feed(
            &mut stream,
            update(
                "sess_1",
                json!({"sessionUpdate": "tool_call_update",
        "toolCallId": "call_1", "status": "in_progress"})
            )
        )
        .is_empty()
    );
    let finished = feed(
        &mut stream,
        update(
            "sess_1",
            json!({"sessionUpdate": "tool_call_update", "toolCallId": "call_1",
            "status": "completed",
            "content": [{"type": "content", "content": {"type": "text", "text": "{\"ok\":true}"}}]}),
        ),
    );
    assert_eq!(
        finished,
        vec![
            json!({"kind": "tool_result", "id": "call_1", "error": false, "text": "{\"ok\":true}"})
        ]
    );
    // A bare live tool name is MitCad's; its input coming later is shown
    // then; a failed call's raw output is its text.
    let second = feed(
        &mut stream,
        update(
            "sess_1",
            json!({"sessionUpdate": "tool_call", "toolCallId": "call_2",
                                 "title": "document_save", "status": "pending"}),
        ),
    );
    assert_eq!(second[0]["server"], "mitcad");
    assert_eq!(second[0]["input"], json!({}));
    let later = feed(
        &mut stream,
        update(
            "sess_1",
            json!({"sessionUpdate": "tool_call_update", "toolCallId": "call_2",
            "rawInput": {"path": "a.mitcad"}, "status": "failed",
            "rawOutput": {"content": [{"type": "text", "text": "refused"}], "isError": true}}),
        ),
    );
    assert_eq!(
        later[0],
        json!({"kind": "tool", "id": "call_2", "name": "document_save",
                                "server": "mitcad", "input": {"path": "a.mitcad"}})
    );
    assert_eq!(
        later[1],
        json!({"kind": "tool_result", "id": "call_2", "error": true, "text": "refused"})
    );
    // Another tool of the agent's own: no server.
    let own = feed(
        &mut stream,
        update(
            "sess_1",
            json!({"sessionUpdate": "tool_call", "toolCallId": "call_3",
                                 "title": "Read notes.txt", "kind": "read", "status": "pending"}),
        ),
    );
    assert_eq!(own[0]["server"], "");
    assert_eq!(own[0]["name"], "Read notes.txt");

    let ended = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "end_turn"}}),
    );
    assert_eq!(
        ended,
        vec![
            json!({"kind": "turn_end", "error": false, "subtype": "end_turn", "text": "",
                    "session": "sess_1"})
        ]
    );
}

#[test]
fn stop_reasons_and_a_refused_prompt() {
    let mut stream = ready(false);
    for (reason, error) in [
        ("max_tokens", true),
        ("max_turn_requests", true),
        ("refusal", true),
        ("a_later_reason", false),
    ] {
        let id = prompt(&mut stream, "hello")["id"].clone();
        let ended = feed(
            &mut stream,
            json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": reason}}),
        );
        assert_eq!(ended[0]["subtype"], reason);
        assert_eq!(ended[0]["error"], error, "{reason}");
        assert_eq!(
            ended[0]["text"].as_str().unwrap().is_empty(),
            !error,
            "{reason}"
        );
    }
    let id = prompt(&mut stream, "hello")["id"].clone();
    let refused = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": "Internal error"}}),
    );
    assert_eq!(refused[0]["kind"], "turn_end");
    assert_eq!(refused[0]["error"], true);
    assert_eq!(refused[0]["text"], "Internal error");
    // The next message goes on.
    assert_eq!(prompt(&mut stream, "again")["method"], "session/prompt");
}

#[test]
fn messages_written_during_a_prompt_wait_for_its_end() {
    let mut stream = ready(false);
    let id = prompt(&mut stream, "first")["id"].clone();
    let events = stream
        .control(&json!({"op": "message", "text": "second"}))
        .unwrap();
    assert!(events.is_empty());
    let ended = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "end_turn"}}),
    );
    assert_eq!(ended[0]["kind"], "turn_end");
    let sent = written(&ended);
    assert_eq!(sent[0]["params"]["prompt"][0]["text"], "second");
}

#[test]
fn stop_cancels_the_running_prompt() {
    let mut stream = ready(false);
    let id = prompt(&mut stream, "wait")["id"].clone();
    let events = stream.control(&json!({"op": "interrupt"})).unwrap();
    let sent = written(&events);
    assert_eq!(
        sent,
        vec![
            json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": "sess_1"}})
        ]
    );
    // Once.
    assert!(
        stream
            .control(&json!({"op": "interrupt"}))
            .unwrap()
            .is_empty()
    );
    // A permission asked meanwhile is answered cancelled.
    let asked = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 7, "method": "session/request_permission", "params": {
            "sessionId": "sess_1", "toolCall": {"toolCallId": "call_9", "title": "model_command"},
            "options": [{"optionId": "yes", "name": "Allow", "kind": "allow_once"}]}}),
    );
    assert_eq!(
        written(&asked)[0]["result"]["outcome"],
        json!({"outcome": "cancelled"})
    );
    let ended = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": "cancelled"}}),
    );
    assert_eq!(ended[0]["subtype"], "cancelled");
    assert_eq!(ended[0]["error"], true);

    // A message not given to the agent yet ends at once.
    let mut early = AcpStream::default();
    start(&mut early, json!({}));
    early
        .control(&json!({"op": "message", "text": "hello"}))
        .unwrap();
    let ended = early.control(&json!({"op": "interrupt"})).unwrap();
    assert_eq!(ended[0]["kind"], "turn_end");
    assert_eq!(ended[0]["subtype"], "cancelled");
    assert!(written(&ended).is_empty());
    // Nothing running: nothing.
    assert!(
        early
            .control(&json!({"op": "interrupt"}))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn permissions_are_answered_so_that_the_live_server_decides() {
    let mut stream = ready(false);
    prompt(&mut stream, "a box");
    let options = json!([
        {"optionId": "always", "name": "Always allow", "kind": "allow_always"},
        {"optionId": "once", "name": "Allow", "kind": "allow_once"},
        {"optionId": "no", "name": "Reject", "kind": "reject_once"},
        {"optionId": "never", "name": "Never", "kind": "reject_always"}]);
    let ask = |id: u64, call: Value| {
        json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission",
               "params": {"sessionId": "sess_1", "toolCall": call, "options": options}})
    };
    // MitCad's own tool: allowed once (the live server asks the user).
    let events = feed(
        &mut stream,
        ask(
            3,
            json!({"toolCallId": "call_1", "title": "mcp__mitcad__model_command"}),
        ),
    );
    assert_eq!(
        written(&events),
        vec![
            json!({"jsonrpc": "2.0", "id": 3, "result": {"outcome": {"outcome": "selected", "optionId": "once"}}})
        ]
    );
    assert_eq!(shown(&events)[0]["subtype"], "allowed");
    // A call shown before is known by its id.
    feed(
        &mut stream,
        update(
            "sess_1",
            json!({"sessionUpdate": "tool_call", "toolCallId": "call_2",
                                              "title": "mitcad/model_query", "status": "pending"}),
        ),
    );
    let events = feed(&mut stream, ask(4, json!({"toolCallId": "call_2"})));
    assert_eq!(written(&events)[0]["result"]["outcome"]["optionId"], "once");
    // The agent's own edits and commands: rejected.
    let events = feed(
        &mut stream,
        ask(
            5,
            json!({"toolCallId": "call_3", "title": "Write notes.txt", "kind": "edit"}),
        ),
    );
    assert_eq!(
        written(&events)[0]["result"]["outcome"],
        json!({"outcome": "selected", "optionId": "no"})
    );
    assert_eq!(shown(&events)[0]["subtype"], "declined");
    // Without a reject option: cancelled.
    let events = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 6, "method": "session/request_permission", "params": {
            "sessionId": "sess_1", "toolCall": {"toolCallId": "call_4", "title": "rm -rf"},
            "options": [{"optionId": "ok", "name": "Allow", "kind": "allow_once"}]}}),
    );
    assert_eq!(
        written(&events)[0]["result"]["outcome"],
        json!({"outcome": "cancelled"})
    );
    // No file system and no terminal of MitCad's.
    for method in ["fs/read_text_file", "fs/write_text_file", "terminal/create"] {
        let events = feed(
            &mut stream,
            json!({"jsonrpc": "2.0", "id": 8, "method": method, "params": {"sessionId": "sess_1"}}),
        );
        let sent = written(&events);
        let answer = &sent[0];
        assert_eq!(answer["id"], 8);
        assert_eq!(answer["error"]["code"], -32601);
        assert_eq!(shown(&events)[0]["subtype"], "unhandled");
    }
}

#[test]
fn a_session_is_loaded_or_a_new_one_starts() {
    // Loaded: the replay is MitCad's own history and is not shown again.
    let mut stream = AcpStream::default();
    start(&mut stream, json!({"resume": true, "session": "sess_old"}));
    let sent = written(&feed(&mut stream, initialized(true, false)));
    assert_eq!(sent[0]["method"], "session/load");
    assert_eq!(sent[0]["params"]["sessionId"], "sess_old");
    assert_eq!(sent[0]["params"]["cwd"], CWD);
    assert_eq!(sent[0]["params"]["mcpServers"][0]["name"], "mitcad");
    assert!(
        feed(
            &mut stream,
            update(
                "sess_old",
                json!({"sessionUpdate": "agent_message_chunk",
        "content": {"type": "text", "text": "an earlier answer"}})
            )
        )
        .is_empty()
    );
    let loaded = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
    );
    assert_eq!(loaded[0]["kind"], "init");
    assert_eq!(loaded[0]["session"], "sess_old");
    assert_eq!(loaded[0]["resume_failed"], false);
    assert_eq!(
        prompt(&mut stream, "again")["params"]["sessionId"],
        "sess_old"
    );

    // The agent no longer has it: a new one, said in init.
    let mut gone = AcpStream::default();
    start(&mut gone, json!({"resume": true, "session": "sess_old"}));
    feed(&mut gone, initialized(true, false));
    let events = feed(
        &mut gone,
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32602, "message": "Session not found"}}),
    );
    assert_eq!(shown(&events)[0]["subtype"], "resume_failed");
    assert_eq!(written(&events)[0]["method"], "session/new");
    let init = feed(
        &mut gone,
        json!({"jsonrpc": "2.0", "id": 2, "result": {"sessionId": "sess_new"}}),
    );
    assert_eq!(init[0]["session"], "sess_new");
    assert_eq!(init[0]["resume_failed"], true);

    // An agent that cannot load sessions: a new one at once.
    let mut cannot = AcpStream::default();
    start(&mut cannot, json!({"resume": true, "session": "sess_old"}));
    let events = feed(&mut cannot, initialized(false, false));
    assert_eq!(shown(&events)[0]["subtype"], "resume_failed");
    assert_eq!(written(&events)[0]["method"], "session/new");
}

#[test]
fn pictures_go_only_to_an_agent_that_takes_them() {
    let picture = json!({"op": "message", "text": "look",
                         "images": [{"media_type": "image/png", "data": "iVBORw0KGgo="}]});
    let mut takes = ready(true);
    let sent = written(&takes.control(&picture).unwrap());
    assert_eq!(
        sent[0]["params"]["prompt"][1],
        json!({"type": "image", "mimeType": "image/png", "data": "iVBORw0KGgo="})
    );
    let mut refuses = ready(false);
    let events = refuses.control(&picture).unwrap();
    assert_eq!(shown(&events)[0]["subtype"], "pictures_left_out");
    assert_eq!(
        written(&events)[0]["params"]["prompt"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        takes
            .control(&json!({"op": "message", "text": "x",
                             "images": [{"media_type": "image/gif", "data": "R0lG"}]}))
            .is_err()
    );
}

#[test]
fn a_start_that_cannot_go_on_ends_the_conversation() {
    let mut stream = AcpStream::default();
    start(&mut stream, json!({}));
    let events = feed(
        &mut stream,
        json!({"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 2}}),
    );
    assert_eq!(events[0]["kind"], "end");
    assert!(
        events[0]["text"]
            .as_str()
            .unwrap()
            .contains("protocol version 2")
    );

    let mut refused = AcpStream::default();
    start(&mut refused, json!({}));
    feed(&mut refused, initialized(false, false));
    let events = feed(
        &mut refused,
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "Authentication required"}}),
    );
    assert_eq!(events[0]["kind"], "end");

    let mut nameless = AcpStream::default();
    start(&mut nameless, json!({}));
    feed(&mut nameless, initialized(false, false));
    let events = feed(
        &mut nameless,
        json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
    );
    assert_eq!(events[0]["kind"], "end");
    // A line that is no message is logged, not shown.
    assert_eq!(nameless.feed("starting up")[0]["kind"], "output");
}

#[test]
fn an_account_failure_of_a_prompt_is_an_account_error() {
    let mut stream = Stream::new("acp").unwrap();
    stream
        .control(&json!({"op": "start", "bridge": BRIDGE, "instance": "4f1d2c", "chat": "a1b2c3", "cwd": CWD}))
        .unwrap();
    stream.feed(&initialized(false, false).to_string());
    stream.feed(&json!({"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "sess_1"}}).to_string());
    let sent = stream
        .control(&json!({"op": "message", "text": "hello"}))
        .unwrap();
    assert_eq!(written(&sent)[0]["id"], 2);
    let events = stream.feed(
        &json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32000, "message": "Authentication required"}})
            .to_string(),
    );
    assert_eq!(events[0]["kind"], "account_error");
    assert_eq!(events[0]["account"]["code"], "signed-out");
    assert_eq!(events[1]["kind"], "turn_end");
    assert_eq!(events[1]["account_error"], true);
}
