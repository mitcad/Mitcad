// SPDX-License-Identifier: MIT
//! `model_reference` and the errors that give the choices (mitcad#714).

use super::*;

/// A backend that names a few permitted queries, commands and feature
/// types, and answers every call with `answer` (an error when it is one).
#[derive(Default)]
struct NamingBackend {
    calls: usize,
    answer: Option<Result<ToolResult, String>>,
}

impl Backend for NamingBackend {
    fn call(&mut self, _name: &str, _arguments: &Value) -> Result<ToolResult, String> {
        self.calls += 1;
        match self.answer.take() {
            Some(answer) => answer,
            None => Ok(ToolResult {
                value: json!({"document": "D1"}),
                is_error: false,
            }),
        }
    }

    fn vocabulary(&self) -> Vocabulary {
        names()
    }
}

fn names() -> Vocabulary {
    let owned =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    Vocabulary {
        queries: owned(&["bodies", "faces", "timeline"]),
        commands: owned(&["add_feature", "edit_feature", "sketch.add_line", "undo"]),
        features: owned(&[
            "extrude",
            "construction_plane",
            "split_body",
            "rectangular_pattern",
        ]),
    }
}

fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn initialize() -> Value {
    request(
        1,
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

fn ready(backend: NamingBackend) -> Session<NamingBackend> {
    let mut session = Session::new(backend);
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
        2,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// `model_reference`'s answer and whether it is an error.
fn lookup(session: &mut Session<NamingBackend>, arguments: Value) -> (Value, bool) {
    let response = session.handle(call("model_reference", arguments)).unwrap();
    let result = &response["result"];
    assert_eq!(
        result["content"][0]["text"].as_str().unwrap(),
        result["structuredContent"].to_string()
    );
    (
        result["structuredContent"].clone(),
        result["isError"].as_bool().unwrap(),
    )
}

fn text(answer: &Value) -> &str {
    answer["text"].as_str().unwrap()
}

#[test]
fn the_reference_tool_is_read_only_and_described_in_both_servers() {
    for (tools, instructions) in [
        (catalog::tools(), "headless"),
        (catalog::live_tools(), "live"),
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "model_reference")
            .unwrap_or_else(|| panic!("{instructions}: no model_reference"));
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["inputSchema"]["required"], json!([]));
        // The tools that need the reference point to the tool.
        for name in ["model_query", "model_command"] {
            let description =
                tools.iter().find(|tool| tool["name"] == name).unwrap()["description"]
                    .as_str()
                    .unwrap();
            assert!(description.contains("model_reference"), "{name}");
            assert!(!description.contains("Read mitcad://"), "{name}");
        }
        let command = tools
            .iter()
            .find(|tool| tool["name"] == "model_command")
            .unwrap();
        let schema = command["inputSchema"]["properties"]["command"]["description"]
            .as_str()
            .unwrap();
        assert!(schema.contains("{\"cmd\": \"add_feature\", \"def\": {...}}"));
        assert!(
            schema
                .contains("JSON numbers are millimetres and radians; write \"90 deg\" for degrees")
        );
    }
    for profile in [Profile::Headless, Profile::Live] {
        let mut session = Session::new(NamingBackend::default());
        session.profile = profile;
        let response = session.handle(initialize()).unwrap();
        let instructions = response["result"]["instructions"].as_str().unwrap();
        assert!(instructions.contains("model_reference"));
        assert!(
            instructions
                .contains("JSON numbers are millimetres and radians; write \"90 deg\" for degrees")
        );
    }
}

#[test]
fn without_arguments_the_contents_and_the_permitted_names() {
    let mut session = ready(NamingBackend::default());
    let (answer, is_error) = lookup(&mut session, json!({}));
    assert!(!is_error);
    let contents = answer["contents"].as_str().unwrap();
    assert!(contents.lines().any(|line| line == "Feature definitions"));
    assert!(contents.lines().any(|line| line == "  sketch"));
    assert!(contents.lines().any(|line| line == "Queries"));
    // `####` headings are left out.
    assert!(!contents.contains("A fixed binormal"));
    assert!(contents.len() <= reference::LIMIT);
    assert_eq!(answer["queries"], json!(["bodies", "faces", "timeline"]));
    assert_eq!(answer["commands"][2], "sketch.add_line");
    assert_eq!(answer["features"][1], "construction_plane");
    assert_eq!(answer["truncated"], false);
    assert!(answer["usage"].as_str().unwrap().contains("search"));
    // The protocol answers it: the backend does nothing.
    assert_eq!(session.backend.calls, 0);
}

#[test]
fn a_section_by_its_heading_or_a_name_it_lists() {
    let mut session = ready(NamingBackend::default());
    let (answer, is_error) = lookup(&mut session, json!({"section": "Values"}));
    assert!(!is_error);
    assert_eq!(answer["section"], "Values");
    assert!(text(&answer).starts_with("## Values"));
    assert!(!text(&answer).contains("\n## References to geometry"));
    assert_eq!(answer["truncated"], false);
    let (answer, _) = lookup(&mut session, json!({"section": "split_body"}));
    assert!(
        answer["section"]
            .as_str()
            .unwrap()
            .starts_with("Face features")
    );
    assert!(text(&answer).contains("split_body"));
    let (answer, _) = lookup(&mut session, json!({"section": "sketch commands"}));
    assert_eq!(answer["section"], "Sketch commands");
    let (answer, is_error) = lookup(&mut session, json!({"section": "No such heading"}));
    assert!(is_error);
    assert!(answer["error"].as_str().unwrap().contains("search"));
}

#[test]
fn a_query_or_a_command_by_its_reference_row() {
    let mut session = ready(NamingBackend::default());
    let (answer, is_error) = lookup(&mut session, json!({"query": "faces"}));
    assert!(!is_error);
    assert_eq!(answer["query"], "faces");
    assert!(text(&answer).contains("| Query | Fields | Result |"));
    assert!(text(&answer).contains("| `faces` |"));
    // Only the rows of that query.
    assert!(!text(&answer).contains("| `edges` |"));
    assert_eq!(answer["permitted"], true);
    let (answer, _) = lookup(&mut session, json!({"command": "sketch.add_line"}));
    assert!(text(&answer).contains("| Command | Fields | Notes |"));
    assert!(text(&answer).contains("| `sketch.add_line` | `start`, `end`"));
    assert!(text(&answer).starts_with("Sketch commands"));
    assert_eq!(answer["permitted"], true);
    // A command the reference has but MCP refuses says so.
    let (answer, is_error) = lookup(&mut session, json!({"command": "begin_command"}));
    assert!(!is_error);
    assert!(text(&answer).contains("`begin_command`"));
    assert_eq!(answer["permitted"], false);
    assert!(answer["note"].as_str().unwrap().contains("not available"));
    // A name the reference does not have lists the permitted ones.
    let (answer, is_error) = lookup(&mut session, json!({"query": "body_bbox"}));
    assert!(is_error);
    let error = answer["error"].as_str().unwrap();
    assert!(error.contains("unknown query 'body_bbox'"));
    assert!(error.contains("bodies, faces, timeline"));
}

#[test]
fn a_feature_type_by_its_section_with_an_example() {
    let mut session = ready(NamingBackend::default());
    let (answer, is_error) = lookup(&mut session, json!({"feature": "construction_plane"}));
    assert!(!is_error);
    assert_eq!(answer["feature"], "construction_plane");
    assert!(
        answer["section"]
            .as_str()
            .unwrap()
            .starts_with("construction_plane")
    );
    assert!(
        answer["example"]
            .as_str()
            .unwrap()
            .contains("{\"type\": \"construction_plane\", \"definition\": {\"type\": \"offset\"")
    );
    assert!(!answer["example"].as_str().unwrap().contains("```"));
    assert!(text(&answer).contains("| plane | `offset` | `plane`, `distance` |"));
    assert_eq!(answer["permitted"], true);
    // A type no heading lists: the section of its first example.
    let (answer, _) = lookup(&mut session, json!({"feature": "rectangular_pattern"}));
    assert_eq!(answer["section"], "Patterns");
    assert!(
        answer["example"]
            .as_str()
            .unwrap()
            .starts_with("{\"type\": \"rectangular_pattern\"")
    );
    let (answer, is_error) = lookup(&mut session, json!({"feature": "construction_plan"}));
    assert!(is_error);
    let error = answer["error"].as_str().unwrap();
    assert!(error.contains("unknown feature type 'construction_plan'"));
    assert!(error.contains("extrude, construction_plane, split_body"));
    assert!(error.contains("{\"feature\": \"<name>\"}"));
}

#[test]
fn a_search_gives_sections_and_excerpts() {
    let mut session = ready(NamingBackend::default());
    let (answer, is_error) = lookup(&mut session, json!({"search": "BBox"}));
    assert!(!is_error);
    let hits = answer["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(answer["total"], hits.len());
    assert_eq!(answer["truncated"], false);
    for hit in hits {
        assert!(!hit["section"].as_str().unwrap().is_empty());
        let excerpt = hit["excerpt"].as_str().unwrap();
        assert!(excerpt.to_lowercase().contains("bbox"), "{excerpt}");
        assert!(excerpt.chars().count() <= 242, "{excerpt}");
    }
    assert!(hits.iter().any(|hit| hit["section"] == "Queries"));
    // Many hits come in pages.
    let (answer, _) = lookup(&mut session, json!({"search": "the"}));
    assert_eq!(answer["truncated"], true);
    let next = answer["next_offset"].as_u64().unwrap();
    assert_eq!(answer["hits"].as_array().unwrap().len() as u64, next);
    let (more, _) = lookup(&mut session, json!({"search": "the", "offset": next}));
    assert_eq!(more["total"], answer["total"]);
    assert!(!more["hits"].as_array().unwrap().is_empty());
    assert_ne!(more["hits"][0], answer["hits"][0]);
}

#[test]
fn a_long_section_is_cut_and_continues_from_its_offset() {
    let mut session = ready(NamingBackend::default());
    let start = COMMANDS_REFERENCE
        .find("\n## Feature definitions\n")
        .unwrap()
        + 1;
    let end = start + COMMANDS_REFERENCE[start..].find("\n## Commands\n").unwrap();
    let whole = &COMMANDS_REFERENCE[start..end];
    assert!(whole.len() > 2 * reference::LIMIT);
    let (first, is_error) = lookup(&mut session, json!({"section": "Feature definitions"}));
    assert!(!is_error);
    assert_eq!(first["truncated"], true);
    assert!(text(&first).len() <= reference::LIMIT);
    assert!(text(&first).ends_with('\n'));
    assert!(first["note"].as_str().unwrap().contains("offset"));
    let subsections = first["subsections"].as_array().unwrap();
    assert!(subsections.contains(&json!("extrude")));
    let mut pages = text(&first).to_owned();
    let mut next = first["next_offset"].as_u64();
    while let Some(offset) = next {
        let (page, is_error) = lookup(
            &mut session,
            json!({"section": "Feature definitions", "offset": offset}),
        );
        assert!(!is_error);
        assert!(text(&page).len() <= reference::LIMIT);
        pages.push_str(text(&page));
        next = page["next_offset"].as_u64();
    }
    assert_eq!(pages, whole);
    let (answer, is_error) = lookup(
        &mut session,
        json!({"section": "Values", "offset": 99_999_999}),
    );
    assert!(is_error);
    assert!(answer["error"].as_str().unwrap().contains("next_offset"));
}

#[test]
fn one_kind_of_lookup_at_a_time() {
    let mut session = ready(NamingBackend::default());
    let (answer, is_error) = lookup(&mut session, json!({"query": "faces", "search": "faces"}));
    assert!(is_error);
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("query and search")
    );
    // The schema refuses an unknown argument.
    let response = session
        .handle(call("model_reference", json!({"name": "faces"})))
        .unwrap();
    assert_eq!(response["error"]["code"], -32602);
}

#[test]
fn unknown_names_list_the_permitted_ones_and_a_missing_name_shows_the_form() {
    let error = unknown_query("bbox", &names());
    assert!(
        error
            .starts_with("unknown query 'bbox'; the queries MCP permits: bodies, faces, timeline.")
    );
    assert!(error.contains("model_reference {\"query\": \"<name>\"}"));
    let error = unknown_command("list", &names());
    assert!(error.starts_with("unknown command 'list'; the commands MCP permits: add_feature, edit_feature, sketch.add_line, undo."));
    assert!(unknown_query("bbox", &Vocabulary::default()).contains("model_reference"));
    // `{"command": "list"}` inside model_command's command.
    let error = missing_name(&json!({"command": "list"}), "cmd");
    assert!(error.contains("{\"cmd\": \"add_feature\", \"def\": {...}}"));
    assert!(error.contains("no 'cmd' (its fields: command)"));
    let error = missing_name(&json!({"cmd": ""}), "cmd");
    assert!(error.contains("its 'cmd' is not a nonempty string"));
    let error = missing_name(&json!({}), "query");
    assert!(error.contains("{\"query\": \"bodies\", \"properties\": true}"));
    assert!(error.contains("this one is empty"));
}

#[test]
fn missing_fields_get_the_reference_of_the_feature_type() {
    let backend = NamingBackend {
        answer: Some(Err("invalid command: missing field `definition`".to_owned())),
        ..NamingBackend::default()
    };
    let mut session = ready(backend);
    let response = session
        .handle(call(
            "model_command",
            json!({"document": "D1", "command": {"cmd": "add_feature", "def": {"type": "construction_plane"}}}),
        ))
        .unwrap();
    let result = &response["result"];
    assert_eq!(result["isError"], true);
    let value = &result["structuredContent"];
    let error = value["error"].as_str().unwrap();
    assert!(error.starts_with("invalid command: missing field `definition`; "));
    assert!(error.contains("model_reference {\"feature\": \"construction_plane\"}"));
    assert_eq!(value["reference"]["feature"], "construction_plane");
    assert!(
        value["reference"]["example"].as_str().unwrap().contains(
            "\"definition\": {\"type\": \"offset\", \"plane\": \"xy\", \"distance\": 10}"
        )
    );
    assert!(
        value["reference"]["text"]
            .as_str()
            .unwrap()
            .contains("`offset_through_point`")
    );
    assert_eq!(result["content"][0]["text"], value.to_string());
}

#[test]
fn missing_fields_of_a_command_get_its_rows() {
    // An array's refused entry (the model's error for entry 1).
    let backend = NamingBackend {
        answer: Some(Ok(ToolResult {
            value: json!({"document": "D1", "applied": false, "entry": 1, "undone": 1,
                "error": "commands[1]: invalid command: missing field `end`; the 1 step(s) of the entries before it were undone"}),
            is_error: true,
        })),
        ..NamingBackend::default()
    };
    let mut session = ready(backend);
    let response = session
        .handle(call(
            "model_command",
            json!({"document": "D1", "command": [
                {"cmd": "undo"},
                {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0]}]}),
        ))
        .unwrap();
    let value = &response["result"]["structuredContent"];
    assert_eq!(value["reference"]["command"], "sketch.add_line");
    assert!(
        value["reference"]["text"]
            .as_str()
            .unwrap()
            .contains("| `sketch.add_line` |")
    );
    assert!(
        value["error"]
            .as_str()
            .unwrap()
            .contains("model_reference {\"command\": \"sketch.add_line\"}")
    );
    // A field of the command itself, not of the definition.
    let backend = NamingBackend {
        answer: Some(Err("invalid command: missing field `uid`".to_owned())),
        ..NamingBackend::default()
    };
    let mut session = ready(backend);
    let response = session
        .handle(call(
            "model_command",
            json!({"document": "D1", "command": {"cmd": "edit_feature", "def": {"type": "extrude"}}}),
        ))
        .unwrap();
    assert_eq!(
        response["result"]["structuredContent"]["reference"]["command"],
        "edit_feature"
    );
    // Other errors stay as they are.
    let refusal = "invalid command: unknown variant `offst`, expected one of `offset`, `angle`";
    let backend = NamingBackend {
        answer: Some(Err(refusal.to_owned())),
        ..NamingBackend::default()
    };
    let mut session = ready(backend);
    let response = session
        .handle(call(
            "model_command",
            json!({"document": "D1", "command": {"cmd": "add_feature", "def": {"type": "construction_plane"}}}),
        ))
        .unwrap();
    assert_eq!(
        response["result"]["structuredContent"],
        json!({"error": refusal})
    );
}

#[test]
fn the_live_server_answers_the_reference_and_explains_its_refusals() {
    let mut session = LiveSession::with_vocabulary(names());
    let prepared =
        |session: &mut LiveSession, message: Value| match session.prepare(message).unwrap() {
            PreparedMessage::Response(response) => response,
            PreparedMessage::Tool { .. } => None,
        };
    assert!(prepared(&mut session, initialize()).is_some());
    assert!(prepared(&mut session, initialized()).is_none());
    // The protocol answers model_reference; the application runs nothing.
    let response = prepared(&mut session, call("model_reference", json!({}))).unwrap();
    assert_eq!(
        response["result"]["structuredContent"]["features"][2],
        "split_body"
    );
    assert!(session.finish(json!({}), false).is_err());
    // The application's refusal of a definition that misses a field.
    let command = json!({"document": "L1", "command": {"cmd": "add_feature",
        "def": {"type": "construction_plane", "definition": {"type": "offset"}}}});
    assert!(matches!(
        session.prepare(call("model_command", command)).unwrap(),
        PreparedMessage::Tool { .. }
    ));
    let response = session
        .finish(
            json!({"error": "invalid command: missing field `plane`"}),
            true,
        )
        .unwrap();
    assert_eq!(response["result"]["isError"], true);
    let value = &response["result"]["structuredContent"];
    assert_eq!(value["reference"]["feature"], "construction_plane");
    assert!(
        value["error"]
            .as_str()
            .unwrap()
            .contains("missing field `plane`; the fields of the feature type 'construction_plane'")
    );
}

/// mitcad#935: the errors repeat at most `ECHO_CHARS` characters of a
/// client's name, and at most a few of its field names.
#[test]
fn errors_repeat_only_the_start_of_a_clients_long_names() {
    assert_eq!(echoed("short"), "short");
    let exact = "a".repeat(ECHO_CHARS);
    assert_eq!(echoed(&exact), exact.as_str());
    let long = format!("{}{}", "ä".repeat(ECHO_CHARS), "b".repeat(1000));
    let cut = echoed(&long);
    assert!(cut.starts_with(&"ä".repeat(ECHO_CHARS)));
    assert!(cut.ends_with("… (1000 more bytes)"), "{cut}");

    let huge = "c".repeat(7 * 1024 * 1024);
    for error in [
        unknown_command(&huge, &names()),
        unknown_command(&huge, &Vocabulary::default()),
        unknown_query(&huge, &names()),
    ] {
        assert!(error.starts_with("unknown "), "{error}");
        assert!(error.contains("more bytes)"), "{error}");
        assert!(error.len() < 2048, "{}", error.len());
    }
    let mut fields = serde_json::Map::new();
    for i in 0..1000 {
        fields.insert(format!("{i}{}", "f".repeat(10_000)), json!(1));
    }
    let error = missing_name(&Value::Object(fields), "cmd");
    assert!(error.contains("this one has no 'cmd'"), "{error}");
    assert!(error.contains(", and 980 more)"), "{error}");
    assert!(error.len() < 8 * 1024, "{}", error.len());
}
