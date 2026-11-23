// SPDX-License-Identifier: MIT
//! The print libraries for live agents (mitcad#2224): refused without the
//! user's preference (in words naming it), handed to the application with
//! it, never in the headless server; bound to the open design's project.

use serde_json::{Value, json};

use super::*;
use crate::{Backend, LiveSession, PreparedMessage, Session, ToolResult};
use mitcad_model_api::classes::{self, ApiClass};

/// A headless backend that records its calls.
#[derive(Default)]
struct Recorder {
    calls: Vec<String>,
}

impl Backend for Recorder {
    fn call(&mut self, name: &str, _: &Value) -> Result<ToolResult, String> {
        self.calls.push(name.to_owned());
        Ok(ToolResult {
            value: json!({"ok": true}),
            is_error: false,
        })
    }
}

fn message(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn initialize() -> Value {
    message(
        1,
        "initialize",
        json!({"protocolVersion": crate::PROTOCOL_VERSION, "capabilities": {},
               "clientInfo": {"name": "library-test", "version": "1"}}),
    )
}

fn initialized() -> Value {
    json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
}

fn call(name: &str, arguments: Value) -> Value {
    message(
        2,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

fn ready_live() -> LiveSession {
    let mut session = LiveSession::new();
    assert!(matches!(
        session.prepare(initialize()).unwrap(),
        PreparedMessage::Response(Some(_))
    ));
    assert!(matches!(
        session.prepare(initialized()).unwrap(),
        PreparedMessage::Response(None)
    ));
    session
}

/// A folder that is absolute on this platform.
fn folder(name: &str) -> String {
    if cfg!(windows) {
        format!("C:\\work\\{name}")
    } else {
        format!("/work/{name}")
    }
}

/// Every form the preference permits, as one command each.
fn library_commands() -> Vec<Value> {
    let mut commands = vec![
        json!({"cmd": "copy_from_print_library", "library": "user", "kind": "material", "id": "bench-pla"}),
        json!({"cmd": "update_from_print_library", "kind": "material", "id": "bench-pla"}),
        json!({"cmd": "decline_print_library_update", "kind": "process", "id": "draft"}),
        json!({"cmd": "save_to_print_library", "library": "user", "kind": "printer", "id": "bench"}),
        json!({"cmd": "create_material", "library": "user", "name": "Bench PLA"}),
        json!({"cmd": "create_material", "based_on": {"library": "base", "id": "pla"}}),
        json!({"cmd": "edit_material", "library": "user", "id": "bench-pla", "density": 1.3}),
        json!({"cmd": "delete_material", "library": "project", "id": "bench-pla"}),
        json!({"cmd": "merge_materials", "library": "user", "ids": ["a", "b"]}),
        json!({"cmd": "import_materials", "library": "user", "text": "[]", "format": "json"}),
        json!({"cmd": "create_dataset_materials", "library": "user", "entries": ["wood/walnut"]}),
        json!({"cmd": "set_component_material", "component": "C0", "material": "bench-pla", "library": "user"}),
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "bench-pla", "library": "user"}),
        json!({"cmd": "create_print_profile", "library": "user", "kind": "process", "name": "Draft"}),
        json!({"cmd": "create_print_profile", "kind": "printer", "based_on": {"library": "user", "id": "bench"}}),
        json!({"cmd": "edit_print_profile", "library": "user", "id": "draft", "values": {}}),
        json!({"cmd": "delete_print_profile", "library": "user", "id": "draft", "kind": "process"}),
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
               "value": "bench", "library": "user"}),
    ];
    // The project's library too, with the design's project.
    commands.push(
        json!({"cmd": "copy_from_print_library", "library": "project", "kind": "material",
                         "id": "bench-pla", "project": folder("bench")}),
    );
    commands
}

/// The calls that stay refused with the preference on.
fn refused_commands() -> Vec<Value> {
    vec![
        json!({"cmd": "import_print_profile", "path": "plate.3mf", "into": "user"}),
        json!({"cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "x", "presets": [],
               "into": "user"}),
        json!({"cmd": "flex.set_item", "item": "hose", "type": "t1", "library": "user"}),
        json!({"cmd": "cam_machine_to_library", "id": "m1"}),
    ]
}

#[test]
fn every_permitted_form_is_a_file_io_call_that_reads_no_path() {
    // The permit covers exactly calls MCP refuses by class, none that reads
    // a file by its path.
    for command in library_commands() {
        assert!(command_uses(&command), "{command}");
        assert_eq!(
            classes::command_class_with(&command),
            Some(ApiClass::FileIo),
            "{command}"
        );
        assert_eq!(classes::file_path_read(&command), None, "{command}");
    }
    for name in QUERIES {
        assert!(query_uses(&json!({"query": name})));
        assert_eq!(classes::query_class(name), Some(ApiClass::FileIo), "{name}");
    }
    // The design's own forms are the model's: no library is used.
    for command in [
        json!({"cmd": "create_material", "name": "Bench PLA"}),
        json!({"cmd": "create_material", "based_on": "pla"}),
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "pla"}),
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile", "value": "bench"}),
    ] {
        assert!(!command_uses(&command), "{command}");
        assert_eq!(
            classes::command_class_with(&command),
            Some(ApiClass::Model),
            "{command}"
        );
    }
}

#[test]
fn slicer_presets_imports_and_other_libraries_stay_refused_with_the_preference() {
    // The application's validator permits a file IO call only when
    // command_uses or query_uses says so (core/ffi/src/mcp.rs); these keep
    // their class refusal.
    for command in refused_commands() {
        assert!(!command_uses(&command), "{command}");
        assert_ne!(
            classes::command_class_with(&command),
            Some(ApiClass::Model),
            "{command}"
        );
    }
    for query in [
        json!({"query": "slicer_presets", "dialect": "orcaslicer"}),
        json!({"query": "cam_library"}),
        json!({"query": "drawing_templates"}),
    ] {
        assert!(!query_uses(&query), "{query}");
        assert!(!classes::external_query_allowed(
            query["query"].as_str().unwrap()
        ));
    }
    // The session hands them to the application, whose class rule refuses
    // them: the preference changes nothing for them.
    let mut session = ready_live();
    session.set_may_use_print_libraries(true);
    for command in refused_commands() {
        match session
            .prepare(call("model_command", json!({"command": command})))
            .unwrap()
        {
            PreparedMessage::Tool { name, arguments } => {
                assert_eq!(name, "model_command");
                assert_eq!(arguments["command"], command);
            }
            PreparedMessage::Response(response) => panic!("{command}: {response:?}"),
        }
        session
            .finish(json!({"error": "refused by class"}), true)
            .unwrap();
    }
}

/// The refusal a live session gives for a call, or None when it hands
/// the call to the application (and completes it).
fn live_refusal(session: &mut LiveSession, tool: &str, arguments: Value) -> Option<Value> {
    match session.prepare(call(tool, arguments)).unwrap() {
        PreparedMessage::Response(response) => {
            let response = response.expect("a response");
            assert_eq!(response["result"]["isError"], true, "{response}");
            Some(response["result"]["structuredContent"].clone())
        }
        PreparedMessage::Tool { .. } => {
            session.finish(json!({"ok": true}), false).unwrap();
            None
        }
    }
}

#[test]
fn the_live_server_refuses_each_form_without_the_preference_and_runs_it_with_it() {
    let mut session = ready_live();
    for command in library_commands() {
        let kind = command["cmd"].as_str().unwrap();
        let refusal = live_refusal(
            &mut session,
            "model_command",
            json!({"document": "live-1", "command": command.clone()}),
        )
        .unwrap_or_else(|| panic!("{command}: not refused"));
        assert_eq!(
            refusal["error"],
            json!(format!("{kind}: {LIBRARIES_REFUSED}")),
            "{command}"
        );
        assert!(
            refusal["error"]
                .as_str()
                .unwrap()
                .contains("Preferences › AI Assistants › Agents may use my print libraries")
        );
        assert_eq!(refusal["applied"], false);
    }
    for name in QUERIES {
        let refusal = live_refusal(
            &mut session,
            "model_query",
            json!({"document": "live-1", "query": {"query": name, "kind": "material"}}),
        )
        .unwrap_or_else(|| panic!("{name}: not refused"));
        assert_eq!(
            refusal["error"],
            json!(format!("query {name}: {LIBRARIES_REFUSED}"))
        );
    }
    // In an array nothing runs, and the entry is named.
    let refusal = live_refusal(
        &mut session,
        "model_command",
        json!({"document": "live-1", "command": [
            {"cmd": "create_material", "name": "Own"},
            {"cmd": "save_to_print_library", "library": "user", "kind": "material", "id": "own"}
        ]}),
    )
    .unwrap();
    assert_eq!(refusal["entry"], 1);
    assert_eq!(refusal["undone"], 0);
    assert!(refusal["error"].as_str().unwrap().starts_with(
        "commands[1]: save_to_print_library: The print libraries are left to the user"
    ));
    // The design's own items need no preference.
    assert!(
        live_refusal(
            &mut session,
            "model_command",
            json!({"command": {"cmd": "create_material", "name": "Own"}}),
        )
        .is_none()
    );
    // With the preference on, the application runs each as given.
    session.set_may_use_print_libraries(true);
    for command in library_commands() {
        match session
            .prepare(call(
                "model_command",
                json!({"document": "live-1", "command": command.clone()}),
            ))
            .unwrap()
        {
            PreparedMessage::Tool { name, arguments } => {
                assert_eq!(name, "model_command");
                assert_eq!(arguments["command"], command);
            }
            PreparedMessage::Response(response) => panic!("{command}: {response:?}"),
        }
        session.finish(json!({"applied": true}), false).unwrap();
    }
    for name in QUERIES {
        assert!(
            live_refusal(
                &mut session,
                "model_query",
                json!({"query": {"query": name, "kind": "material"}}),
            )
            .is_none(),
            "{name}"
        );
    }
    // Off again: refused again.
    session.set_may_use_print_libraries(false);
    assert!(
        live_refusal(
            &mut session,
            "model_query",
            json!({"query": {"query": "library_items", "kind": "printer"}}),
        )
        .is_some()
    );
}

#[test]
fn the_headless_server_still_refuses_them() {
    let mut session = Session::new(Recorder::default());
    assert!(
        session
            .handle(initialize())
            .unwrap()
            .get("result")
            .is_some()
    );
    assert!(session.handle(initialized()).is_none());
    for command in library_commands() {
        let response = session
            .handle(call(
                "model_command",
                json!({"document": "D1", "command": command.clone()}),
            ))
            .unwrap();
        assert_eq!(response["result"]["isError"], true, "{command}");
        let error = response["result"]["structuredContent"]["error"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(error.ends_with(HEADLESS_REFUSED), "{error}");
    }
    for name in QUERIES {
        let response = session
            .handle(call(
                "model_query",
                json!({"document": "D1", "query": {"query": name}}),
            ))
            .unwrap();
        assert_eq!(response["result"]["isError"], true, "{name}");
    }
    assert!(session.backend.calls.is_empty());
}

#[test]
fn a_call_is_bound_to_the_open_design_s_project() {
    let project = folder("bench");
    let copy = json!({"cmd": "copy_from_print_library", "library": "project", "kind": "material", "id": "pla"});
    // Left out: the design's project is filled in.
    let bound = bind_project(&copy, &project).unwrap();
    assert_eq!(bound["project"], json!(project));
    // The same folder, also with a trailing separator or a `.` part.
    let separator = std::path::MAIN_SEPARATOR;
    for given in [
        project.clone(),
        format!("{project}{separator}"),
        format!("{project}{separator}.{separator}"),
    ] {
        let mut asked = copy.clone();
        asked["project"] = json!(given);
        assert_eq!(
            bind_project(&asked, &project).unwrap()["project"],
            json!(project),
            "{given}"
        );
    }
    // Another folder, a relative one, one with `..` or not a string: refused
    // in words.
    for given in [
        json!(folder("other")),
        json!("bench"),
        json!(format!("{project}{separator}..{separator}bench")),
        json!(7),
    ] {
        let mut asked = copy.clone();
        asked["project"] = given.clone();
        let error = bind_project(&asked, &project).unwrap_err();
        assert!(error.contains("project"), "{given}: {error}");
    }
    let mut other = copy.clone();
    other["project"] = json!(folder("other"));
    assert!(
        bind_project(&other, &project)
            .unwrap_err()
            .contains("not this design's project")
    );
    // A design in no project: none is filled in, and any folder is refused.
    let unbound = bind_project(&copy, "").unwrap();
    assert!(unbound.get("project").is_none());
    let error = bind_project(&other, "").unwrap_err();
    assert!(error.contains("in no project"), "{error}");
    // An empty or null one is as one left out.
    for given in [json!(""), Value::Null] {
        let mut asked = copy.clone();
        asked["project"] = given;
        assert_eq!(
            bind_project(&asked, &project).unwrap()["project"],
            json!(project)
        );
        assert!(bind_project(&asked, "").unwrap().get("project").is_none());
    }
    // A query is bound the same way.
    let items = json!({"query": "library_items", "kind": "material"});
    assert_eq!(
        bind_project(&items, &project).unwrap(),
        json!({"query": "library_items", "kind": "material", "project": project})
    );
}

#[test]
fn the_schemas_say_when_the_library_forms_run() {
    let all = crate::Vocabulary::default();
    for name in LIBRARY_FORMS {
        let Some(live) = crate::command_schema(name, &all, true) else {
            continue;
        };
        let restrictions = live["x-mitcad-restrictions"].to_string();
        assert!(
            restrictions.contains("Agents may use my print libraries"),
            "{name}: {restrictions}"
        );
        let headless = crate::command_schema(name, &all, false).unwrap();
        let restrictions = headless["x-mitcad-restrictions"].to_string();
        assert!(
            restrictions.contains("MCP refuses its library form"),
            "{name}: {restrictions}"
        );
    }
}
