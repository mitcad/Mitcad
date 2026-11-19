// SPDX-License-Identifier: MIT
//! The commands' JSON Schemas that MCP gives on demand (mitcad#1897,
//! `core/mcp/src/schemas.rs`) against the model's API: each is for a
//! command both servers permit, its examples are read whole by the API,
//! its fields are the ones the API lists for the command, and leaving out a
//! required field is refused as missing. A field added to or taken from a
//! command without its schema fails here.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use mitcad_mcp::{Vocabulary, command_schema, schema_commands};
use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_api::classes;

/// The names the live server permits, as the bridge gives them
/// (`core/ffi/src/mcp.rs`).
fn live() -> Vocabulary {
    let owned =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    Vocabulary {
        queries: owned(&classes::external_queries()),
        commands: owned(&classes::external_commands()),
        features: owned(<mitcad_model::features::FeatureDef>::TYPE_NAMES),
    }
}

/// The headless server's: the live server's and its two print imports.
fn headless() -> Vocabulary {
    let mut names = live();
    names.commands.extend([
        "import_print_profile".to_owned(),
        "import_slicer_presets".to_owned(),
    ]);
    names.commands.sort();
    names
}

/// The API's refusal of a command on an empty design; None when it runs.
fn refusal(command: &Value) -> Option<String> {
    let mut doc = Document::new(MockKernel::default());
    doc.command(&command.to_string())
        .err()
        .map(|error| error.to_string())
}

/// Whether a refusal is the API's reading of the command's fields, not
/// the design's answer to them (no such feature, plate or comment).
fn reading_error(error: &str) -> bool {
    [
        "unknown field",
        "missing field",
        "invalid type",
        "unknown variant",
        "invalid length",
    ]
    .iter()
    .any(|reading| error.contains(reading))
}

/// The fields the API lists for a command after an unknown one, as serde
/// writes them: `expected one of `a`, `b``, `expected `a` or `b``,
/// `expected `a``, `there are no fields`.
fn listed_fields(error: &str) -> Option<BTreeSet<String>> {
    let (_, rest) = error.split_once("unknown field `")?;
    let (_, rest) = rest.split_once("`, ")?;
    if rest.starts_with("there are no fields") {
        return Some(BTreeSet::new());
    }
    let rest = rest.strip_prefix("expected ")?;
    let rest = rest.strip_prefix("one of ").unwrap_or(rest);
    let mut pieces = rest.split('`');
    if !pieces.next()?.is_empty() {
        return None;
    }
    let mut fields = BTreeSet::new();
    while let Some(field) = pieces.next() {
        fields.insert(field.to_owned());
        if !matches!(pieces.next(), Some(", " | " or ")) {
            break;
        }
    }
    Some(fields)
}

#[test]
fn the_field_lists_of_the_api_s_errors_are_read() {
    let error = "invalid command: unknown field `zz`, expected one of `def`, `name`, `index`";
    assert_eq!(
        listed_fields(error),
        Some(BTreeSet::from([
            "def".to_owned(),
            "name".to_owned(),
            "index".to_owned()
        ]))
    );
    let two = "invalid command: unknown field `zz`, expected `plate` or `items`";
    assert_eq!(listed_fields(two).map(|fields| fields.len()), Some(2));
    let none = "invalid command: unknown field `zz`, there are no fields";
    assert_eq!(listed_fields(none), Some(BTreeSet::new()));
    assert!(reading_error(error) && !reading_error("no feature F2"));
}

/// A schema advertises only a command both servers permit; a vocabulary
/// without the command gets none.
#[test]
fn every_schema_is_for_a_command_the_servers_permit() {
    let all = schema_commands(&Vocabulary::default());
    assert!(all.contains(&"place_on_plate") && all.contains(&"add_feature"));
    let live = live();
    let headless = headless();
    for name in &all {
        assert!(
            classes::external_commands().contains(name),
            "{name} is not a command MCP permits"
        );
        let schema = command_schema(name, &live, true).unwrap();
        assert_eq!(schema["x-mitcad-server"], "live");
        let schema = command_schema(name, &headless, false).unwrap();
        assert_eq!(schema["x-mitcad-server"], "headless");
    }
    assert_eq!(schema_commands(&live), all);
    let narrow = Vocabulary {
        commands: vec!["undo".to_owned()],
        ..Vocabulary::default()
    };
    assert!(command_schema("add_feature", &narrow, true).is_none());
    assert_eq!(schema_commands(&narrow), ["undo"]);
    // A feature definition's types are the server's.
    let add = command_schema("add_feature", &live, true).unwrap();
    assert_eq!(
        add["properties"]["def"]["properties"]["type"]["enum"],
        json!(live.features)
    );
}

/// The commands with a schema whose fields the API does not read as one
/// struct of the command (mitcad#1918), with the reason: their examples are
/// read whole as every other's, but an unknown field and a missing one are
/// refused only after the design was looked at (or not at all), so the
/// empty design cannot show which fields the API reads.
const UNPROBED: &[(&str, &str)] = &[
    (
        "edit_render_light",
        "its fields are the light's, a map read after the light is looked up: on the empty design it is refused as no such light",
    ),
    (
        "edit_render_shot",
        "its fields are the shot's, a map read after the shot is looked up: on the empty design it is refused as no such shot, and without id as needing the shot's id",
    ),
    (
        "link_gears",
        "it has no command struct: link_gears_json reads its fields and refuses an unknown one as \"link_gears has no field\" and a missing gear as \"link_gears needs a gear\"",
    ),
];

/// A command MCP runs whose library form it refuses (a material or print
/// profile named in a library, mitcad#655) says so in its restrictions,
/// and no other command does (mitcad#1918).
#[test]
fn a_library_form_mcp_refuses_is_in_the_restrictions() {
    let vocabulary = live();
    let model = Some(classes::ApiClass::Model);
    for name in schema_commands(&vocabulary) {
        let plain = classes::command_class_with(&json!({"cmd": name}));
        let library = classes::command_class_with(&json!({"cmd": name, "library": "user"}));
        let refused = plain == model && library != model;
        let schema = command_schema(name, &vocabulary, true).unwrap();
        let said = schema["x-mitcad-restrictions"]
            .to_string()
            .contains("refuses its library form");
        assert_eq!(refused, said, "{name}");
    }
}

/// The drift check: each schema against the fields the API reads.
#[test]
fn the_schemas_agree_with_the_fields_the_api_reads() {
    let vocabulary = live();
    let mut problems = Vec::new();
    for (name, reason) in UNPROBED {
        if command_schema(name, &vocabulary, false).is_none() || reason.is_empty() {
            problems.push(format!("{name} is not probed without a schema or a reason"));
        }
    }
    for name in schema_commands(&vocabulary) {
        let schema = command_schema(name, &vocabulary, false).unwrap();
        let properties: BTreeSet<String> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|field| *field != "cmd")
            .cloned()
            .collect();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|field| *field != "cmd")
            .collect();
        let examples = schema["examples"].as_array().unwrap();
        // Each example is read whole: the empty design may refuse it (no
        // such feature), never for its fields.
        for example in examples {
            if let Some(error) = refusal(example)
                && reading_error(&error)
            {
                problems.push(format!("{name}: its example {example}: {error}"));
            }
        }
        if UNPROBED.iter().any(|(unprobed, _)| *unprobed == name) {
            continue;
        }
        let example = &examples[0];
        // An unknown field is refused, with the API's list of the fields.
        let mut probe = example.clone();
        probe["zz_probe"] = json!(0);
        match refusal(&probe) {
            Some(error) if error.contains("unknown field `zz_probe`") => {
                if let Some(fields) = listed_fields(&error)
                    && fields != properties
                {
                    problems.push(format!(
                        "{name}: the API reads {fields:?}, the schema has {properties:?}"
                    ));
                }
            }
            // A command without fields may ignore one it does not know.
            other if !properties.is_empty() => {
                problems.push(format!(
                    "{name}: an unknown field is not refused: {other:?}"
                ));
            }
            _ => {}
        }
        // A required field left out is refused as missing.
        for field in &required {
            let mut without = example.clone();
            without.as_object_mut().unwrap().remove(*field);
            match refusal(&without) {
                Some(error) if error.contains(&format!("missing field `{field}`")) => {}
                other => problems.push(format!("{name} without {field}: {other:?}")),
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The permitted commands without a JSON Schema, each with the reason a
/// schema cannot describe it. Every permitted command has one now
/// (mitcad#1912 the sketch commands, mitcad#1918 the others); a new
/// permitted command needs a schema in its family's module of
/// `core/mcp/src/schemas/`, or a place here with its reason.
const WITHOUT_SCHEMA: &[(&str, &str)] = &[];

/// Every command MCP permits has a schema or is listed above, never both,
/// and the list names only permitted commands.
#[test]
fn every_permitted_command_has_a_schema_or_a_listed_reason() {
    let with_schema: BTreeSet<&str> = schema_commands(&Vocabulary::default())
        .into_iter()
        .collect();
    let listed: BTreeSet<&str> = WITHOUT_SCHEMA.iter().map(|(name, _)| *name).collect();
    assert_eq!(listed.len(), WITHOUT_SCHEMA.len(), "a command listed twice");
    let permitted = classes::external_commands();
    let mut problems = Vec::new();
    for (name, reason) in WITHOUT_SCHEMA {
        if reason.is_empty() {
            problems.push(format!("{name} is listed without its reason"));
        }
    }
    for name in &permitted {
        match (with_schema.contains(name), listed.contains(name)) {
            (true, true) => problems.push(format!("{name} has a schema and is listed")),
            (false, false) => problems.push(format!("{name} has neither a schema nor a reason")),
            _ => {}
        }
    }
    for name in &listed {
        if !permitted.contains(name) {
            problems.push(format!("{name} is listed but not a command MCP permits"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    // The sketch commands have theirs (mitcad#1912), and every other
    // family's command too (mitcad#1918).
    for name in &permitted {
        assert!(with_schema.contains(name), "{name} has no schema");
    }
}
