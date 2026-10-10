// SPDX-License-Identifier: MIT
use serde_json::{Value, json};

pub(crate) fn tools() -> Vec<Value> {
    let document = json!({"type": "string", "minLength": 1, "description": "Explicit document handle returned by document_create or document_open"});
    let path = json!({"type": "string", "minLength": 1, "description": "Path within the configured workspace"});
    vec![
        tool(
            "document_create",
            "Create an empty metric CAD document and return its handle. Documents remain open until closed or the server exits.",
            json!({}),
            &[],
            false,
            false,
        ),
        tool(
            "document_open",
            "Open a Mitcad project within the configured workspace and return its document handle. The source file is not changed.",
            json!({"path": path}),
            &["path"],
            false,
            false,
        ),
        tool(
            "document_list",
            "List open document handles and their saved paths and modification state.",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "document_close",
            "Close a document. Unsaved changes prevent closing unless discard is explicitly true.",
            json!({
                "document": document,
                "discard": {"type": "boolean", "default": false, "description": "Explicitly discard unsaved changes"}
            }),
            &["document"],
            false,
            true,
        ),
        tool(
            "model_query",
            "Read a document using one existing JSON model query. Read mitcad://reference/commands for query names, arguments, identities and units.",
            json!({"document": document, "query": {"type": "object", "description": "One JSON query object from the model API reference"}}),
            &["document", "query"],
            true,
            false,
        ),
        tool(
            "model_command",
            "Apply one JSON model command to a document. Read mitcad://reference/commands first. Script arrays, file imports and external links are not supported through this tool.",
            json!({"document": document, "command": {"type": "object", "description": "One JSON command object; lengths use millimetres unless explicitly specified"}}),
            &["document", "command"],
            false,
            true,
        ),
        tool(
            "document_save",
            "Save a document as a Mitcad project inside the workspace. An existing file requires overwrite:true.",
            json!({
                "document": document,
                "path": path,
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file"}
            }),
            &["document", "path"],
            false,
            true,
        ),
        tool(
            "model_export",
            "Export geometry to a new file inside the workspace. Existing output files are refused. Optionally select bodies; DXF requires a sketch handle instead of bodies. Defaults are metric.",
            json!({
                "document": document,
                "path": path,
                "format": {"type": "string", "enum": ["step", "iges", "brep", "stl", "obj", "3mf", "dxf"]},
                "bodies": {"type": "array", "items": {"type": "string", "minLength": 1}, "description": "Optional body identities, for example F7.b0"},
                "sketch": {"type": "string", "minLength": 1, "description": "Sketch feature identity for DXF, for example F2"}
            }),
            &["document", "path", "format"],
            false,
            false,
        ),
    ]
}

fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
    destructive: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        },
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "openWorldHint": false
        }
    })
}

/// Validate the small, fixed schema vocabulary used by our tool catalog.
/// CAD command and query contents remain the document backend's responsibility.
pub(crate) fn validate(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let valid_type = match schema.get("type").and_then(Value::as_str) {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        _ => true,
    };
    if !valid_type {
        return Err(format!("{path} must be {}", schema["type"]));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        return Err(format!("{path} must be one of {}", schema["enum"]));
    }
    if let Some(string) = value.as_str()
        && let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
        && (string.chars().count() as u64) < minimum
    {
        return Err(format!("{path} must not be empty"));
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}.{key} is required"));
                }
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, value) in object {
            if let Some(property) = properties.and_then(|properties| properties.get(key)) {
                validate(value, property, &format!("{path}.{key}"))?;
            } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                return Err(format!("Unknown argument {path}.{key}"));
            }
        }
    }
    if let Some(array) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for (index, value) in array.iter().enumerate() {
            validate(value, items, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}
