// SPDX-License-Identifier: MIT
//! The chat's logic (mitcad#947): Claude Code's stream parsed into panel
//! events, its command line and messages, the approvals, the undo merge and
//! the history format.

use serde_json::{Value, json};

use super::undo::label;
use super::{ClaudeStream, Stream, request, request_text};

const SESSION: &str = "0b5c7c1e-2f1a-4c3d-9e8f-1a2b3c4d5e6f";

fn feed(stream: &mut ClaudeStream, line: Value) -> Vec<Value> {
    stream.feed(&line.to_string())
}

#[test]
fn init_names_the_bridge_and_the_model() {
    let mut stream = ClaudeStream::default();
    let events = feed(
        &mut stream,
        json!({"type": "system", "subtype": "init", "session_id": SESSION, "model": "claude-test",
               "mcp_servers": [{"name": "other", "status": "connected"}, {"name": "mitcad", "status": "connected"}]}),
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "init");
    assert_eq!(events[0]["session"], SESSION);
    assert_eq!(events[0]["model"], "claude-test");
    assert_eq!(events[0]["bridge"], "connected");

    let failed = feed(
        &mut stream,
        json!({"type": "system", "subtype": "init", "mcp_servers": [{"name": "mitcad", "status": "failed"}],
               "mcp_server_errors": [{"name": "mitcad", "error": "No running Mitcad instance"}]}),
    );
    assert_eq!(failed[0]["bridge"], "failed");
    assert_eq!(failed[0]["bridge_error"], "No running Mitcad instance");
    let by_name = feed(
        &mut stream,
        json!({"type": "system", "subtype": "init", "mcp_servers": [{"name": "mitcad", "status": "failed"}],
               "mcp_server_errors": {"mitcad": "refused"}}),
    );
    assert_eq!(by_name[0]["bridge_error"], "refused");
    let missing = feed(
        &mut stream,
        json!({"type": "system", "subtype": "init", "mcp_servers": []}),
    );
    assert_eq!(missing[0]["bridge"], "missing");
}

#[test]
fn streamed_text_is_not_repeated_by_the_whole_message() {
    let mut stream = ClaudeStream::default();
    assert!(
        feed(
            &mut stream,
            json!({"type": "stream_event", "event": {"type": "message_start",
        "message": {"id": "msg_1"}}})
        )
        .is_empty()
    );
    let first = feed(
        &mut stream,
        json!({"type": "stream_event", "event": {"type": "content_block_delta",
        "index": 0, "delta": {"type": "text_delta", "text": "Adding "}}}),
    );
    let second = feed(
        &mut stream,
        json!({"type": "stream_event", "event": {"type": "content_block_delta",
        "index": 0, "delta": {"type": "text_delta", "text": "a box."}}}),
    );
    assert_eq!(first, vec![json!({"kind": "text", "text": "Adding "})]);
    assert_eq!(second[0]["text"], "a box.");
    // The whole message after its deltas: its tool use, no text again.
    let whole = feed(
        &mut stream,
        json!({"type": "assistant", "message": {"id": "msg_1", "content": [
        {"type": "text", "text": "Adding a box."},
        {"type": "tool_use", "id": "toolu_1", "name": "mcp__mitcad__model_command",
         "input": {"document": "d", "command": {"cmd": "add_feature"}}}]}}),
    );
    assert_eq!(whole.len(), 1);
    assert_eq!(whole[0]["kind"], "tool");
    assert_eq!(whole[0]["name"], "model_command");
    assert_eq!(whole[0]["server"], "mitcad");
    assert_eq!(whole[0]["input"]["command"]["cmd"], "add_feature");
    // A message without deltas gives its text whole.
    let plain = feed(
        &mut stream,
        json!({"type": "assistant", "message": {"id": "msg_2",
        "content": [{"type": "text", "text": "Done."}]}}),
    );
    assert_eq!(plain, vec![json!({"kind": "text", "text": "Done."})]);
}

#[test]
fn tool_starts_results_and_the_turn_end() {
    let mut stream = ClaudeStream::default();
    let start = feed(
        &mut stream,
        json!({"type": "stream_event", "event": {"type": "content_block_start",
        "index": 1, "content_block": {"type": "tool_use", "id": "toolu_2", "name": "mcp__mitcad__document_save"}}}),
    );
    assert_eq!(
        start,
        vec![json!({"kind": "tool_start", "id": "toolu_2", "name": "document_save"})]
    );
    let results = feed(
        &mut stream,
        json!({"type": "user", "message": {"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "toolu_2", "is_error": true,
         "content": [{"type": "text", "text": "The user denied saving."}, {"type": "image"}]},
        {"type": "tool_result", "tool_use_id": "toolu_3", "content": "x".repeat(5000)}]}}),
    );
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["error"], true);
    assert_eq!(results[0]["text"], "The user denied saving.\n[image]");
    assert_eq!(results[1]["error"], false);
    assert_eq!(
        results[1]["text"].as_str().unwrap().len(),
        super::MAX_RESULT_CHARS
    );
    let end = feed(
        &mut stream,
        json!({"type": "result", "subtype": "success", "is_error": false,
        "result": "Done.", "session_id": SESSION}),
    );
    assert_eq!(end[0]["kind"], "turn_end");
    assert_eq!(end[0]["error"], false);
    assert_eq!(end[0]["session"], SESSION);
    let failed = feed(
        &mut stream,
        json!({"type": "result", "subtype": "error_during_execution", "is_error": false}),
    );
    assert_eq!(failed[0]["error"], true);
}

#[test]
fn other_lines_are_output_or_nothing() {
    let mut stream = Stream::new("claude").unwrap();
    assert_eq!(stream.feed("not json")[0]["kind"], "output");
    assert!(stream.feed("").is_empty());
    assert!(stream.feed("{\"type\": \"future_message\"}").is_empty());
    assert_eq!(
        stream.feed("{\"type\": \"system\", \"subtype\": \"compact_boundary\"}")[0]["kind"],
        "status"
    );
    assert!(Stream::new("someone").is_err());
}

#[test]
fn the_command_line_starts_or_resumes_with_the_bridge_only() {
    let base = json!({"op": "claude_arguments", "session": SESSION, "bridge": "/opt/mitcad/mitcad-cli",
                      "instance": "4f1d2c", "chat": "a1b2c3"});
    let fresh = request(&base).unwrap();
    let arguments: Vec<&str> = fresh["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        &arguments[..7],
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages"
        ]
    );
    assert_eq!(&arguments[7..9], ["--session-id", SESSION]);
    let tools = arguments.iter().position(|a| *a == "--tools").unwrap();
    assert_eq!(arguments[tools + 1], "");
    assert!(arguments.contains(&"--strict-mcp-config"));
    let allowed = arguments
        .iter()
        .position(|a| *a == "--allowedTools")
        .unwrap();
    assert_eq!(arguments[allowed + 1], "mcp__mitcad");
    let config_at = arguments.iter().position(|a| *a == "--mcp-config").unwrap();
    let config: Value = serde_json::from_str(arguments[config_at + 1]).unwrap();
    assert_eq!(
        config["mcpServers"]["mitcad"]["command"],
        "/opt/mitcad/mitcad-cli"
    );
    assert_eq!(
        config["mcpServers"]["mitcad"]["args"],
        json!([
            "mcp",
            "--connect",
            "--instance",
            "4f1d2c",
            "--chat-session",
            "a1b2c3"
        ])
    );

    let mut resume = base.clone();
    resume["resume"] = json!(true);
    resume["model"] = json!("claude-test");
    let resumed = request(&resume).unwrap();
    let words: Vec<&str> = resumed["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(&words[7..9], ["--resume", SESSION]);
    assert_eq!(&words[words.len() - 2..], ["--model", "claude-test"]);

    for (field, value) in [
        ("session", "--help"),
        ("instance", "-x"),
        ("chat", ""),
        ("bridge", ""),
        ("model", "--dangerously"),
    ] {
        let mut bad = base.clone();
        bad[field] = json!(value);
        assert!(request(&bad).is_err(), "{field} {value}");
    }
}

#[test]
fn messages_carry_text_context_and_pictures() {
    let answer = request(&json!({"op": "claude_message", "text": " a 30 x 20 x 10 mm box ",
        "context": "Selected: face F2", "images": [{"media_type": "image/png", "data": "iVBORw0KGgo="}]})).unwrap();
    let line = answer["line"].as_str().unwrap();
    assert!(line.ends_with('\n'));
    assert_eq!(line.matches('\n').count(), 1);
    let message: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(message["type"], "user");
    assert_eq!(message["parent_tool_use_id"], Value::Null);
    let content = message["message"]["content"].as_array().unwrap();
    assert_eq!(
        content[0],
        json!({"type": "text", "text": "a 30 x 20 x 10 mm box"})
    );
    assert_eq!(content[1]["text"], "Selected: face F2");
    assert_eq!(content[2]["source"]["media_type"], "image/png");
    assert!(request(&json!({"op": "claude_message", "text": "  "})).is_err());
    assert!(request(&json!({"op": "claude_message", "text": "x", "images": [{"media_type": "image/gif", "data": "AA=="}]})).is_err());
    assert!(request(&json!({"op": "claude_message", "text": "x", "images": [{"media_type": "image/png", "data": "\"}"}]})).is_err());
}

#[test]
fn versions_and_sign_in() {
    let new =
        request(&json!({"op": "claude_version", "output": "2.4.10 (Claude Code)\n"})).unwrap();
    assert_eq!(new["version"], "2.4.10");
    assert_eq!(new["ok"], true);
    let old = request(&json!({"op": "claude_version", "output": "1.0.128 (Claude Code)"})).unwrap();
    assert_eq!(old["ok"], false);
    assert_eq!(old["minimum"], super::CLAUDE_MINIMUM_VERSION);
    let none = request(&json!({"op": "claude_version", "output": "command not found"})).unwrap();
    assert_eq!(none["version"], Value::Null);

    let signed = request(&json!({"op": "claude_auth", "exit_code": 0,
        "output": "{\"loggedIn\": true, \"authMethod\": \"claude.ai\"}"}))
    .unwrap();
    assert_eq!(signed, json!({"signed_in": true, "method": "claude.ai"}));
    let out =
        request(&json!({"op": "claude_auth", "exit_code": 1, "output": "Not logged in"})).unwrap();
    assert_eq!(out["signed_in"], false);
    let said_no =
        request(&json!({"op": "claude_auth", "exit_code": 0, "output": "{\"loggedIn\": false}"}))
            .unwrap();
    assert_eq!(said_no["signed_in"], false);
}

fn approval(tool: &str, arguments: Value, mode: &str, always: Value) -> Value {
    request(
        &json!({"op": "approval", "tool": tool, "arguments": arguments, "mode": mode,
                    "always": always, "turn_depth": 4, "undo_depth": 6}),
    )
    .unwrap()
}

#[test]
fn approvals_by_mode() {
    let edit = json!({"document": "d", "command": {"cmd": "add_feature"}});
    let save = json!({"document": "d", "path": "/designs/box.mitcad"});
    for (mode, edit_verdict, save_verdict) in [
        ("ask", "ask", "ask"),
        ("model", "run", "ask"),
        ("all", "run", "run"),
        ("unknown", "run", "ask"),
    ] {
        assert_eq!(
            approval("model_command", edit.clone(), mode, json!([]))["verdict"],
            edit_verdict,
            "{mode}"
        );
        assert_eq!(
            approval("document_save", save.clone(), mode, json!([]))["verdict"],
            save_verdict,
            "{mode}"
        );
        assert_eq!(
            approval("model_query", json!({}), mode, json!([]))["verdict"],
            "run",
            "{mode}"
        );
        assert_eq!(
            approval("ui_screenshot", json!({}), mode, json!([]))["verdict"],
            "run",
            "{mode}"
        );
    }
    let saved = approval("document_save", save.clone(), "model", json!([]));
    assert_eq!(saved["kind"], "document_save");
    assert_eq!(saved["path"], "/designs/box.mitcad");
    assert_eq!(
        approval("document_save", save, "model", json!(["document_save"]))["verdict"],
        "run"
    );
    assert_eq!(
        approval("model_command", edit, "ask", json!(["edit"]))["verdict"],
        "run"
    );
    for (tool, arguments, kind) in [
        (
            "document_open",
            json!({"path": "/designs/a.mitcad"}),
            "document_open",
        ),
        (
            "model_export",
            json!({"path": "/designs/a.step"}),
            "model_export",
        ),
        (
            "ui_screenshot",
            json!({"path": "/designs/a.png"}),
            "screenshot_file",
        ),
        ("ui_command", json!({"id": "file.save"}), "file_command"),
        ("some_new_tool", json!({}), "other"),
    ] {
        let decided = approval(tool, arguments, "model", json!([]));
        assert_eq!(decided["verdict"], "ask", "{tool}");
        assert_eq!(decided["kind"], kind, "{tool}");
    }
}

#[test]
fn undo_asks_only_past_the_message_start() {
    let undo = |turn: Value, now: Value, tool: &str| {
        let arguments = if tool == "ui_command" {
            json!({"id": "edit.undo"})
        } else {
            json!({"document": "d", "command": {"cmd": "undo"}})
        };
        request(
            &json!({"op": "approval", "tool": tool, "arguments": arguments, "mode": "model",
                        "always": [], "turn_depth": turn, "undo_depth": now}),
        )
        .unwrap()
    };
    for tool in ["model_command", "ui_command"] {
        // The agent's own step of this message: runs.
        assert_eq!(undo(json!(4), json!(5), tool)["verdict"], "run");
        // A step from before the message: asks.
        let past = undo(json!(4), json!(4), tool);
        assert_eq!(past["verdict"], "ask");
        assert_eq!(past["kind"], "undo_past_start");
        assert_eq!(undo(Value::Null, json!(9), tool)["verdict"], "ask");
    }
}

#[test]
fn undo_merges_a_turn_into_one_named_step() {
    let merge = |fields: Value| {
        let mut asked = json!({"op": "undo_merge", "start_depth": 3, "end_depth": 6,
                               "message": "a 30 × 20 × 10 mm box with the top edges rounded 2 mm"});
        for (key, value) in fields.as_object().unwrap() {
            asked[key] = value.clone();
        }
        request(&asked).unwrap()
    };
    let merged = merge(json!({}));
    assert_eq!(merged["merge"], true);
    assert_eq!(merged["depth"], 3);
    assert_eq!(merged["label"], "AI: a 30 × 20 × 10 mm box…");
    assert_eq!(merge(json!({"user_edited": true}))["reason"], "user_edited");
    assert_eq!(merge(json!({"user_edited": true}))["merge"], false);
    assert_eq!(merge(json!({"sketch": true}))["reason"], "sketch");
    assert_eq!(merge(json!({"panel": true}))["reason"], "panel");
    assert_eq!(merge(json!({"end_depth": 3}))["reason"], "no_change");
    assert_eq!(merge(json!({"end_depth": 2}))["reason"], "undone");
    assert_eq!(merge(json!({"end_depth": 4}))["merge"], true);
    assert_eq!(merge(json!({"start_depth": null}))["reason"], "unknown");
}

#[test]
fn labels_keep_the_first_words_on_one_line() {
    assert_eq!(label("Add a hole"), "AI: Add a hole");
    assert_eq!(
        label("one two three four five six seven eight nine"),
        "AI: one two three four five six seven eight…"
    );
    assert_eq!(
        label("line\nbreaks\tand\u{7}controls"),
        "AI: line breaks and controls"
    );
    assert_eq!(label(&"x".repeat(60)), format!("AI: {}…", "x".repeat(40)));
    assert_eq!(label("   "), "AI: message");
}

#[test]
fn history_lines_round_trip_and_resume_the_last_session() {
    let line = |entry: Value| {
        request(&json!({"op": "history_entry", "entry": entry})).unwrap()["line"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let mut content = String::new();
    content += &line(
        json!({"role": "user", "text": "first", "agent": "claude", "session": "s1",
                            "time": "2026-10-29T10:00:00Z", "unknown": "dropped"}),
    );
    content +=
        &line(json!({"role": "assistant", "text": "answer", "agent": "claude", "session": "s1"}));
    content += "{not json\n";
    content += "{\"v\": 9, \"role\": \"user\", \"text\": \"from a newer MitCad\"}\n";
    content += &line(
        json!({"role": "tool", "name": "model_command", "summary": "add_feature", "error": false}),
    );
    let read = request(&json!({"op": "history_read", "content": content})).unwrap();
    let entries = read["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["text"], "first");
    assert!(entries[0].get("unknown").is_none());
    assert_eq!(entries[0]["v"], 1);
    assert_eq!(entries[2]["error"], false);
    assert_eq!(read["skipped"], 2);
    assert_eq!(read["sessions"]["claude"], "s1");

    // A new conversation: only what follows is shown; its session resumes.
    content += &line(
        json!({"role": "notice", "event": "new_conversation", "agent": "claude", "session": "s2"}),
    );
    content += &line(json!({"role": "user", "text": "second", "agent": "claude", "session": "s2"}));
    let again = request(&json!({"op": "history_read", "content": content, "limit": 1})).unwrap();
    assert_eq!(again["entries"].as_array().unwrap().len(), 1);
    assert_eq!(again["entries"][0]["text"], "second");
    assert_eq!(again["sessions"]["claude"], "s2");
    assert!(request(&json!({"op": "history_entry", "entry": {"role": "system"}})).is_err());
}

#[test]
fn requests_in_text() {
    let answer: Value = serde_json::from_str(&request_text(
        "{\"op\": \"claude_version\", \"output\": \"2.1.0\"}",
    ))
    .unwrap();
    assert_eq!(answer["ok"], true);
    let error: Value = serde_json::from_str(&request_text("{\"op\": \"nothing\"}")).unwrap();
    assert!(error["error"].as_str().unwrap().contains("nothing"));
    let invalid: Value = serde_json::from_str(&request_text("[")).unwrap();
    assert!(invalid["error"].is_string());
}
