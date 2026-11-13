// SPDX-License-Identifier: MIT
//! The guides on how to model well (mitcad#1508): the files in the Agent
//! Skills layout, `model_reference {"guide": …}`, the resources and the
//! names the topics use.

use std::collections::BTreeSet;

use super::*;
use crate::guide::{INDEX, TOPICS, URI_PREFIX};

/// A backend that is never called: the protocol answers the guides.
struct NoBackend;

impl Backend for NoBackend {
    fn call(&mut self, name: &str, _arguments: &Value) -> Result<ToolResult, String> {
        Err(format!("{name} is not for the guides' tests"))
    }
}

fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn ready() -> Session<NoBackend> {
    let mut session = Session::new(NoBackend);
    let initialize = request(
        1,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }),
    );
    assert!(session.handle(initialize).unwrap().get("result").is_some());
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    assert!(session.handle(initialized).is_none());
    session
}

/// `model_reference`'s answer and whether it is an error.
fn lookup(session: &mut Session<NoBackend>, arguments: Value) -> (Value, bool) {
    let response = session
        .handle(request(
            2,
            "tools/call",
            json!({"name": "model_reference", "arguments": arguments}),
        ))
        .unwrap();
    let result = &response["result"];
    (
        result["structuredContent"].clone(),
        result["isError"].as_bool().unwrap(),
    )
}

fn topic_text(id: &str) -> &'static str {
    TOPICS
        .iter()
        .find(|(topic, _)| *topic == id)
        .unwrap_or_else(|| panic!("no topic {id}"))
        .1
}

#[test]
fn every_topic_file_is_in_the_index_and_every_index_line_has_a_file() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("guide/mitcad-modeling");
    // The Agent Skills layout: the name is the folder's.
    assert_eq!(guide::field("name"), Some("mitcad-modeling"));
    let description = guide::field("description").unwrap_or_default();
    assert!(
        !description.is_empty() && description.chars().count() <= 1024,
        "SKILL.md's description: {description:?}"
    );
    assert!(folder.join("SKILL.md").is_file());
    assert!(
        guide::index().starts_with("# "),
        "the index after the frontmatter"
    );
    assert!(!guide::index().contains("description:"));
    let files: BTreeSet<String> = std::fs::read_dir(folder.join("references"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let compiled: BTreeSet<String> = TOPICS.iter().map(|(id, _)| format!("{id}.md")).collect();
    assert_eq!(files, compiled, "references/ and guide.rs's TOPICS");
    let entries = guide::entries();
    let indexed: BTreeSet<String> = entries
        .iter()
        .map(|entry| format!("{}.md", entry.id))
        .collect();
    assert_eq!(files, indexed, "references/ and SKILL.md's index");
    assert_eq!(indexed.len(), entries.len(), "an index line twice");
    // Every list line of the index links its topic's file.
    let lines = guide::index()
        .lines()
        .filter(|line| line.trim_start().starts_with("- "))
        .count();
    assert_eq!(
        lines,
        entries.len(),
        "an index line without references/<id>.md"
    );
    for entry in &entries {
        assert!(!entry.read_when.is_empty(), "{}: when to read it", entry.id);
        assert_ne!(entry.id, INDEX);
    }
    // The text compiled in is the file's, and fits one answer.
    for (id, text) in TOPICS {
        let file = std::fs::read_to_string(folder.join(format!("references/{id}.md"))).unwrap();
        assert_eq!(file.replace("\r\n", "\n"), text.replace("\r\n", "\n"));
        assert!(
            text.len() <= reference::LIMIT,
            "{id} is longer than an answer"
        );
        assert!(text.starts_with("# "), "{id} starts with its title");
    }
    assert!(guide::index().len() <= reference::LIMIT);
}

#[test]
fn the_guide_lookup_the_index_and_an_unknown_id() {
    let mut session = ready();
    // The contents list the guides beside the reference's headings.
    let (contents, is_error) = lookup(&mut session, json!({}));
    assert!(!is_error);
    let guides = contents["guides"].as_array().unwrap();
    assert_eq!(guides.len(), TOPICS.len());
    assert_eq!(guides[0]["id"], "principles");
    assert!(
        guides[0]["read_when"]
            .as_str()
            .unwrap()
            .contains("before any modeling task")
    );
    assert!(contents["usage"].as_str().unwrap().contains("guide"));
    // The index: the body of SKILL.md.
    let (answer, is_error) = lookup(&mut session, json!({"guide": "index"}));
    assert!(!is_error);
    assert_eq!(answer["guide"], "index");
    assert_eq!(answer["text"], guide::index());
    assert!(answer["text"].as_str().unwrap().contains("- [principles]("));
    assert_eq!(answer["truncated"], false);
    // A topic, also in parts from an offset.
    let (answer, is_error) = lookup(&mut session, json!({"guide": "principles"}));
    assert!(!is_error);
    assert_eq!(answer["guide"], "principles");
    assert_eq!(answer["text"], topic_text("principles"));
    let (answer, _) = lookup(&mut session, json!({"guide": "principles", "offset": 2}));
    assert_eq!(answer["text"], &topic_text("principles")[2..]);
    let (answer, is_error) = lookup(
        &mut session,
        json!({"guide": "principles", "offset": 99_999_999}),
    );
    assert!(is_error);
    assert!(answer["error"].as_str().unwrap().contains("next_offset"));
    // An unknown id lists the ids.
    let (answer, is_error) = lookup(&mut session, json!({"guide": "fillets"}));
    assert!(is_error);
    let error = answer["error"].as_str().unwrap();
    assert!(error.starts_with("no guide fillets; the guides: index, principles"));
    assert!(error.contains("{\"guide\": \"index\"}"));
    // One kind of lookup at a time.
    let (answer, is_error) = lookup(&mut session, json!({"guide": "index", "section": "Values"}));
    assert!(is_error);
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("section and guide")
    );
}

#[test]
fn the_guides_are_resources_in_both_servers() {
    let mut session = ready();
    let list = session
        .handle(request(3, "resources/list", json!({})))
        .unwrap();
    let resources = list["result"]["resources"].as_array().unwrap();
    let guides: Vec<&Value> = resources
        .iter()
        .filter(|resource| resource["uri"].as_str().unwrap().starts_with(URI_PREFIX))
        .collect();
    assert_eq!(guides.len(), 1 + TOPICS.len());
    assert_eq!(guides[0]["uri"], "mitcad://guide/index");
    assert_eq!(guides[1]["uri"], "mitcad://guide/principles");
    for resource in &guides {
        assert_eq!(resource["mimeType"], "text/markdown");
        assert!(!resource["description"].as_str().unwrap().is_empty());
    }
    for (uri, text) in [
        ("mitcad://guide/principles", topic_text("principles")),
        ("mitcad://guide/index", guide::index()),
    ] {
        let response = session
            .handle(request(4, "resources/read", json!({"uri": uri})))
            .unwrap();
        let contents = &response["result"]["contents"][0];
        assert_eq!(contents["uri"], uri);
        assert_eq!(contents["mimeType"], "text/markdown");
        assert_eq!(contents["text"], text);
    }
    for uri in ["mitcad://guide/fillets", "mitcad://guide/"] {
        let response = session
            .handle(request(5, "resources/read", json!({"uri": uri})))
            .unwrap();
        assert_eq!(response["error"]["code"], -32002, "{uri}");
    }
    // The live server lists and subscribes to them too.
    let mut live = LiveSession::new();
    let respond = |live: &mut LiveSession, message: Value| match live.prepare(message).unwrap() {
        PreparedMessage::Response(response) => response,
        PreparedMessage::Tool { .. } => panic!("the protocol answers it"),
    };
    let initialize = request(
        1,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }),
    );
    let response = respond(&mut live, initialize).unwrap();
    // One sentence of the instructions points at the index.
    assert!(
        response["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("model_reference {\"guide\": \"index\"}")
    );
    assert!(
        respond(
            &mut live,
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none()
    );
    let list = respond(&mut live, request(3, "resources/list", json!({}))).unwrap();
    assert!(
        list["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|resource| resource["uri"] == "mitcad://guide/principles")
    );
    let response = respond(
        &mut live,
        request(
            4,
            "resources/subscribe",
            json!({"uri": "mitcad://guide/principles"}),
        ),
    )
    .unwrap();
    assert_eq!(response["result"], json!({}));
}

#[test]
fn the_instructions_and_the_tool_description_point_at_the_guides() {
    let initialize = request(
        1,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }),
    );
    let mut headless = Session::new(NoBackend);
    let response = headless.handle(initialize).unwrap();
    let instructions = response["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("model_reference {\"guide\": \"index\"}"));
    assert!(instructions.chars().count() <= INSTRUCTIONS_CHARS);
    for tools in [catalog::tools(), catalog::live_tools()] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "model_reference")
            .unwrap();
        let description = tool["description"].as_str().unwrap();
        assert!(description.contains("guide (how to model well"));
        assert!(description.contains("\"index\""));
        assert!(description.contains("mitcad://guide/<id>"));
        assert!(description.chars().count() <= catalog::DESCRIPTION_CHARS);
        assert_eq!(tool["inputSchema"]["properties"]["guide"]["type"], "string");
    }
}

/// The names MCP permits, as the application gives them to
/// `model_reference` (core/ffi/src/mcp.rs): the headless server's
/// `CadBackend` adds the two print imports its `model_command` takes into
/// the design (the 3d-printing guide's slicer export, mitcad#1733).
fn permitted() -> Vocabulary {
    let owned =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    let mut commands = owned(&mitcad_model_api::classes::external_commands());
    commands.extend(owned(&["import_print_profile", "import_slicer_presets"]));
    Vocabulary {
        queries: owned(&mitcad_model_api::classes::external_queries()),
        commands,
        features: owned(<mitcad_model::FeatureDef>::TYPE_NAMES),
    }
}

/// Whether a backticked text is one name, as a command, a query, a
/// feature type or a field is written (`add_feature`, `sketch.add_line`).
fn is_name(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_lowercase())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// The names a JSON text gives to a key: `"cmd": "add_feature"`.
fn json_names<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    let pattern = format!("\"{key}\":");
    text.match_indices(&pattern)
        .filter_map(|(at, _)| {
            let rest = text[at + pattern.len()..].trim_start().strip_prefix('"')?;
            rest.split('"').next()
        })
        .filter(|name| !name.starts_with('<'))
        .collect()
}

#[test]
fn the_guides_name_only_what_mcp_permits() {
    let vocabulary = permitted();
    let tools: BTreeSet<String> = catalog::tools()
        .iter()
        .chain(&catalog::live_tools())
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    let guides: BTreeSet<&str> = std::iter::once(INDEX)
        .chain(TOPICS.iter().map(|(id, _)| *id))
        .collect();
    let permits = |name: &str| {
        [
            &vocabulary.queries,
            &vocabulary.commands,
            &vocabulary.features,
        ]
        .into_iter()
        .flatten()
        .any(|known| known == name)
            || tools.contains(name)
            || guides.contains(name)
    };
    let mut problems = Vec::new();
    let documents = std::iter::once(("SKILL.md", guide::SKILL)).chain(TOPICS.iter().copied());
    for (document, text) in documents {
        let mut code = String::new();
        let mut fenced = false;
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                code.push_str(line);
                code.push('\n');
                continue;
            }
            for span in line.split('`').skip(1).step_by(2) {
                code.push_str(span);
                code.push('\n');
                if !is_name(span) || permits(span) {
                    continue;
                }
                // A command or a query the API has but MCP refuses, or a
                // name neither MCP nor the reference knows (renamed or
                // removed). A field is in the reference.
                if mitcad_model_api::classes::command_class(span).is_some()
                    || mitcad_model_api::classes::query_class(span).is_some()
                {
                    problems.push(format!("{document}: `{span}` is not permitted through MCP"));
                } else if !COMMANDS_REFERENCE.contains(&format!("`{span}`")) {
                    problems.push(format!(
                        "{document}: `{span}` is neither a permitted name nor in the reference"
                    ));
                }
            }
        }
        // The commands and queries of JSON examples.
        for (key, names) in [
            ("cmd", &vocabulary.commands),
            ("query", &vocabulary.queries),
        ] {
            for name in json_names(&code, key) {
                if !names.iter().any(|known| known == name) {
                    problems.push(format!(
                        "{document}: \"{key}\": \"{name}\" is not permitted"
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    // The check finds what it looks for.
    assert!(is_name("sketch.add_line") && !is_name("{\"origin\", \"normal\"}"));
    assert_eq!(
        json_names("{\"cmd\": \"add_feature\"} {\"cmd\":\"<name>\"}", "cmd"),
        ["add_feature"]
    );
    assert!(mitcad_model_api::classes::command_class("export").is_some());
    assert!(!permits("export"));
}

#[test]
fn the_guides_keep_the_slicer_export_and_the_field_tested_pitfalls() {
    // mitcad#1733: the order of a slicer export and what fails often,
    // from the notes of an agent that printed kits through MCP.
    let printing = topic_text("3d-printing");
    let export = printing
        .split_once("## Export for a slicer")
        .expect("3d-printing: the export for a slicer")
        .1;
    let order = [
        "`set_print_bed_face`",
        "`place_on_plate`",
        "`import_print_profile`",
        "`set_component_material`",
        "`model_export`",
    ];
    let at: Vec<usize> = order
        .iter()
        .map(|step| {
            export
                .find(step)
                .unwrap_or_else(|| panic!("the export names {step}"))
        })
        .collect();
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    for point in [
        "Mirrored bodies",
        "its own copy",
        "\"plates\": \"one_file\"",
        "`refinement`",
        "Import from Slicer…",
    ] {
        assert!(export.contains(point), "the export: {point}");
    }
    for point in ["0.15 mm for a peg", "`step`", "50°", "one direction"] {
        assert!(printing.contains(point), "3d-printing: {point}");
    }
    let features = topic_text("choosing-features");
    for point in [
        "`all_edges`",
        "dip",
        "as many edges",
        "`combine`",
        "after every cut",
    ] {
        assert!(features.contains(point), "choosing-features: {point}");
    }
}
