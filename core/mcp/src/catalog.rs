// SPDX-License-Identifier: MIT
use serde_json::{Value, json};

use crate::reference::echoed;

// The longest strings the tools take (mitcad#935), in characters: a
// handle or a command id, an owner, occurrence or other name, an entity's
// topological name (it nests the features it came through), a path.
const ID_CHARS: u64 = 256;
const NAME_CHARS: u64 = 1024;
const ENTITY_CHARS: u64 = 4096;
const PATH_CHARS: u64 = 4096;

pub(crate) fn tools() -> Vec<Value> {
    let document = json!({"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "Explicit document handle returned by document_create or document_open"});
    let path = json!({"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "Path within the configured workspace"});
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
            "Open a Mitcad project within the configured workspace and return its document handle. The source file is not changed. A SolidCraft design (.solidcraft) is imported with its history into a new, unsaved document (save it with document_save and a path).",
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
            "Read a document using one existing JSON model query. model_reference lists the query names and gives each one's arguments and result (identities and units too). print_thickness answers its summary only (thinnest wall, where, regions, thin_triangles) unless the query sets triangles (true, with max_triangles to cap them).",
            json!({"document": document, "query": query_schema()}),
            &["document", "query"],
            true,
            false,
        ),
        tool(
            "model_command",
            "Apply a JSON model command, or an array of them in order, to a document. Look up the command, or the feature type of add_feature, with model_reference first. An array stops at the first refused entry (the entries before it are undone, nothing of the array stays; the result names the entry) or at the first entry whose added or edited feature fails (applied; stopped_at names it). File imports and external links are not supported through this tool. isError is true only when a command is refused or a feature it added or edited fails; failed_features lists every failed feature in the timeline (edited, new). The result holds the document's state and what the command changed (changes: features added, changed, recomputed or failed, bodies added or changed, what was removed); timeline: full gives the whole timeline and bodies instead, none neither.",
            json!({
                "document": document,
                "command": command_schema(),
                "timeline": timeline_schema()
            }),
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
            "Export geometry to a new file inside the workspace. Existing output files are refused. Optionally select bodies; DXF requires a sketch handle, or a sheet-metal body whose Flat Pattern is written, instead of bodies, unless drawing or cut_sheet is given; cut_sheet writes a cut sheet kept in the design (DXF or SVG at 1:1, model_reference {\"section\": \"Cut sheets\"}). PDF and SVG (and DXF with drawing) write a drawing of the bodies: views with hidden lines removed, overall dimensions, hole diameters, a hole table and a title block; drawing holds the export_drawing options (sheet, orientation, scale, projection, views, section, hidden_lines, ...; model_reference {\"section\": \"Drawings\"}). Defaults are metric.",
            json!({
                "document": document,
                "path": path,
                "format": {"type": "string", "enum": ["step", "iges", "brep", "stl", "obj", "3mf", "usdz", "dxf", "pdf", "svg"]},
                "bodies": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS}, "description": "Optional body identities, for example F7.b0"},
                "sketch": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "Sketch feature identity for DXF, for example F2"},
                "flat_pattern": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "Sheet-metal body identity for a Flat Pattern DXF, for example F3.b0"},
                "drawing": {"type": "object", "description": "Drawing options (export_drawing without cmd, path and bodies), for example {\"sheet\": \"A3\", \"views\": [\"front\", \"top\", \"right\", \"iso\"], \"hidden_lines\": true}"},
                "cut_sheet": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A cut sheet kept in the design (its id like D2, or its name) written as DXF or SVG: its parts' outlines at 1:1 on the layers CUT_OUTER and CUT_INNER"},
                "labels": {"type": "boolean", "default": false, "description": "With cut_sheet: each part's name as text on the layer LABEL"}
            }),
            &["document", "path", "format"],
            false,
            false,
        ),
        reference_tool(),
    ]
}

/// The command reference in parts (mitcad#714), in both servers; the
/// protocol answers it without the backend.
fn reference_tool() -> Value {
    tool(
        "model_reference",
        "Look up the model API reference: commands, queries, feature definitions, values and units, identities. Without arguments: the contents (the sections' headings) and the names of every query, command and feature type this server permits. With one of: section (a heading or a name it lists, e.g. \"Values\", \"split_body\", \"Sketch commands\"), query or command (its reference row: fields and result), feature (a feature type's section with an example definition), search (the sections and excerpts that contain a text). Each answer holds at most 64 kB of text; a longer one says truncated and gives next_offset, to pass as offset for the rest. The resource mitcad://reference/commands is the whole reference (about 1 MB).",
        json!({
            "section": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A section's heading, or a name its heading lists, e.g. \"Values\", \"split_body\" or \"Sketch commands\""},
            "query": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A query's name, e.g. \"bodies\" or \"faces\""},
            "command": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A command's name (its cmd), e.g. \"add_feature\" or \"sketch.add_line\""},
            "feature": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A feature type (a definition's type), e.g. \"extrude\" or \"construction_plane\""},
            "search": {"type": "string", "minLength": 2, "maxLength": NAME_CHARS, "description": "A text to find in any case, e.g. \"bbox\""},
            "offset": {"type": "integer", "minimum": 0, "description": "Where a truncated answer continues: its next_offset"}
        }),
        &[],
        true,
        false,
    )
}

/// `model_command`'s command: an object, or an array of them run in order
/// (mitcad#689).
fn command_schema() -> Value {
    json!({
        "type": ["object", "array"],
        "items": {"type": "object"},
        "minItems": 1,
        "maxItems": 1000,
        "description": "One JSON command object {\"cmd\": \"<name>\", ...its fields}, for example {\"cmd\": \"add_feature\", \"def\": {...}}, or an array of them run in order. JSON numbers are millimetres and radians; write \"90 deg\" for degrees. model_reference gives each command's fields."
    })
}

/// `model_query`'s query (mitcad#714).
fn query_schema() -> Value {
    json!({
        "type": "object",
        "description": "One JSON query object {\"query\": \"<name>\", ...its fields}, for example {\"query\": \"bodies\", \"properties\": true}; model_reference lists the queries and gives each one's fields"
    })
}

/// How much of the document `model_command` answers with (mitcad#689).
fn timeline_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["changed", "full", "none"],
        "default": "changed",
        "description": "changed: what the command changed (default); full: the whole timeline and bodies; none: only the document's state"
    })
}

pub(crate) fn live_tools() -> Vec<Value> {
    let mut tools: Vec<Value> = tools()
        .into_iter()
        .filter(|tool| {
            matches!(
                tool["name"].as_str(),
                Some(
                    "document_list"
                        | "model_query"
                        | "model_command"
                        | "model_export"
                        | "model_reference"
                )
            )
        })
        .collect();
    for tool in &mut tools {
        match tool["name"].as_str() {
            Some("document_list") => {
                tool["description"] = json!(
                    "List the application's open document (its handle, path, name and state) and the folders the user allows agents to save to and open from (folders; empty: none allowed)."
                );
            }
            Some("model_query") => {
                tool["description"] = json!(
                    "Read the open document with one JSON model query, also while a sketch or command is open. Refused only while the model computes: the result then says busy and retry_after_ms. model_reference lists the query names and gives each one's arguments and result (identities and units too)."
                );
            }
            Some("model_command") => {
                tool["description"] = json!(
                    "Apply a JSON model command, or an array of them in order, to the open document through its normal undo workflow; the change is one undo step labelled (agent) in the undo text. While a sketch is open, sketch.* commands on it (and edit_feature of it, undo, redo) go through the sketch session as the user's would; other commands wait until it is finished. Look up the command, or the feature type of add_feature, with model_reference first. File, remote and internal commands are refused. An array stops at the first refused entry (the entries before it are undone, nothing of the array stays; the result names the entry) or at the first entry whose added or edited feature fails (applied; stopped_at names it); undo and redo run alone. isError is true only when a command is refused or a feature it added or edited fails; failed_features lists every failed feature in the timeline (edited, new). The result holds the document's state and what the command changed (changes: features added, changed, recomputed or failed, bodies added or changed, what was removed); timeline: full gives the whole timeline and bodies instead, none neither."
                );
            }
            // Into the folders for designs (mitcad#689), as
            // document_save's path.
            Some("model_export") => {
                tool["description"] = json!(
                    "Export the open document's geometry to a new file in a folder the user allows agents to use (document_list lists them); the document does not change. An existing file is refused. bodies selects what is written (for example leaving out a helper body). DXF requires a sketch handle, or a sheet-metal body whose Flat Pattern is written, instead of bodies, unless drawing or cut_sheet is given; cut_sheet writes a cut sheet kept in the design (DXF or SVG at 1:1, model_reference {\"section\": \"Cut sheets\"}). PDF and SVG (and DXF with drawing) write a drawing of the bodies: views with hidden lines removed, overall dimensions, hole diameters, a hole table and a title block; drawing holds the export_drawing options (model_reference {\"section\": \"Drawings\"}). Refused while a command panel is open, while a sketch in the middle of the timeline is edited (its later features are rolled back until it is finished), or when the document has failed features. Defaults are metric."
                );
                tool["inputSchema"]["properties"]["path"]["description"] = json!(
                    "A new file in a folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder), with the format's extension; a relative path is relative to the first allowed folder; '..' is refused"
                );
            }
            _ => {}
        }
        if let Some(document) = tool["inputSchema"]["properties"].get_mut("document") {
            document["description"] = json!(LIVE_DOCUMENT);
        }
    }
    tools.extend(ui_tools());
    tools
}

const LIVE_DOCUMENT: &str = "Explicit live document handle returned by document_list";

/// The application's own state and actions on the open document
/// (mitcad#265); the application executes them.
fn ui_tools() -> Vec<Value> {
    let document = json!({"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": LIVE_DOCUMENT});
    let kinds = json!([
        "face",
        "edge",
        "vertex",
        "body",
        "profile",
        "sketch_curve",
        "sketch_point",
        "sketch_constraint",
        "sketch_dimension",
        "plane",
        "axis",
        "point",
        "component",
        "sketch",
        "feature"
    ]);
    let item = json!({
        "type": "object",
        "description": "An entity as ui_state lists the selection: kind, owner (a body such as F2.b0, a sketch such as F4, a datum, a component), name (a face, edge or vertex name, a region key, a curve, point, constraint or dimension id; empty for bodies, datums and sketches), occurrence (\"O1/O4\"; empty in the root)",
        "properties": {
            "kind": {"type": "string", "enum": kinds},
            "owner": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS},
            "name": {"type": "string", "maxLength": ENTITY_CHARS},
            "occurrence": {"type": "string", "maxLength": NAME_CHARS}
        },
        "required": ["kind", "owner"],
        "additionalProperties": false
    });
    let folder_path = json!({"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "A .mitcad file in a folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder); a relative path is relative to the first allowed folder; '..' is refused"});
    let point = json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": "x, y, z in millimetres"});
    vec![
        tool(
            "document_save",
            "Save the open document. Without path: to its own path as File > Save does, with the project's version and commit message when it is in a project. With path: as Save As, to that file in a folder the user allows agents to use (document_list lists them); the document then is that file. An existing file requires overwrite: true. Refused for a read-only document, while a command panel is open and while a dialog would have to ask the user.",
            json!({
                "document": document,
                "path": folder_path,
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file"}
            }),
            &["document"],
            false,
            true,
        ),
        tool(
            "document_open",
            "Open a Mitcad design (.mitcad) in the application's window in place of the open document, as File > Open does, from a folder the user allows agents to use (document_list lists them). The previous document handle becomes invalid; the result carries the new one. Refused while the open document has unsaved changes unless discard is true, while a sketch or command panel is open, and while a dialog would have to ask the user.",
            json!({
                "path": folder_path,
                "discard": {"type": "boolean", "default": false, "description": "Explicitly discard the open document's unsaved changes (they and their recovery data are lost)"}
            }),
            &["path"],
            false,
            true,
        ),
        tool(
            "ui_state",
            "The application's state: mode (idle, sketch, command, pick_plane), the active component, the sketch being edited with its Sketch Palette status and warnings (each with the entities it is about), the running command with its inputs and panel messages, the selection, the hint or error line, read-only state, unsaved changes, undo and redo labels, connected agents, and the application's version and build.",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "ui_log",
            "The application's latest log lines (its own messages, command refusals), each with a sequence number. Pass since to get only lines after that number.",
            json!({
                "since": {"type": "integer", "minimum": 0, "description": "Only lines with a larger sequence number"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 2000, "default": 200, "description": "At most this many of the newest lines"}
            }),
            &[],
            true,
            false,
        ),
        tool(
            "ui_screenshot",
            "An image of the 3D view as the user sees it, including the sketch overlay, or of the whole window, scaled to fit max_size pixels and the response limit, returned as an MCP image (PNG, or JPEG with format). With path it is written to that file in a folder the user allows agents to use, and only the path, the size and the bytes are returned. width and height, or aspect, render the 3D view offscreen at that size, without the widgets over it, so the picture does not take the window's shape; frame it first with ui_camera's aspect.",
            json!({
                "window": {"type": "boolean", "default": false, "description": "The whole window instead of the 3D view"},
                "max_size": {"type": "integer", "minimum": 64, "maximum": 4096, "default": 1280, "description": "The longer side's largest size in pixels"},
                "format": {"type": "string", "enum": ["png", "jpeg"], "description": "png (the default, or by the path's extension) or jpeg"},
                "quality": {"type": "integer", "minimum": 1, "maximum": 100, "default": 90, "description": "A JPEG's quality"},
                "width": {"type": "integer", "minimum": 16, "maximum": 4096, "description": "Pixels across an offscreen render of the 3D view (with height, or with aspect)"},
                "height": {"type": "integer", "minimum": 16, "maximum": 4096, "description": "Pixels down an offscreen render of the 3D view (with width, or with aspect)"},
                "aspect": {"type": "number", "minimum": 0.1, "maximum": 10, "description": "Width / height of an offscreen render of the 3D view; max_size on its longer side unless width or height is given"},
                "path": {"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "A .png or .jpg file in a folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder) to write the image to instead of returning it; a relative path is relative to the first allowed folder; '..' is refused"},
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file at path"}
            }),
            &[],
            false,
            false,
        ),
        tool(
            "ui_select",
            "Set the application's selection to the given entities, as clicks would, to show the user something; the user can then press Delete or run a command on them. Optionally pan or zoom the view to them.",
            json!({
                "document": document,
                "items": {"type": "array", "items": item, "description": "The entities; empty clears the selection"},
                "reveal": {"type": "string", "enum": ["none", "pan", "fit"], "default": "pan", "description": "pan: move the view to them when none is in sight, keeping the zoom unless they do not fit; fit: zoom to them"}
            }),
            &["document", "items"],
            false,
            false,
        ),
        tool(
            "ui_command",
            "Run a command of the application's command registry by its id (as app/COMMANDS.md lists them: sketch.clean_up_stale_projections, sketch.project, sketch.finish, ...) with input values, then OK or Cancel; or with edit, open a feature for editing (a sketch in sketch mode) as a double-click in the timeline does; or with only finish, end the open command panel, or with cancel a sketch being edited (its changes taken back). The result carries the panel's validation, preview and warnings. Commands that open a file dialog or another modal dialog are refused.",
            json!({
                "document": document,
                "id": {"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "A command id of the registry"},
                "edit": {"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "A feature uid to edit instead of running a command"},
                "inputs": {"type": "object", "description": "Input values by the panel's input keys: text for values and texts, a choice's value, true or false for checks, an array of entities (as ui_select's items) for selections"},
                "finish": {"type": "string", "enum": ["ok", "cancel", "keep_open"], "default": "ok", "description": "What ends a command panel"}
            }),
            &["document"],
            false,
            true,
        ),
        tool(
            "ui_camera",
            "Read or set the 3D view's camera, for inspection screenshots; the view changes at once (no animation) and the result is the camera as it then is. Without arguments it only reads. Given values replace the current camera's: eye and target (points), up, height (mm of the model the view shows at the target, top to bottom), fov (the perspective's vertical field of view in degrees), projection. Then box frames that box, or frame frames the shown bodies (only the isolated ones when the design isolates some) or the selection, seen along the resulting view direction with a small margin, in the part of the view no panel covers.",
            json!({
                "eye": point,
                "target": point,
                "up": point,
                "height": {"type": "number", "minimum": 0.000001, "description": "Millimetres of the model the view shows at the target, top to bottom"},
                "fov": {"type": "number", "minimum": 1, "maximum": 170, "description": "Vertical field of view of the perspective camera, degrees"},
                "projection": {"type": "string", "enum": ["orthographic", "perspective"]},
                "box": {
                    "type": "object",
                    "description": "A box in model coordinates to frame",
                    "properties": {"min": point, "max": point},
                    "required": ["min", "max"],
                    "additionalProperties": false
                },
                "frame": {"type": "string", "enum": ["none", "shown", "selection"], "default": "none", "description": "shown: the shown bodies (the isolated ones when some are isolated); selection: the selected entities"},
                "aspect": {"type": "number", "minimum": 0.1, "maximum": 10, "description": "Frame box or frame for a picture of this width / height (ui_screenshot's aspect, or its width and height) instead of the view's own shape"}
            }),
            &[],
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
    let of_type = |name: &str| match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        _ => true,
    };
    // One type, or a list of them (mitcad#689: a command or an array).
    let valid_type = match schema.get("type") {
        Some(Value::String(name)) => of_type(name.as_str()),
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).any(of_type),
        _ => true,
    };
    if !valid_type {
        return Err(format!("{path} must be {}", schema["type"]));
    }
    if let Some(number) = value.as_f64() {
        if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
            && number < minimum
        {
            return Err(format!("{path} must be at least {minimum}"));
        }
        if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64)
            && number > maximum
        {
            return Err(format!("{path} must be at most {maximum}"));
        }
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
    // A string's length in characters (mitcad#935): counted only up to the
    // limit, a megabyte string is refused without being walked.
    if let Some(string) = value.as_str()
        && let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64)
        && string
            .chars()
            .nth(usize::try_from(maximum).unwrap_or(usize::MAX))
            .is_some()
    {
        return Err(format!("{path} must have at most {maximum} characters"));
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
                return Err(format!("Unknown argument {path}.{}", echoed(key)));
            }
        }
    }
    if let Some(array) = value.as_array() {
        if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64)
            && (array.len() as u64) < minimum
        {
            return Err(format!("{path} must have at least {minimum} items"));
        }
        if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64)
            && (array.len() as u64) > maximum
        {
            return Err(format!("{path} must have at most {maximum} items"));
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
