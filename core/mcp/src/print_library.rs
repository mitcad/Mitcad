// SPDX-License-Identifier: MIT
//! The print libraries for live agents (mitcad#2224): My library (the
//! user's) and the project's library of materials and print profiles, and
//! the base library read from them. MCP refuses them by class (file IO,
//! `core/model-api/src/classes.rs`); the live application permits them
//! when the user allows it (Preferences › AI Assistants › Agents may use
//! my print libraries). Off, and always in the headless server, the
//! session refuses them in words before the application sees the call.
//! On, the application binds each call to the open design's own project
//! ([`bind_project`]) and its validator lets these forms through
//! ([`command_uses`], [`query_uses`]); nothing else of the file IO class.

use std::path::{Component, Path};

use serde_json::{Value, json};

/// The refusal of a print library call without the user's preference
/// (the live server).
pub const LIBRARIES_REFUSED: &str = "The print libraries are left to the user (Preferences › AI Assistants › Agents may use my print libraries); leave library and based_on out to work on the design's own materials and profiles";

/// The headless server's refusal: it has no Preferences, and keeps a
/// design's items in the design (mitcad#1570).
pub const HEADLESS_REFUSED: &str = "The print libraries are not available through the headless MCP server; leave library and based_on out to work on the design's own materials and profiles";

/// The queries that read the libraries' folders.
pub const QUERIES: &[&str] = &["library_items", "print_library"];

/// The commands that read or write the libraries' folders by name.
pub const COMMANDS: &[&str] = &[
    "copy_from_print_library",
    "update_from_print_library",
    "decline_print_library_update",
    "save_to_print_library",
];

/// The commands whose library form reads or writes a library's folder
/// (`command_class_with` in `core/model-api/src/classes.rs`, mitcad#655):
/// `library` given as a library's name, or `based_on` as an object.
pub const LIBRARY_FORMS: &[&str] = &[
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

/// Whether a command uses the print libraries in a form the preference
/// permits: one of [`COMMANDS`], or the library form of one of
/// [`LIBRARY_FORMS`]. Not `import_print_profile` or
/// `import_slicer_presets` (they read files by path), nor a Flex type from
/// a library (`flex.set_item`: not a print library).
pub fn command_uses(request: &Value) -> bool {
    let Some(name) = request.get("cmd").and_then(Value::as_str) else {
        return false;
    };
    COMMANDS.contains(&name)
        || (LIBRARY_FORMS.contains(&name)
            && (request.get("library").is_some_and(Value::is_string)
                || request.get("based_on").is_some_and(Value::is_object)))
}

/// Whether a query reads the print libraries in a form the preference
/// permits: one of [`QUERIES`]. Not `slicer_presets` (an installed
/// slicer's own folders, mitcad#830).
pub fn query_uses(request: &Value) -> bool {
    request
        .get("query")
        .and_then(Value::as_str)
        .is_some_and(|name| QUERIES.contains(&name))
}

/// A print library call bound to the open design's project (`project`, its
/// folder; empty for a design outside a project): the request with
/// `project` set to that folder (left out without one). The live server
/// never takes an agent's folder: a `project` that names another folder,
/// or any folder for a design outside a project, is refused in words.
/// Folders are compared by their parts (a trailing separator or a `.`
/// part does not matter); a relative one is another folder. An empty
/// `project` is as one left out.
pub fn bind_project(request: &Value, project: &str) -> Result<Value, String> {
    let mut request = request.clone();
    let Some(fields) = request.as_object_mut() else {
        return Ok(request);
    };
    match fields.get("project") {
        None | Some(Value::Null) => {}
        Some(Value::String(given)) if given.is_empty() => {}
        Some(Value::String(given)) if !project.is_empty() && same_folder(given, project) => {}
        Some(Value::String(given)) if project.is_empty() => {
            return Err(format!(
                "project names the folder {}, but this design is in no project: leave project out (My library and the base library need none)",
                crate::echoed(given)
            ));
        }
        Some(Value::String(given)) => {
            return Err(format!(
                "project names the folder {}, not this design's project ({project}): agents use only the open design's own project's library; leave project out",
                crate::echoed(given)
            ));
        }
        Some(_) => {
            return Err(
                "project must be the open design's project folder, a string; leave it out to use it"
                    .to_owned(),
            );
        }
    }
    if project.is_empty() {
        fields.remove("project");
    } else {
        fields.insert("project".to_owned(), json!(project));
    }
    Ok(request)
}

/// Whether two absolute folders are the same by their parts.
fn same_folder(given: &str, project: &str) -> bool {
    let given = Path::new(given);
    if !given.is_absolute() {
        return false;
    }
    let given = parts(given);
    !given.contains(&Component::ParentDir) && given == parts(Path::new(project))
}

/// A path's parts without its `.` parts.
fn parts(path: &Path) -> Vec<Component<'_>> {
    path.components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect()
}

/// The refusal of a `model_command`'s or `model_query`'s print library use
/// unless `allowed`, as the tool's error result, nothing of the call
/// having run; `live` chooses the words (the headless server has no
/// Preferences).
pub(crate) fn apply_policy(
    name: &str,
    arguments: &Value,
    allowed: bool,
    live: bool,
) -> Result<(), Value> {
    if allowed {
        return Ok(());
    }
    let refused = if live {
        LIBRARIES_REFUSED
    } else {
        HEADLESS_REFUSED
    };
    match name {
        "model_query" => {
            let query = &arguments["query"];
            if query_uses(query) {
                let kind = query["query"].as_str().unwrap_or_default();
                return Err(json!({"error": format!("query {kind}: {refused}")}));
            }
            Ok(())
        }
        "model_command" => {
            let array = arguments["command"].is_array();
            let entries: Vec<&Value> = match &arguments["command"] {
                Value::Array(entries) => entries.iter().collect(),
                command if command.is_object() => vec![command],
                _ => return Ok(()),
            };
            for (i, entry) in entries.into_iter().enumerate() {
                if !command_uses(entry) {
                    continue;
                }
                let kind = entry["cmd"].as_str().unwrap_or_default();
                let mut refusal = json!({"applied": false});
                if array {
                    refusal["error"] = json!(format!(
                        "commands[{i}]: {kind}: {refused}; nothing of the array ran"
                    ));
                    refusal["entry"] = json!(i);
                    refusal["undone"] = json!(0);
                } else {
                    refusal["error"] = json!(format!("{kind}: {refused}"));
                }
                return Err(refusal);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "print_library_tests.rs"]
mod tests;
