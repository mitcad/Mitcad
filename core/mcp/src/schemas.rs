// SPDX-License-Identifier: MIT
//! Machine-readable JSON Schemas of commands (mitcad#1897): what
//! `model_reference {"command": "<name>"}` answers beside the command's
//! reference entry, on demand, so that the tool listing and the server's
//! instructions stay short (mitcad#1506, mitcad#1528).
//!
//! Each schema is written here from the command's fields as the model's
//! API reads them (`core/model-api`: serde with `deny_unknown_fields`):
//! the fields and which are required, their types, enums, units and
//! defaults, the alternatives, examples, and the reference of a nested
//! feature definition. A schema is given only for a command the server
//! permits (its vocabulary), with the restrictions of that server (live or
//! headless). The drift test `core/model-tests/api/src/mcp_schema_tests.rs`
//! runs each schema's examples through the model's API and checks its
//! fields against the API's own list of a command's fields.

use serde_json::{Map, Value, json};

use crate::reference::Vocabulary;

// The sketch commands' schemas (mitcad#1912).
mod sketch;

// The other families' schemas (mitcad#1918), one module per family of
// commands, each with its `COMMANDS` and `spec` as `sketch`.
mod annotations;
mod appearance;
mod assembly;
mod build;
mod cam;
mod data;
mod document;
mod drawings;
mod form;
mod joints;
mod print;

/// A family's module: its commands with a schema, in the reference's
/// order, and its schemas as [`spec`] writes them.
type Family = (&'static [&'static str], fn(&str) -> Option<Value>);

/// The families' modules, after the commands of [`COMMANDS`].
const FAMILIES: &[Family] = &[
    (sketch::COMMANDS, sketch::spec),
    (document::COMMANDS, document::spec),
    (appearance::COMMANDS, appearance::spec),
    (print::COMMANDS, print::spec),
    (data::COMMANDS, data::spec),
    (assembly::COMMANDS, assembly::spec),
    (joints::COMMANDS, joints::spec),
    (build::COMMANDS, build::spec),
    (drawings::COMMANDS, drawings::spec),
    (annotations::COMMANDS, annotations::spec),
    (cam::COMMANDS, cam::spec),
    (form::COMMANDS, form::spec),
];

/// The JSON Schema dialect of the schemas.
pub(crate) const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// The commands with a schema, in the reference's order, besides those of
/// the families' modules ([`FAMILIES`]). A new one needs a row here (or in
/// its family's `COMMANDS`) and its arm in [`spec`] (or its family's
/// `spec`); a test checks both.
pub(crate) const COMMANDS: &[&str] = &[
    "add_feature",
    "edit_feature",
    "delete_feature",
    "suppress_feature",
    "rename_feature",
    "add_parameter",
    "set_parameter",
    "undo",
    "redo",
    "address_comment",
    "add_print_plate",
    "edit_print_plate",
    "move_print_plate",
    "delete_print_plate",
    "place_on_plate",
    "remove_from_plates",
];

/// What every command of a `model_command` array may carry besides its own
/// fields: the array resolves them before the command reads its fields.
const ARRAY_NOTE: &str = "In a model_command array an entry may also name its result with \"as\": \"<name>\", and a later entry may give a value as {\"ref\": \"<name>.<path>\"} (model_reference {\"section\": \"Array\"}); the array resolves both before this schema applies.";

/// The restrictions of the live server's model_command, for every command.
const LIVE: &str = "Live: one undo step labeled (agent); refused while the document is read-only; while a sketch is open, only sketch commands on it, edit_feature of it, undo and redo run, the others wait until it is finished.";

/// The commands whose library form MCP refuses (`command_class_with` in
/// `core/model-api/src/classes.rs`): a library named in `library` (or a
/// `based_on` object) is read from or written to its folder.
const LIBRARY_FORMS: &[&str] = &[
    "create_material",
    "edit_material",
    "delete_material",
    "merge_materials",
    "import_materials",
    "create_dataset_materials",
    "set_component_material",
    "set_body_material",
    "create_print_profile",
    "edit_print_profile",
    "delete_print_profile",
    "set_property",
];

/// The restriction of [`LIBRARY_FORMS`].
const LIBRARY_FORM: &str = "MCP refuses its library form: library given as a library's name (or based_on as an object) reads or writes that library's folder; leave them out to work on the design's own items.";

/// The restrictions of the headless server's model_command, for every
/// command.
const HEADLESS: &str = "Headless: the document handle is required.";

/// A feature id, `F<number>`.
fn feature_uid(description: &str) -> Value {
    json!({"type": "string", "pattern": "^F(0|[1-9][0-9]*)$", "description": description})
}

/// A value: a number in mm (lengths) or radians (angles), or text read as
/// an expression with units and parameters.
fn value_input(description: &str) -> Value {
    json!({
        "type": ["number", "string"],
        "description": format!("{description}: a number in mm (a length) or radians (an angle), or text read as an expression with units and parameters (\"40 mm\", \"w / 2\", \"90 deg\")")
    })
}

/// A parameter limit as `limits.rs` reads it: a value, or null or "" to
/// remove it.
fn limit(description: &str) -> Value {
    json!({"type": ["number", "string", "null"], "description": format!("{description} (a value or an expression; null or \"\" removes it; model_reference {{\"section\": \"Parameter limits\"}})")})
}

/// A list of ids or names.
fn ids(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": description})
}

/// An id or a name.
fn id(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

/// A switch with its default.
fn flag(default: bool, description: &str) -> Value {
    json!({"type": "boolean", "default": default, "description": description})
}

fn number(description: &str) -> Value {
    json!({"type": "number", "description": description})
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn object(description: &str) -> Value {
    json!({"type": "object", "description": description})
}

/// One of a closed set of words.
fn choice(values: &[&str], description: &str) -> Value {
    json!({"type": "string", "enum": values, "description": description})
}

/// A body or an occurrence on a print plate.
fn plate_item(with_count: bool) -> Value {
    let mut properties = json!({
        "body": {"type": "string", "minLength": 1, "description": "A printed body by its uid (F2.b0) or name"},
        "occurrence": {"type": "string", "minLength": 1, "description": "An occurrence by its path of uids or names (\"O1/O4\", \"Arm:1/Pin:2\"): its printed bodies as assembled"}
    });
    if with_count {
        properties["count"] = json!({"type": "integer", "minimum": 1, "maximum": 999, "description": "Copies printed; left out: the count the item has (here or on the plate it leaves), else 1"});
    } else {
        properties["count"] = json!({"type": "integer", "description": "Not read"});
    }
    json!({
        "type": "object",
        "properties": properties,
        "oneOf": [{"required": ["body"]}, {"required": ["occurrence"]}],
        "additionalProperties": false
    })
}

/// A command's schema as written here: `description`, `properties` (its
/// fields without `cmd`), `required`, `examples` and, where it has its
/// own, `restrictions`.
fn spec(name: &str) -> Option<Value> {
    let plate = || json!({"type": "string", "minLength": 1, "description": "A print plate by its id (plate2) or name"});
    let spec = match name {
        "add_feature" => json!({
            "description": "Adds a feature at the timeline marker (or after/index) and recomputes. Result: uid, name, parameters created.",
            "properties": {
                "def": {},
                "name": {"type": "string", "description": "The feature's name; default the type's next name (Extrude1)"},
                "component": {"type": "string", "description": "The component (uid or name) it goes into; the active component when left out"},
                "after": feature_uid("Inserted just after this feature instead of at the marker"),
                "index": {"type": "integer", "minimum": 0, "description": "Inserted at this timeline position (the number of features before it) instead of at the marker"},
                "fit_faces": {"type": "boolean", "default": false, "description": "A thread whose faces do not fit it gets them fitted in the same undo step"},
                "shared": {"type": "string", "enum": ["parameter", "own"], "description": "With fit_faces: change a named parameter used elsewhere too (parameter) or give the dimension its own value (own)"}
            },
            "required": ["def"],
            "examples": [
                {"cmd": "add_feature", "name": "Base", "def": {"type": "extrude",
                  "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
                  "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
            ]
        }),
        "edit_feature" => json!({
            "description": "Replaces a feature's definition (the same type) and recomputes. Result: parameters created.",
            "properties": {
                "uid": feature_uid("The feature (F<number>; a name is refused)"),
                "def": {}
            },
            "required": ["uid", "def"],
            "examples": [
                {"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
                  "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
                  "extent": {"type": "distance", "distance": "h"}, "operation": "new_body"}}
            ]
        }),
        "delete_feature" => json!({
            "description": "Deletes a feature. Result: deleted (uids in timeline order).",
            "properties": {
                "uid": feature_uid("The feature"),
                "dependents": {"type": "boolean", "default": false, "description": "Also delete the features that depend on it"}
            },
            "required": ["uid"],
            "examples": [{"cmd": "delete_feature", "uid": "F3"}, {"cmd": "delete_feature", "uid": "F1", "dependents": true}]
        }),
        "suppress_feature" => json!({
            "description": "Suppresses a feature, takes the suppression away, or makes it follow a condition.",
            "properties": {
                "uid": feature_uid("The feature"),
                "suppressed": {"type": "boolean", "default": true, "description": "true suppresses, false takes the suppression away"},
                "expression": {"type": "string", "description": "Instead of suppressed: a condition the suppression follows (Holes = 'No'); \"\" removes it"}
            },
            "required": ["uid"],
            "examples": [{"cmd": "suppress_feature", "uid": "F3"}, {"cmd": "suppress_feature", "uid": "F3", "suppressed": false}]
        }),
        "rename_feature" => json!({
            "description": "Renames a feature.",
            "properties": {
                "uid": feature_uid("The feature"),
                "name": {"type": "string", "minLength": 1, "description": "Its new name"}
            },
            "required": ["uid", "name"],
            "examples": [{"cmd": "rename_feature", "uid": "F2", "name": "Base"}]
        }),
        "add_parameter" => json!({
            "description": "Adds a named user parameter.",
            "properties": {
                "name": {"type": "string", "minLength": 1, "description": "Its name: not a unit, function or constant (round, mm, pi)"},
                "value": value_input("Its value"),
                "expression": {"type": "string", "description": "Instead of value: an expression"},
                "unit": {"type": "string", "description": "Its unit (mm, in, deg, ...); left out: from the value"},
                "comment": {"type": "string", "default": "", "description": "A comment"},
                "min": limit("The least value"),
                "max": limit("The greatest value"),
                "step": limit("The step between allowed values"),
                "strict": {"type": "boolean", "description": "Refuse values outside the limits instead of warning"},
                "values": {"type": ["array", "null"], "description": "The allowed values (expression texts or numbers); null or [] removes them"}
            },
            "required": ["name"],
            "examples": [{"cmd": "add_parameter", "name": "w", "value": "40 mm", "comment": "Width"}, {"cmd": "add_parameter", "name": "half", "expression": "w / 2"}]
        }),
        "set_parameter" => json!({
            "description": "Changes a parameter. Result: changed (the parameters whose values changed).",
            "properties": {
                "name": {"type": "string", "minLength": 1, "description": "The parameter"},
                "value": value_input("Its new value, written in the parameter's unit"),
                "expression": {"type": "string", "description": "Instead of value: an expression"},
                "unit": {"type": "string", "description": "Its unit"},
                "comment": {"type": "string", "description": "Its comment"},
                "favorite": {"type": "boolean", "description": "A favourite"},
                "min": limit("The least value"),
                "max": limit("The greatest value"),
                "step": limit("The step between allowed values"),
                "strict": {"type": "boolean", "description": "Refuse values outside the limits instead of warning"},
                "values": {"type": ["array", "null"], "description": "The allowed values; null or [] removes them"}
            },
            "required": ["name"],
            "examples": [{"cmd": "set_parameter", "name": "w", "value": 50}, {"cmd": "set_parameter", "name": "w", "expression": "2 * h"}]
        }),
        "undo" => json!({
            "description": "Undoes the last undo step. Result: label (null when there was none).",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "undo"}],
            "restrictions": ["Runs alone, not in a model_command array."]
        }),
        "redo" => json!({
            "description": "Redoes the last undone step. Result: label (null when there was none).",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "redo"}],
            "restrictions": ["Runs alone, not in a model_command array."]
        }),
        "address_comment" => json!({
            "description": "Marks an open comment addressed, saying what was done; put it in the same model_command array as the change. Resolving is left to the user.",
            "properties": {
                "id": {"type": "string", "minLength": 1, "description": "The comment (C3)"},
                "resolution": {"type": "string", "minLength": 1, "description": "What was done, briefly"},
                "author": {"type": "string", "description": "Set by MCP to the client's name; a value given is replaced"}
            },
            "required": ["id", "resolution"],
            "examples": [{"cmd": "address_comment", "id": "C3", "resolution": "Made the hole an M6 thread"}]
        }),
        "add_print_plate" => json!({
            "description": "Adds a print plate. Result: id, name.",
            "properties": {
                "name": {"type": "string", "minLength": 1, "description": "Its name, unique ignoring case; default Plate <n>"}
            },
            "required": [],
            "examples": [{"cmd": "add_print_plate", "name": "Vase"}]
        }),
        "edit_print_plate" => json!({
            "description": "Renames a print plate and sets or takes away its own print process values.",
            "properties": {
                "plate": plate(),
                "name": {"type": "string", "minLength": 1, "description": "Its new name"},
                "values": {"type": "object", "additionalProperties": true, "description": "Print Process values by their keys in the key map's JSON form (spiral_vase, layer_height, perimeter_generator; model_reference {\"query\": \"print_keys\"}); a key's null takes its value away"}
            },
            "required": ["plate"],
            "examples": [{"cmd": "edit_print_plate", "plate": "Vase", "name": "Vases", "values": {"spiral_vase": true, "layer_height": null}}]
        }),
        "move_print_plate" => json!({
            "description": "Moves a print plate in the order.",
            "properties": {
                "plate": plate(),
                "index": {"type": "integer", "minimum": 0, "description": "Its place in the order, from 0"}
            },
            "required": ["plate", "index"],
            "examples": [{"cmd": "move_print_plate", "plate": "Vase", "index": 0}]
        }),
        "delete_print_plate" => json!({
            "description": "Deletes a print plate; what was on it goes on no plate (the first one's).",
            "properties": {"plate": plate()},
            "required": ["plate"],
            "examples": [{"cmd": "delete_print_plate", "plate": "Vase"}]
        }),
        "place_on_plate" => json!({
            "description": "Puts bodies and occurrences on a print plate; an item on another plate moves. Result: moved (each item that left another plate, with from and to) and warnings.",
            "properties": {
                "plate": plate(),
                "items": {"type": "array", "minItems": 1, "items": plate_item(true)}
            },
            "required": ["plate", "items"],
            "examples": [{"cmd": "place_on_plate", "plate": "Vase", "items": [{"body": "Body1", "count": 4}, {"occurrence": "Arm:1"}]}]
        }),
        "remove_from_plates" => json!({
            "description": "Takes bodies and occurrences off their print plates.",
            "properties": {
                "items": {"type": "array", "minItems": 1, "items": plate_item(false)}
            },
            "required": ["items"],
            "examples": [{"cmd": "remove_from_plates", "items": [{"occurrence": "O1"}]}]
        }),
        _ => return FAMILIES.iter().find_map(|(_, spec)| spec(name)),
    };
    Some(spec)
}

/// Every command with a schema, in the reference's order.
fn all_commands() -> impl Iterator<Item = &'static str> {
    COMMANDS
        .iter()
        .chain(FAMILIES.iter().flat_map(|(commands, _)| commands.iter()))
        .copied()
}

/// A feature definition's schema: its type, from the feature types the
/// server permits, and where each type's fields are.
fn feature_def(vocabulary: &Vocabulary) -> Value {
    let mut kind = json!({"type": "string", "description": "The feature type"});
    if !vocabulary.features.is_empty() {
        kind["enum"] = json!(vocabulary.features);
    }
    json!({
        "type": "object",
        "description": "A feature definition: its type and that type's fields. model_reference {\"feature\": \"<type>\"} gives a type's fields and an example definition.",
        "properties": {"type": kind},
        "required": ["type"],
        "x-mitcad-reference": "model_reference {\"feature\": \"<type>\"}"
    })
}

/// The commands with a schema that the server permits (all of them when
/// its vocabulary does not say).
pub fn schema_commands(vocabulary: &Vocabulary) -> Vec<&'static str> {
    all_commands()
        .filter(|name| permits(vocabulary, name))
        .collect()
}

fn permits(vocabulary: &Vocabulary, name: &str) -> bool {
    vocabulary.commands.is_empty() || vocabulary.commands.iter().any(|known| known == name)
}

/// A command's JSON Schema for a server (`live` or headless) with this
/// vocabulary; None for a command without one, or one the server does not
/// permit.
pub fn command_schema(name: &str, vocabulary: &Vocabulary, live: bool) -> Option<Value> {
    if !permits(vocabulary, name) {
        return None;
    }
    let spec = spec(name)?;
    let mut properties = Map::new();
    properties.insert("cmd".to_owned(), json!({"const": name}));
    if let Some(own) = spec["properties"].as_object() {
        for (field, schema) in own {
            // A feature definition is written `"def": {}` in a spec; another
            // command's `def` (an analysis) has its own schema.
            let feature = schema.as_object().is_some_and(Map::is_empty);
            let schema = if field == "def" && feature {
                feature_def(vocabulary)
            } else {
                schema.clone()
            };
            properties.insert(field.clone(), schema);
        }
    }
    let mut required = vec![json!("cmd")];
    required.extend(spec["required"].as_array().cloned().unwrap_or_default());
    let mut restrictions: Vec<Value> = spec["restrictions"].as_array().cloned().unwrap_or_default();
    if LIBRARY_FORMS.contains(&name) {
        restrictions.push(json!(LIBRARY_FORM));
    }
    restrictions.push(json!(if live { LIVE } else { HEADLESS }));
    restrictions.push(json!(ARRAY_NOTE));
    Some(json!({
        "$schema": DIALECT,
        "title": name,
        "description": format!(
            "{} Its reference: model_reference {{\"command\": \"{name}\"}}.",
            spec["description"].as_str().unwrap_or_default()
        ),
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
        "examples": spec["examples"],
        "x-mitcad-server": if live { "live" } else { "headless" },
        "x-mitcad-restrictions": restrictions
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_command_has_a_schema_and_its_examples_name_it() {
        let all = Vocabulary::default();
        for name in all_commands() {
            let schema =
                command_schema(name, &all, false).unwrap_or_else(|| panic!("{name}: no schema"));
            assert_eq!(schema["$schema"], DIALECT);
            assert_eq!(schema["properties"]["cmd"]["const"], name);
            assert_eq!(schema["additionalProperties"], false);
            let properties = schema["properties"].as_object().unwrap();
            for required in schema["required"].as_array().unwrap() {
                assert!(
                    properties.contains_key(required.as_str().unwrap()),
                    "{name}: {required} is required but not a property"
                );
            }
            let examples = schema["examples"].as_array().unwrap();
            assert!(!examples.is_empty(), "{name}: no example");
            for example in examples {
                assert_eq!(example["cmd"], name, "{name}: {example}");
                for field in example.as_object().unwrap().keys() {
                    assert!(properties.contains_key(field), "{name}: {field}");
                }
                for required in schema["required"].as_array().unwrap() {
                    assert!(
                        example.get(required.as_str().unwrap()).is_some(),
                        "{name}: an example without {required}"
                    );
                }
            }
            assert!(
                schema["description"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("{{\"command\": \"{name}\"}}"))
            );
        }
        assert!(spec("no_such_command").is_none());
        assert!(command_schema("no_such_command", &all, false).is_none());
    }

    #[test]
    fn a_schema_follows_the_server_and_its_vocabulary() {
        let vocabulary = Vocabulary {
            queries: vec![],
            commands: vec!["add_feature".to_owned(), "undo".to_owned()],
            features: vec!["extrude".to_owned(), "fillet".to_owned()],
        };
        // A command the server does not permit has no schema.
        assert!(command_schema("place_on_plate", &vocabulary, false).is_none());
        assert_eq!(schema_commands(&vocabulary), ["add_feature", "undo"]);
        let headless = command_schema("add_feature", &vocabulary, false).unwrap();
        let live = command_schema("add_feature", &vocabulary, true).unwrap();
        assert_eq!(headless["x-mitcad-server"], "headless");
        assert_eq!(live["x-mitcad-server"], "live");
        // The feature types are the server's, with where their fields are.
        let def = &live["properties"]["def"];
        assert_eq!(
            def["properties"]["type"]["enum"],
            json!(["extrude", "fillet"])
        );
        assert!(
            def["x-mitcad-reference"]
                .as_str()
                .unwrap()
                .contains("\"feature\"")
        );
        let restrictions = live["x-mitcad-restrictions"].to_string();
        assert!(restrictions.contains("read-only"));
        assert!(restrictions.contains("\\\"as\\\""));
        let undo = command_schema("undo", &vocabulary, true).unwrap();
        assert!(
            undo["x-mitcad-restrictions"]
                .to_string()
                .contains("Runs alone")
        );
    }
}
