// SPDX-License-Identifier: MIT
//! Codex's app-server protocol in the chat (mitcad#947): the start with
//! MitCad's bridge as the only MCP server, the thread and its resume, the
//! turns of the messages, the streamed answer, tool calls and turn ends,
//! Stop (turn/interrupt), and the approvals Codex asks for answered so that
//! the live server's gate stays the one that decides. The messages are those
//! of the JSON Schema of `codex app-server generate-json-schema` (codex-cli
//! 0.157.1).

use serde_json::{Value, json};

use super::{CODEX_MINIMUM_VERSION, CodexStream, Stream, request};

const BRIDGE: &str = "/opt/mitcad/mitcad-cli";

fn start(stream: &mut CodexStream, extra: Value) -> Vec<Value> {
    let mut control = json!({"op": "start", "bridge": BRIDGE, "instance": "4f1d2c",
                             "chat": "a1b2c3", "cwd": "/home/u/.local/share/MitCad/chat-agent"});
    for (key, value) in extra.as_object().unwrap() {
        control[key] = value.clone();
    }
    stream.control(&control).unwrap()
}

fn feed(stream: &mut CodexStream, message: Value) -> Vec<Value> {
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

/// `initialize`'s result as the app server gives it.
fn initialized() -> Value {
    json!({"id": 0, "result": {"codexHome": "/home/u/.codex", "platformFamily": "unix",
                               "platformOs": "linux", "userAgent": "codex/test"}})
}

/// A stream started, initialized and with its thread `thr_1`.
fn ready() -> CodexStream {
    let mut stream = CodexStream::default();
    start(&mut stream, json!({}));
    feed(&mut stream, initialized());
    feed(
        &mut stream,
        json!({"id": 1, "result": {"thread": {"id": "thr_1"}, "model": "gpt-test"}}),
    );
    stream
}

#[test]
fn the_start_gives_codex_the_bridge_only_and_its_own_tools_off() {
    let mut stream = CodexStream::default();
    let first = written(&start(&mut stream, json!({"model": "gpt-test"})));
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["method"], "initialize");
    assert_eq!(first[0]["id"], 0);
    assert_eq!(first[0]["params"]["clientInfo"]["name"], "mitcad");
    assert!(first[0].get("jsonrpc").is_none());

    let events = feed(&mut stream, initialized());
    let sent = written(&events);
    // A notification without params: the schema's `initialized` has none.
    assert_eq!(sent[0], json!({"method": "initialized"}));
    assert_eq!(sent[1]["method"], "thread/start");
    let params = &sent[1]["params"];
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["model"], "gpt-test");
    assert_eq!(params["cwd"], "/home/u/.local/share/MitCad/chat-agent");
    let config = &params["config"];
    // The whole table: the user's other MCP servers stay out.
    let servers = config["mcp_servers"].as_object().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers["mitcad"]["command"], BRIDGE);
    assert_eq!(
        servers["mitcad"]["args"],
        json!([
            "mcp",
            "--connect",
            "--instance",
            "4f1d2c",
            "--chat-session",
            "a1b2c3"
        ])
    );
    assert!(servers["mitcad"]["tool_timeout_sec"].as_u64().unwrap() >= 600);
    assert_eq!(config["approval_policy"], "never");
    assert_eq!(config["sandbox_mode"], "read-only");
    assert_eq!(sent[2]["method"], "model/list");

    let init = feed(
        &mut stream,
        json!({"id": 1, "result": {"thread": {"id": "thr_1"}, "model": "gpt-test"}}),
    );
    assert_eq!(
        init,
        vec![
            json!({"kind": "init", "session": "thr_1", "model": "gpt-test",
                         "bridge": "pending", "bridge_error": null, "resume_failed": false})
        ]
    );
    let models = feed(
        &mut stream,
        json!({"id": 2, "result": {"data": [
            {"id": "m1", "model": "gpt-test", "displayName": "GPT test", "isDefault": true,
             "hidden": false, "description": "", "defaultReasoningEffort": "medium",
             "supportedReasoningEfforts": []},
            {"id": "m2", "model": "gpt-hidden", "displayName": "Hidden", "hidden": true,
             "isDefault": false},
            {"id": "m3", "model": "--bad", "displayName": "Bad", "hidden": false,
             "isDefault": false}], "nextCursor": null}}),
    );
    assert_eq!(
        models,
        vec![json!({"kind": "models", "models": [
            {"id": "gpt-test", "name": "GPT test", "default": true}]})]
    );
}

#[test]
fn the_thread_config_uses_only_keys_the_current_codex_knows() {
    let mut stream = CodexStream::default();
    start(&mut stream, json!({}));
    let sent = written(&feed(&mut stream, initialized()));
    let config = &sent[1]["params"]["config"];
    // Shell (also its unified form), the freeform patch tool and pictures:
    // features of the current Codex.
    for feature in [
        "features.shell_tool",
        "features.unified_exec",
        "features.apply_patch_freeform",
        "features.view_image",
        "features.web_search_request",
    ] {
        assert_eq!(config[feature], false, "{feature}");
    }
    // Web search is a mode ("disabled", "cached", "indexed", "live").
    assert_eq!(config["web_search"], "disabled");
    // Gone (`include_apply_patch_tool`), moved to features
    // (`tools.view_image`), or a table now (`tools.web_search`).
    for stale in [
        "include_apply_patch_tool",
        "tools.view_image",
        "tools.web_search",
    ] {
        assert!(config.get(stale).is_none(), "{stale}");
    }
    let server = &config["mcp_servers"]["mitcad"];
    assert_eq!(server["startup_timeout_sec"], 60);
    assert_eq!(server["tool_timeout_sec"], 900);
}

#[test]
fn the_start_checks_what_goes_on_the_bridge_command() {
    let mut stream = CodexStream::default();
    for (field, value) in [
        ("instance", "-x"),
        ("chat", ""),
        ("bridge", ""),
        ("model", "--dangerously"),
    ] {
        let mut control =
            json!({"op": "start", "bridge": BRIDGE, "instance": "4f1d2c", "chat": "a1b2c3"});
        control[field] = json!(value);
        assert!(stream.control(&control).is_err(), "{field} {value}");
    }
    let resume = json!({"op": "start", "bridge": BRIDGE, "instance": "4f1d2c", "chat": "a1b2c3",
                        "resume": true, "session": "--last"});
    assert!(stream.control(&resume).is_err());
    assert!(stream.control(&json!({"op": "nothing"})).is_err());
    // Claude Code's stream has no controls.
    let mut claude = Stream::new("claude").unwrap();
    assert!(claude.control(&json!({"op": "start"})).is_err());
    assert!(Stream::new("codex").is_ok());
}

#[test]
fn a_message_before_the_thread_waits_and_goes_as_a_turn() {
    let mut stream = CodexStream::default();
    start(&mut stream, json!({"model": "gpt-test"}));
    let early = stream
        .control(&json!({"op": "message", "text": " a 30 x 20 x 10 mm box ",
                         "context": "Selected: face F2",
                         "images": [{"media_type": "image/png", "data": "iVBORw0KGgo="}]}))
        .unwrap();
    assert!(early.is_empty());
    feed(&mut stream, initialized());
    let events = feed(
        &mut stream,
        json!({"id": 1, "result": {"thread": {"id": "thr_1"}}}),
    );
    assert_eq!(events[0]["kind"], "init");
    assert_eq!(events[0]["model"], "gpt-test");
    let turn = written(&events);
    assert_eq!(turn.len(), 1);
    assert_eq!(turn[0]["method"], "turn/start");
    assert_eq!(turn[0]["params"]["threadId"], "thr_1");
    assert_eq!(turn[0]["params"]["model"], "gpt-test");
    assert_eq!(
        turn[0]["params"]["input"],
        json!([
            {"type": "text", "text": "a 30 x 20 x 10 mm box"},
            {"type": "text", "text": "Selected: face F2"},
            {"type": "image", "url": "data:image/png;base64,iVBORw0KGgo="}
        ])
    );
    assert!(
        stream
            .control(&json!({"op": "message", "text": "  "}))
            .is_err()
    );
    assert!(
        stream
            .control(&json!({"op": "message", "text": "x",
                             "images": [{"media_type": "image/gif", "data": "AA=="}]}))
            .is_err()
    );
}

#[test]
fn a_turn_streams_its_answer_tools_and_end() {
    let mut stream = ready();
    let sent = written(
        &stream
            .control(&json!({"op": "message", "text": "a box"}))
            .unwrap(),
    );
    let turn_request = sent[0]["id"].clone();
    assert!(
        feed(
            &mut stream,
            json!({"id": turn_request, "result": {"turn": {"id": "turn_1", "status": "inProgress"}}})
        )
        .is_empty()
    );
    assert!(
        feed(
            &mut stream,
            json!({"method": "turn/started", "params": {"threadId": "thr_1", "turn": {"id": "turn_1"}}})
        )
        .is_empty()
    );
    let delta = feed(
        &mut stream,
        json!({"method": "item/agentMessage/delta",
               "params": {"threadId": "thr_1", "turnId": "turn_1", "itemId": "msg_1", "delta": "Adding "}}),
    );
    assert_eq!(delta, vec![json!({"kind": "text", "text": "Adding "})]);
    // The whole message after its deltas gives no text again.
    assert!(
        feed(
            &mut stream,
            json!({"method": "item/completed",
                   "params": {"item": {"type": "agentMessage", "id": "msg_1", "text": "Adding a box."}}})
        )
        .is_empty()
    );
    let tool = feed(
        &mut stream,
        json!({"method": "item/started", "params": {"item": {"type": "mcpToolCall", "id": "call_1",
            "server": "mitcad", "tool": "model_command", "status": "inProgress",
            "arguments": {"document": "d", "command": {"cmd": "add_feature"}}}}}),
    );
    assert_eq!(
        tool,
        vec![
            json!({"kind": "tool", "id": "call_1", "name": "model_command", "server": "mitcad",
                    "input": {"document": "d", "command": {"cmd": "add_feature"}}})
        ]
    );
    let done = feed(
        &mut stream,
        json!({"method": "item/completed", "params": {"item": {"type": "mcpToolCall", "id": "call_1",
            "server": "mitcad", "tool": "model_command", "status": "completed",
            "arguments": {"document": "d"},
            "result": {"content": [{"type": "text", "text": "{\"ok\":true}"}, {"type": "image", "data": "AA=="}]}}}}),
    );
    assert_eq!(
        done,
        vec![
            json!({"kind": "tool_result", "id": "call_1", "error": false,
                    "text": "{\"ok\":true}\n[image]"})
        ]
    );
    // A call that completes without having started: its row, then its result.
    let refused = feed(
        &mut stream,
        json!({"method": "item/completed", "params": {"item": {"type": "mcpToolCall", "id": "call_2",
            "server": "mitcad", "tool": "document_save", "status": "failed",
            "arguments": "{\"path\": \"/x.mitcad\"}",
            "error": {"message": "The user did not allow this (document_save)."}}}}),
    );
    assert_eq!(refused[0]["kind"], "tool");
    assert_eq!(refused[0]["input"]["path"], "/x.mitcad");
    assert_eq!(refused[1]["error"], true);
    assert_eq!(
        refused[1]["text"],
        "The user did not allow this (document_save)."
    );
    let whole = feed(
        &mut stream,
        json!({"method": "item/completed",
               "params": {"item": {"type": "agentMessage", "id": "msg_2", "text": "Done."}}}),
    );
    assert_eq!(whole, vec![json!({"kind": "text", "text": "Done."})]);
    let end = feed(
        &mut stream,
        json!({"method": "turn/completed",
               "params": {"threadId": "thr_1", "turn": {"id": "turn_1", "status": "completed"}}}),
    );
    assert_eq!(
        end,
        vec![
            json!({"kind": "turn_end", "error": false, "subtype": "completed",
                    "text": "", "session": "thr_1"})
        ]
    );

    // A failed turn names its error; an error notification first.
    let sent = written(
        &stream
            .control(&json!({"op": "message", "text": "again"}))
            .unwrap(),
    );
    assert_eq!(sent[0]["method"], "turn/start");
    let retry = feed(
        &mut stream,
        json!({"method": "error", "params": {"error": {"message": "reconnecting"}, "willRetry": true}}),
    );
    assert_eq!(retry[0]["subtype"], "retry");
    feed(
        &mut stream,
        json!({"method": "error", "params": {"error": {"message": "usage limit reached"}, "willRetry": false}}),
    );
    let failed = feed(
        &mut stream,
        json!({"method": "turn/completed", "params": {"turn": {"id": "turn_2", "status": "failed"}}}),
    );
    assert_eq!(failed[0]["error"], true);
    assert_eq!(failed[0]["subtype"], "failed");
    assert_eq!(failed[0]["text"], "usage limit reached");

    // A refused turn/start ends the message.
    let sent = written(
        &stream
            .control(&json!({"op": "message", "text": "third"}))
            .unwrap(),
    );
    let refused = feed(
        &mut stream,
        json!({"id": sent[0]["id"], "error": {"code": -32600, "message": "thread busy"}}),
    );
    assert_eq!(refused[0]["kind"], "turn_end");
    assert_eq!(refused[0]["text"], "thread busy");
    // Other lines: output, or nothing.
    assert_eq!(stream.feed("not json")[0]["kind"], "output");
    assert!(stream.feed("").is_empty());
    assert!(
        feed(
            &mut stream,
            json!({"method": "thread/tokenUsage/updated", "params": {}})
        )
        .is_empty()
    );
}

#[test]
fn stop_interrupts_the_running_turn() {
    let mut stream = ready();
    let sent = written(
        &stream
            .control(&json!({"op": "message", "text": "wait"}))
            .unwrap(),
    );
    // Before the turn's id is known the interrupt waits for it.
    assert!(
        stream
            .control(&json!({"op": "interrupt"}))
            .unwrap()
            .is_empty()
    );
    let interrupt = written(&feed(
        &mut stream,
        json!({"id": sent[0]["id"], "result": {"turn": {"id": "turn_9"}}}),
    ));
    assert_eq!(interrupt[0]["method"], "turn/interrupt");
    assert_eq!(
        interrupt[0]["params"],
        json!({"threadId": "thr_1", "turnId": "turn_9"})
    );
    assert!(feed(&mut stream, json!({"id": interrupt[0]["id"], "result": {}})).is_empty());
    let end = feed(
        &mut stream,
        json!({"method": "turn/completed", "params": {"turn": {"id": "turn_9", "status": "interrupted"}}}),
    );
    assert_eq!(end[0]["subtype"], "interrupted");
    assert_eq!(end[0]["error"], true);

    // With the turn known: at once.
    let sent = written(
        &stream
            .control(&json!({"op": "message", "text": "wait"}))
            .unwrap(),
    );
    feed(
        &mut stream,
        json!({"method": "turn/started", "params": {"turn": {"id": "turn_10"}}}),
    );
    feed(
        &mut stream,
        json!({"id": sent[0]["id"], "result": {"turn": {"id": "turn_10"}}}),
    );
    let now = written(&stream.control(&json!({"op": "interrupt"})).unwrap());
    assert_eq!(now[0]["params"]["turnId"], "turn_10");

    // A message still waiting for the thread ends here.
    let mut early = CodexStream::default();
    start(&mut early, json!({}));
    early
        .control(&json!({"op": "message", "text": "x"}))
        .unwrap();
    let ended = early.control(&json!({"op": "interrupt"})).unwrap();
    assert_eq!(ended[0]["kind"], "turn_end");
    assert_eq!(ended[0]["subtype"], "interrupted");
}

#[test]
fn a_thread_is_resumed_or_a_new_one_starts() {
    let mut stream = CodexStream::default();
    start(
        &mut stream,
        json!({"resume": true, "session": "019a0b1c-thread"}),
    );
    let sent = written(&feed(&mut stream, initialized()));
    assert_eq!(sent[1]["method"], "thread/resume");
    assert_eq!(sent[1]["params"]["threadId"], "019a0b1c-thread");
    assert_eq!(
        sent[1]["params"]["config"]["mcp_servers"]["mitcad"]["command"],
        BRIDGE
    );
    let resumed = feed(
        &mut stream,
        json!({"id": sent[1]["id"], "result": {"thread": {"id": "019a0b1c-thread"}, "model": "gpt-test"}}),
    );
    assert_eq!(resumed[0]["session"], "019a0b1c-thread");
    assert_eq!(resumed[0]["resume_failed"], false);

    // The thread is gone: a new one, and init says so.
    let mut gone = CodexStream::default();
    start(&mut gone, json!({"resume": true, "session": "thr_old"}));
    let sent = written(&feed(&mut gone, initialized()));
    let events = feed(
        &mut gone,
        json!({"id": sent[1]["id"], "error": {"code": -32600, "message": "no rollout found for thread id thr_old"}}),
    );
    assert_eq!(shown(&events)[0]["subtype"], "resume_failed");
    let again = written(&events);
    assert_eq!(again[0]["method"], "thread/start");
    assert!(again[0]["params"].get("threadId").is_none());
    let init = feed(
        &mut gone,
        json!({"id": again[0]["id"], "result": {"thread": {"id": "thr_new"}}}),
    );
    assert_eq!(init[0]["session"], "thr_new");
    assert_eq!(init[0]["resume_failed"], true);

    // A conversation that cannot start ends the process.
    let mut refused = CodexStream::default();
    start(&mut refused, json!({}));
    feed(&mut refused, initialized());
    let end = feed(
        &mut refused,
        json!({"id": 1, "error": {"code": -32600, "message": "not logged in"}}),
    );
    assert_eq!(end[0]["kind"], "end");
    assert!(end[0]["text"].as_str().unwrap().contains("not logged in"));
}

#[test]
fn approvals_codex_asks_for_are_answered_so_that_the_live_server_decides() {
    let mut stream = ready();
    let answer = |stream: &mut CodexStream, message: Value| {
        let events = feed(stream, message);
        (written(&events)[0].clone(), shown(&events)[0].clone())
    };
    // Its own shell and file changes: declined.
    let (shell, said) = answer(
        &mut stream,
        json!({"id": 40, "method": "item/commandExecution/requestApproval",
               "params": {"threadId": "thr_1", "turnId": "t", "itemId": "i", "command": "rm -rf ~"}}),
    );
    assert_eq!(shell, json!({"id": 40, "result": {"decision": "decline"}}));
    assert_eq!(said["subtype"], "declined");
    let (patch, _) = answer(
        &mut stream,
        json!({"id": "41", "method": "item/fileChange/requestApproval", "params": {"itemId": "i"}}),
    );
    assert_eq!(
        patch,
        json!({"id": "41", "result": {"decision": "decline"}})
    );
    // The v1 approvals: `denied` is an object with the rejection now.
    for (id, method) in [(42, "execCommandApproval"), (47, "applyPatchApproval")] {
        let (old, said) = answer(
            &mut stream,
            json!({"id": id, "method": method, "params": {"conversationId": "thr_1"}}),
        );
        assert_eq!(old["id"], id);
        let rejection = &old["result"]["decision"]["denied"]["rejection"];
        assert!(rejection.as_str().is_some_and(|text| !text.is_empty()));
        assert_eq!(said["subtype"], "declined");
    }
    let (permissions, _) = answer(
        &mut stream,
        json!({"id": 48, "method": "item/permissions/requestApproval",
               "params": {"threadId": "thr_1", "turnId": "t", "itemId": "i",
                          "permissions": {}, "cwd": "/", "startedAtMs": 0}}),
    );
    assert_eq!(
        permissions,
        json!({"id": 48, "result": {"permissions": {}}})
    );
    // A confirmation of MitCad's own tool: accepted, the gate decides.
    let (ours, said) = answer(
        &mut stream,
        json!({"id": 43, "method": "mcpServer/elicitation/request",
               "params": {"threadId": "thr_1", "serverName": "mitcad", "mode": "form",
                          "message": "Allow model_command?", "requestedSchema": {}}}),
    );
    assert_eq!(ours["result"]["action"], "accept");
    assert_eq!(said["subtype"], "allowed");
    // Another server's, or a page to open: declined.
    let (other, _) = answer(
        &mut stream,
        json!({"id": 44, "method": "mcpServer/elicitation/request",
               "params": {"serverName": "elsewhere", "mode": "form", "message": "?"}}),
    );
    assert_eq!(
        other,
        json!({"id": 44, "result": {"action": "decline", "content": null}})
    );
    let (page, _) = answer(
        &mut stream,
        json!({"id": 45, "method": "mcpServer/elicitation/request",
               "params": {"serverName": "mitcad", "mode": "url", "url": "https://example.org"}}),
    );
    assert_eq!(page["result"]["action"], "decline");
    // Anything else: not handled, as JSON-RPC says.
    let (unknown, said) = answer(
        &mut stream,
        json!({"id": 46, "method": "account/chatgptAuthTokens/refresh", "params": {}}),
    );
    assert_eq!(unknown["error"]["code"], -32601);
    assert_eq!(said["subtype"], "unhandled");
}

#[test]
fn bridge_status_after_init() {
    let mut stream = ready();
    let failed = feed(
        &mut stream,
        json!({"method": "mcpServer/startupStatus/updated",
               "params": {"name": "mitcad", "status": "failed", "error": "No running MitCad instance"}}),
    );
    assert_eq!(
        failed,
        vec![json!({"kind": "bridge", "bridge": "failed",
                    "bridge_error": "No running MitCad instance"})]
    );
    let ready = feed(
        &mut stream,
        json!({"method": "mcpServer/startupStatus/updated", "params": {"name": "mitcad", "status": "ready"}}),
    );
    assert_eq!(ready[0]["bridge"], "connected");
    assert!(
        feed(
            &mut stream,
            json!({"method": "mcpServer/startupStatus/updated",
                   "params": {"name": "mitcad", "status": "starting", "threadId": "thr_1"}})
        )
        .is_empty()
    );
    let cancelled = feed(
        &mut stream,
        json!({"method": "mcpServer/startupStatus/updated",
               "params": {"name": "mitcad", "status": "cancelled", "error": null}}),
    );
    assert_eq!(
        cancelled,
        vec![json!({"kind": "bridge", "bridge": "failed", "bridge_error": null})]
    );
    // Another server: only for the log (the thread should have none).
    let other = feed(
        &mut stream,
        json!({"method": "mcpServer/startupStatus/updated", "params": {"name": "other", "status": "ready"}}),
    );
    assert_eq!(
        other,
        vec![json!({"kind": "status", "subtype": "other_mcp_server", "text": "other ready"})]
    );
}

#[test]
fn a_config_warning_is_said_for_the_log() {
    let mut stream = ready();
    let warning = feed(
        &mut stream,
        json!({"method": "configWarning",
               "params": {"summary": "Codex is ignoring 1 unrecognized configuration setting",
                          "details": "tools.view_image", "path": null}}),
    );
    assert_eq!(
        warning,
        vec![json!({"kind": "status", "subtype": "config_warning",
                    "text": "Codex is ignoring 1 unrecognized configuration setting: tools.view_image"})]
    );
}

#[test]
fn codex_version_sign_in_and_models() {
    let new = request(&json!({"op": "codex_version", "output": "codex-cli 0.91.2\n"})).unwrap();
    assert_eq!(new["version"], "0.91.2");
    assert_eq!(new["ok"], true);
    let pre =
        request(&json!({"op": "codex_version", "output": "codex-cli 0.95.0-alpha.3"})).unwrap();
    assert_eq!(pre["version"], "0.95.0");
    let old = request(&json!({"op": "codex_version", "output": "codex-cli 0.39.0"})).unwrap();
    assert_eq!(old["ok"], false);
    assert_eq!(old["minimum"], CODEX_MINIMUM_VERSION);
    let none = request(&json!({"op": "codex_version", "output": "not found"})).unwrap();
    assert_eq!(none["version"], Value::Null);

    let signed = request(
        &json!({"op": "codex_auth", "exit_code": 0, "output": "Logged in using ChatGPT\n"}),
    )
    .unwrap();
    assert_eq!(signed, json!({"signed_in": true, "method": "ChatGPT"}));
    let key = request(
        &json!({"op": "codex_auth", "exit_code": 0, "output": "Logged in using an API key - sk-***"}),
    )
    .unwrap();
    assert_eq!(key["method"], "API key");
    let out =
        request(&json!({"op": "codex_auth", "exit_code": 1, "output": "Not logged in"})).unwrap();
    assert_eq!(out["signed_in"], false);

    assert_eq!(
        request(&json!({"op": "codex_arguments"})).unwrap()["arguments"],
        json!(["app-server"])
    );
    let claude = request(&json!({"op": "claude_models"})).unwrap();
    assert!(
        claude["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "sonnet")
    );
}

#[test]
fn a_new_conversation_resumes_only_its_own_sessions() {
    let line = |entry: Value| {
        request(&json!({"op": "history_entry", "entry": entry})).unwrap()["line"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let mut content = String::new();
    content += &line(json!({"role": "user", "text": "first", "agent": "claude", "session": "s1"}));
    // Codex chosen in the panel: a new conversation.
    content += &line(
        json!({"role": "notice", "event": "new_conversation", "agent": "codex",
                            "session": "provisional"}),
    );
    content +=
        &line(json!({"role": "user", "text": "second", "agent": "codex", "session": "thr_1"}));
    let read = request(&json!({"op": "history_read", "content": content})).unwrap();
    assert_eq!(read["sessions"], json!({"codex": "thr_1"}));
}
