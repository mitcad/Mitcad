// SPDX-License-Identifier: MIT
//! The JSON Schemas of the design's data commands (mitcad#1918): comments
//! (`core/model-api/src/comments.rs`), typed property sets
//! (`properties.rs`), tables and their CSV (`tables.rs`, `tables_csv.rs`),
//! the parameters CSV (`parameters_csv.rs`), the configuration table and
//! its rows' CSV (`libraries.rs`, `configurations.rs`,
//! `configuration_rows_csv.rs`), the design's summary
//! (`design_properties.rs`) and Auto Fix (`auto_fix.rs`), written from
//! their fields as [`super`] writes the others. The drift test
//! `core/model-tests/api/src/mcp_schema_tests.rs` checks them against the
//! API like the others.

use serde_json::{Value, json};

use super::{choice, feature_uid, id, ids, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "add_comment",
    "edit_comment",
    "reply_comment",
    "retarget_comment",
    "resolve_comment",
    "reopen_comment",
    "delete_comment",
    "set_property",
    "remove_property",
    "define_property_type",
    "edit_property_type",
    "remove_property_type",
    "set_table",
    "rename_table",
    "rename_table_column",
    "delete_table",
    "import_table_csv",
    "import_parameters_csv",
    "import_configuration_rows_csv",
    "set_design_summary",
    "auto_fix",
    "set_configurations",
    "apply_configuration",
    "configure",
    "generate_configuration_rows",
];

/// The standard property types (`BaseType`).
const PROPERTY_TYPES: &str = "text, integer, number, length, area, volume, mass, angle, boolean (yes/no) or date, or an own type's name";

/// A comment's id.
fn comment() -> Value {
    json!({"type": "string", "minLength": 1, "description": "The comment (C3)"})
}

/// Who wrote it: the MCP layer sets its client's name.
fn author() -> Value {
    json!({"type": "string", "description": "Set by MCP to the client's name; a value given is replaced"})
}

/// What a comment is pinned to (`CommentTarget`).
fn comment_target() -> Value {
    json!({
        "type": "object",
        "description": "What the comment is pinned to: {\"feature\"}, {\"body\"} with face, edge or vertex, {\"sketch\"} with curve, point, constraint or dimension, {\"parameter\"}, or {\"at\": [x, y, z]} (mm in its component's coordinates) with component; occurrence places a body or a point's component",
        "properties": {
            "feature": {"type": "string", "description": "A feature (F3)"},
            "body": {"type": "string", "description": "A body (F2.b0)"},
            "face": {"type": "string", "description": "A face of the body, by its name"},
            "edge": {"type": "string", "description": "An edge of the body, by its name"},
            "vertex": {"type": "string", "description": "A vertex of the body, by its name"},
            "sketch": {"type": "string", "description": "A sketch (its uid), with one of curve, point, constraint and dimension"},
            "curve": {"type": "string", "description": "A sketch's curve (c<n>)"},
            "point": {"type": "string", "description": "A sketch's point (p<n>)"},
            "constraint": {"type": "string", "description": "A sketch's constraint (k<n>)"},
            "dimension": {"type": "string", "description": "A sketch's dimension (k<n>)"},
            "parameter": {"type": "string", "description": "A parameter, by its name"},
            "at": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": "A free point, mm"},
            "component": {"type": ["string", "null"], "description": "The component a free point is in; left out, \"\" or null: the root"},
            "occurrence": {"type": "string", "description": "The occurrence that places the body or the point's component (O1/O4); \"\" the root's"}
        },
        "additionalProperties": false
    })
}

/// A property's owner.
fn owner() -> Value {
    json!({"type": "string", "minLength": 1, "description": "The owner by id or name: a component (C2, C0 the root), an occurrence (O3, Wall:2, O1/O4) or a body (F7.b0)"})
}

/// A value of an own property type's (a number, a text, a boolean).
fn property_value(description: &str) -> Value {
    json!({"type": ["number", "string", "boolean"], "description": format!("{description}: a JSON number of a quantity is canonical (mm, mm², mm³, kg, radians); text is read in the type's unit, else the design's, unless it names its own (\"1.2 m\", \"500 g\")")})
}

/// The configuration table (`Configurations`).
fn configurations() -> Value {
    json!({
        "type": ["object", "null"],
        "description": "The table; null or an empty table removes it (model_reference {\"section\": \"Configurations: options, rows and rules\"})",
        "properties": {
            "options": {"type": "array", "items": {"type": "string"}, "description": "The options: user parameters' names in choice order"},
            "closed": {"type": "boolean", "default": false, "description": "Only the rows' combinations are valid"},
            "parameters": {"type": "array", "items": {"type": "string"}, "description": "The other parameters a closed table's rows set"},
            "default": {"type": "string", "description": "The row a design is inserted in; the first when left out"},
            "rows": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Unique in the table (M5x16)"},
                    "values": {"type": "object", "additionalProperties": {"type": "string"}, "description": "Expressions by parameter: the options' constants ('M5', \"16 mm\") and a closed table's other parameters"},
                    "designation": {"type": "string", "description": "How a bill of materials names a part made in this row"},
                    "part_number": {"type": "string", "description": "The supplier's article number"},
                    "supplier_url": {"type": "string", "description": "The supplier's product page (https)"},
                    "select": {"type": "object", "description": "An old table's selector values; migrated into values"}
                },
                "required": ["name"],
                "additionalProperties": false
            }},
            "rules": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "enabled": {"type": "boolean", "default": true},
                    "comment": {"type": "string"},
                    "when": {"type": "string", "description": "A condition (Color = 'Black'); true when left out"},
                    "then": {"type": "array", "items": {"type": "object"}, "description": "Actions: an option's restriction (option with values, exclude, min, max, step), require (with message), forbid, or in_table with columns"},
                    "else": {"type": "array", "items": {"type": "object"}, "description": "Actions when the condition does not hold"}
                },
                "required": ["name"],
                "additionalProperties": false
            }},
            "selectors": {"type": "array", "items": {"type": "string"}, "description": "An old table's cascaded choices; migrated to options"}
        },
        "additionalProperties": false
    })
}

/// A data command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "add_comment" => json!({
            "description": "Pins a comment to the model. Result: id.",
            "properties": {
                "text": text("The comment"),
                "target": comment_target(),
                "author": author()
            },
            "required": ["text", "target"],
            "examples": [{"cmd": "add_comment", "text": "Make this wall 3 mm", "target": {"feature": "F2"}},
                         {"cmd": "add_comment", "text": "Check this size", "target": {"parameter": "w"}}]
        }),
        "edit_comment" => json!({
            "description": "Changes a comment's text. Result: changed.",
            "properties": {
                "id": comment(),
                "text": text("Its new text")
            },
            "required": ["id", "text"],
            "examples": [{"cmd": "edit_comment", "id": "C1", "text": "Make this wall 4 mm"}]
        }),
        "reply_comment" => json!({
            "description": "Replies to a comment; its status stays.",
            "properties": {
                "id": comment(),
                "text": text("The reply"),
                "author": author()
            },
            "required": ["id", "text"],
            "examples": [{"cmd": "reply_comment", "id": "C1", "text": "Done in Extrude2"}]
        }),
        "retarget_comment" => json!({
            "description": "Pins a comment to another target. Result: changed.",
            "properties": {
                "id": comment(),
                "target": comment_target()
            },
            "required": ["id", "target"],
            "examples": [{"cmd": "retarget_comment", "id": "C1", "target": {"feature": "F3"}}]
        }),
        "resolve_comment" => json!({
            "description": "Resolves a comment (from open or addressed).",
            "properties": {"id": comment()},
            "required": ["id"],
            "examples": [{"cmd": "resolve_comment", "id": "C1"}],
            "restrictions": ["Refused for an MCP client unless Preferences › AI Assistants › Agents may resolve comments is on; mark a comment done with address_comment instead."]
        }),
        "reopen_comment" => json!({
            "description": "Opens a comment again, with an optional reply saying why.",
            "properties": {
                "id": comment(),
                "text": text("A reply saying why"),
                "author": author()
            },
            "required": ["id"],
            "examples": [{"cmd": "reopen_comment", "id": "C1", "text": "The wall is still 2 mm"}]
        }),
        "delete_comment" => json!({
            "description": "Deletes a comment; its id is not given again.",
            "properties": {"id": comment()},
            "required": ["id"],
            "examples": [{"cmd": "delete_comment", "id": "C1"}]
        }),
        "set_property" => json!({
            "description": "Sets a typed property of a component, occurrence or body. Result: owner, set, key, type, value (as stored), edited.",
            "properties": {
                "owner": owner(),
                "set": text("The property set (Identity); case does not matter"),
                "key": text("The key (Mark), without /"),
                "value": property_value("The value; optional with a type that has a default, or for an existing property"),
                "type": text(&format!("Its type: {PROPERTY_TYPES}; left out: the property's, else what the value is")),
                "unit": text("A quantity's unit a number or bare text is in; a number's label"),
                "source": choice(&["user", "rvt", "ifc", "ipt", "f3d", "library", "3mf"], "Where the value came from; default user (never computed)"),
                "library": choice(&["base", "user", "project"], "The profile of Print Printer or Print Process as a library's item: copied in, or the design's copy reused"),
                "project": text("The project's folder, when the design is in one")
            },
            "required": ["owner", "set", "key"],
            "examples": [{"cmd": "set_property", "owner": "C0", "set": "Identity", "key": "Mark", "value": "W-12"},
                         {"cmd": "set_property", "owner": "C0", "set": "Identity", "key": "Width", "value": "1.2 m", "type": "length"}]
        }),
        "remove_property" => json!({
            "description": "Removes a stored property; an occurrence's component value shows again.",
            "properties": {
                "owner": owner(),
                "set": text("The property set"),
                "key": text("The key")
            },
            "required": ["owner", "set", "key"],
            "examples": [{"cmd": "remove_property", "owner": "C0", "set": "Identity", "key": "Mark"}]
        }),
        "define_property_type" => json!({
            "description": "Defines an own property type. Result: name.",
            "properties": {
                "name": text("Its name: not a standard type's"),
                "base": choice(&["text", "integer", "number", "length", "area", "volume", "mass", "angle", "boolean", "date"], "The standard type it is based on"),
                "values": {"type": "array", "items": {"type": ["number", "string", "boolean"]}, "description": "The allowed values; a property of the type is then a choice"},
                "unit": text("A quantity's shown unit (m, deg, g); a number's label"),
                "default": property_value("Its default value"),
                "description": {"type": "string", "default": "", "description": "Its description"}
            },
            "required": ["name", "base"],
            "examples": [{"cmd": "define_property_type", "name": "Finish", "base": "text", "values": ["matt", "gloss"], "default": "matt"}]
        }),
        "edit_property_type" => json!({
            "description": "Changes an own property type. Result: name.",
            "properties": {
                "name": text("The type"),
                "new_name": text("Its new name; its properties follow"),
                "base": choice(&["text", "integer", "number", "length", "area", "volume", "mass", "angle", "boolean", "date"], "Its base type, only while no property uses it"),
                "values": {"type": "array", "items": {"type": ["number", "string", "boolean"]}, "description": "The allowed values; each property's value must stay allowed"},
                "unit": {"type": ["string", "null"], "description": "Its unit; null removes it"},
                "default": {"type": ["number", "string", "boolean", "null"], "description": "Its default; null removes it"},
                "description": text("Its description")
            },
            "required": ["name"],
            "examples": [{"cmd": "edit_property_type", "name": "Finish", "values": ["matt", "gloss", "satin"], "default": null}]
        }),
        "remove_property_type" => json!({
            "description": "Removes an own property type. Result: converted (how many properties keep their values under its base type).",
            "properties": {"name": text("The type")},
            "required": ["name"],
            "examples": [{"cmd": "remove_property_type", "name": "Finish"}]
        }),
        "set_table" => json!({
            "description": "Creates or replaces a table that expressions look values up in (Table.Column, XLOOKUP). Result: changed (the parameters whose values changed), moves.",
            "properties": {
                "name": text("The table's name, by the parameter name rules"),
                "table": {
                    "type": "object",
                    "description": "Its content",
                    "properties": {
                        "comment": {"type": "string", "default": "", "description": "A comment"},
                        "columns": {"type": "array", "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "unit": {"type": "string", "description": "Any unit a parameter takes (mm, in, deg, mm^2), \"\" for unitless, or Text"}
                            },
                            "required": ["name", "unit"],
                            "additionalProperties": false
                        }},
                        "rows": {"type": "array", "items": {"type": "array", "items": {"type": ["number", "string", "null"]}}, "description": "A cell per column: a number in the column's unit, a text, or null"}
                    },
                    "additionalProperties": false
                }
            },
            "required": ["name", "table"],
            "examples": [{"cmd": "set_table", "name": "Screws", "table": {
                "columns": [{"name": "Size", "unit": "Text"}, {"name": "dk", "unit": "mm"}],
                "rows": [["M5", 8.5], ["M6", 10]]}}]
        }),
        "rename_table" => json!({
            "description": "Renames a table; the expressions that use it follow.",
            "properties": {
                "name": text("The table"),
                "new_name": text("Its new name")
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_table", "name": "Screws", "new_name": "Bolts"}]
        }),
        "rename_table_column" => json!({
            "description": "Renames a table's column; the expressions that use it follow.",
            "properties": {
                "table": text("The table"),
                "name": text("The column"),
                "new_name": text("Its new name")
            },
            "required": ["table", "name", "new_name"],
            "examples": [{"cmd": "rename_table_column", "table": "Screws", "name": "dk", "new_name": "head"}]
        }),
        "delete_table" => json!({
            "description": "Deletes a table; refused while a parameter or a configuration row uses it.",
            "properties": {"name": text("The table")},
            "required": ["name"],
            "examples": [{"cmd": "delete_table", "name": "Screws"}]
        }),
        "import_table_csv" => json!({
            "description": "Creates a table from CSV text, or replaces it whole. Result: what it did (as the table_csv_preview query), moves.",
            "properties": {
                "name": text("The table"),
                "csv": text("The CSV text: a header of column names with their units in brackets (Size [Text],d [mm]), then a row per line")
            },
            "required": ["name", "csv"],
            "examples": [{"cmd": "import_table_csv", "name": "Screws", "csv": "Size [Text],dk [mm]\r\nM5,8.5\r\nM6,10\r\n"}]
        }),
        "import_parameters_csv" => json!({
            "description": "Imports parameters from CSV text. Result: what it did (as the parameters_csv_preview query), moves.",
            "properties": {
                "csv": text("The CSV text: MitCad's table (name,kind,unit,expression,value,comment,favorite,min,max,step,strict; name and expression needed) or the .f3d application's parameter table")
            },
            "required": ["csv"],
            "examples": [{"cmd": "import_parameters_csv", "csv": "name,unit,expression\r\nw,mm,40 mm\r\n"}]
        }),
        "import_configuration_rows_csv" => json!({
            "description": "Imports the configuration table's rows from CSV text. Result: what it did (as the configuration_rows_csv_preview query), moves.",
            "properties": {
                "csv": text("The CSV text, read as a table CSV: the first column of values is the key option; Name, Designation, Part Number and Supplier URL fill the rows' fields")
            },
            "required": ["csv"],
            "examples": [{"cmd": "import_configuration_rows_csv", "csv": "size,d [mm],k [mm]\r\nM3,3,1.86\r\n"}]
        }),
        "set_design_summary" => json!({
            "description": "Sets the design's summary; a field left out keeps its value, an empty text removes it. Result: changed, summary.",
            "properties": {
                "title": text("Its title"),
                "subject": text("Its subject"),
                "author": text("Its author"),
                "keywords": text("Its keywords"),
                "comments": text("Its comments")
            },
            "required": [],
            "examples": [{"cmd": "set_design_summary", "title": "Bracket", "keywords": "steel, wall"}]
        }),
        "auto_fix" => json!({
            "description": "Picks again the references the features' warnings found again, and fits threads' faces. Result: warnings, fixed, changed, followed, left, fitted, choices, label, message.",
            "properties": {
                "features": {"type": "array", "items": feature_uid("A feature"), "description": "The features; every feature when left out or empty"},
                "shared": choice(&["parameter", "own"], "What a thread's fit does with a named parameter other places use too: change it (parameter) or give the dimension its own value (own); left out, such a fit is left and choices names it")
            },
            "required": [],
            "examples": [{"cmd": "auto_fix"}, {"cmd": "auto_fix", "features": ["F3"], "shared": "own"}]
        }),
        "set_configurations" => json!({
            "description": "Sets the design's configuration table. Result: rows (count), options, added, warnings.",
            "properties": {"configurations": configurations()},
            "required": [],
            "examples": [{"cmd": "set_configurations", "configurations": {"options": ["Size"], "rows": [
                            {"name": "M5", "values": {"Size": "'M5'"}}, {"name": "M6", "values": {"Size": "'M6'"}}]}},
                         {"cmd": "set_configurations", "configurations": null}]
        }),
        "apply_configuration" => json!({
            "description": "Applies a configuration row. Result: changed (parameters whose values changed), moves.",
            "properties": {"name": id("The row")},
            "required": ["name"],
            "examples": [{"cmd": "apply_configuration", "name": "M5"}]
        }),
        "configure" => json!({
            "description": "Chooses option values, with the rules' cascade. Result: changed, moves, row (or null), refused (null, or the rule that refused the change: nothing changed).",
            "properties": {
                "values": {"type": "object", "minProperties": 1, "additionalProperties": {"type": ["number", "string"]}, "description": "Values by option: a number (mm or radians), an expression, or a text parameter's text"}
            },
            "required": ["values"],
            "examples": [{"cmd": "configure", "values": {"Length": "800 mm"}}]
        }),
        "generate_configuration_rows" => json!({
            "description": "One configuration row per key of a table's column, setting an option to it. Result: rows, added, updated, created, moves.",
            "properties": {
                "table": text("The table"),
                "column": text("Its column of keys"),
                "option": text("The option (a user parameter) the rows set"),
                "columns": ids("Other columns of the table as the closed table's parameter columns")
            },
            "required": ["table", "column", "option"],
            "examples": [{"cmd": "generate_configuration_rows", "table": "Screws", "column": "Size", "option": "Size"}]
        }),
        _ => return None,
    };
    Some(spec)
}
