// SPDX-License-Identifier: MIT
//! The JSON Schemas of the measurement and annotation commands and of the
//! image references' (mitcad#1918), written from their fields in
//! `core/model-api/src/measurements.rs`, `annotations.rs` and
//! `image_references.rs` (with `MeasurePick`, `AnnotationPlane`,
//! `AnnotationFormat`, `Tolerance` and `GeometricTolerance` of
//! `core/model/src/document/annotations.rs`) as [`super`] writes the
//! others. The drift test `core/model-tests/api/src/mcp_schema_tests.rs`
//! checks them against the API like the others.

use serde_json::{Value, json};

use super::{choice, flag, ids, number, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "add_measurement",
    "rename_measurement",
    "set_measurement_color",
    "set_measurement_visible",
    "set_measurement_kind",
    "delete_measurement",
    "add_annotation",
    "edit_annotation",
    "delete_annotation",
    "set_annotation_text_height",
    "calibrate_image_reference",
    "set_image_reference_visible",
    "rename_image_reference",
    "delete_image_reference",
];

/// An annotation's kinds (`MeasureKind`): the dimensions, then the leader
/// notes.
const KINDS: &[&str] = &[
    "distance",
    "angle",
    "offset",
    "axis_distance",
    "center_distance",
    "radius",
    "diameter",
    "note",
    "datum",
    "frame",
    "hole_callout",
    "material",
    "process_note",
];

/// The characteristics of a feature control frame (`Characteristic`).
const CHARACTERISTICS: &[&str] = &[
    "straightness",
    "flatness",
    "circularity",
    "cylindricity",
    "line_profile",
    "surface_profile",
    "parallelism",
    "perpendicularity",
    "angularity",
    "position",
    "concentricity",
    "symmetry",
    "circular_runout",
    "total_runout",
];

/// A pick as the `measure` query takes a selection given as fields
/// (`MeasurePick`).
fn pick(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}: a body (\"F2.b0\") with an optional face, edge or vertex by its name, or a datum (\"F5\", \"xy\"), with the occurrence that places it (\"O1/O4\"; left out in the root)"),
        "properties": {
            "body": {"type": "string"},
            "face": {"type": "string"},
            "edge": {"type": "string"},
            "vertex": {"type": "string"},
            "datum": {"type": "string"},
            "occurrence": {"type": "string"}
        },
        "additionalProperties": false
    })
}

fn kind(description: &str) -> Value {
    choice(KINDS, description)
}

fn name() -> Value {
    json!({"type": "string", "minLength": 1, "description": "The annotation (measurement) by its name (Distance1)"})
}

fn image() -> Value {
    json!({"type": "string", "minLength": 1, "description": "The image reference by its name (Image1)"})
}

fn color() -> Value {
    text("#rrggbb, or auto (the automatic colour)")
}

/// A point or a direction in the design's (or the pick's) coordinates.
fn vec3(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": description})
}

fn plane() -> Value {
    json!({
        "type": "object",
        "description": "The plane it lies in: {\"normal\": [x, y, z]} (with \"x\", the direction a note's or a diameter's text runs along), directions in the first pick's own coordinates; or {\"pick\": <pick>}, a planar face or datum plane it lies parallel to. Left out: chosen from the picks",
        "properties": {
            "normal": vec3("A direction its plane is square to"),
            "x": vec3("The direction its text runs along"),
            "pick": pick("A planar face or a datum plane")
        },
        "additionalProperties": false
    })
}

fn line_offset() -> Value {
    number(
        "A length's dimension line from its ends, mm, signed; an angle's arc radius; left out: placed automatically",
    )
}

fn text_offset() -> Value {
    json!({"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"},
           "description": "[along, across], mm in its plane: the text from its default place (a note's from its arrow's point)"})
}

fn format(edit: bool) -> Value {
    let description = if edit {
        "A dimension's format; only the fields given change, null sets one back"
    } else {
        "A dimension's format"
    };
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "decimals": {"type": ["integer", "null"], "minimum": 0, "maximum": 6, "description": "Decimals shown; left out: up to three without trailing zeros"},
            "prefix": {"type": ["string", "null"], "description": "In place of the kind's (Ø, R); \"\" for none"},
            "before": text("Own text before the value (4 x)"),
            "after": text("Own text after the value (THRU)"),
            "replace": {"type": ["string", "null"], "description": "A text in place of the value and its tolerance"}
        },
        "additionalProperties": false
    })
}

fn tolerance() -> Value {
    json!({
        "description": "A dimension's tolerance, mm for lengths and degrees for angles: {\"type\": \"symmetric\", \"value\"}, {\"type\": \"deviations\" or \"limits\", \"upper\", \"lower\"}, or {\"type\": \"fit\", \"code\" (ISO 286, H7, g6), \"deviations\"}",
        "type": "object",
        "properties": {
            "type": {"type": "string", "enum": ["symmetric", "deviations", "limits", "fit"]},
            "value": {"type": "number"},
            "upper": {"type": "number"},
            "lower": {"type": "number"},
            "code": {"type": "string"},
            "deviations": {"type": "boolean", "default": false}
        },
        "required": ["type"],
        "additionalProperties": false
    })
}

fn frame() -> Value {
    json!({
        "type": "object",
        "description": "A feature control frame's geometric tolerance (ISO 1101)",
        "properties": {
            "characteristic": {"type": "string", "enum": CHARACTERISTICS},
            "value": {"type": "number", "exclusiveMinimum": 0, "description": "The zone's width, mm"},
            "diameter": {"type": "boolean", "default": false, "description": "A cylindrical zone (Ø)"},
            "modifier": {"type": "string", "enum": ["M", "L"], "description": "A material condition (ISO 2692)"},
            "datums": {"type": "array", "maxItems": 3, "items": {"type": "string"}, "description": "The datums' letters, in order of precedence"}
        },
        "required": ["characteristic", "value"],
        "additionalProperties": false
    })
}

/// A measurement or annotation command's schema (fields without `cmd`).
fn annotation(command: &str) -> Option<Value> {
    let spec = match command {
        "add_measurement" => json!({
            "description": "Keeps a measurement (a dimension annotation) of one or two picks, measured at the timeline marker whenever asked. Result: name, kind.",
            "properties": {
                "a": pick("The first pick"),
                "b": pick("The second pick"),
                "kind": choice(&KINDS[..7], "What it measures; left out: what the picks give first"),
                "name": text("Its name; default the next free <Kind><n> (Distance1)"),
                "color": color()
            },
            "required": ["a"],
            "examples": [
                {"cmd": "add_measurement", "a": {"datum": "xy"}, "b": {"datum": "yz"}, "kind": "angle"},
                {"cmd": "add_measurement", "a": {"body": "F2.b0", "face": "F2:top"}, "b": {"body": "F2.b0", "face": "F2:bottom"}, "kind": "offset", "color": "#ff8000"}
            ]
        }),
        "rename_measurement" => json!({
            "description": "Renames a measurement.",
            "properties": {
                "name": name(),
                "new_name": json!({"type": "string", "minLength": 1, "description": "Its new name"})
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_measurement", "name": "Distance1", "new_name": "Gap"}]
        }),
        "set_measurement_color" => json!({
            "description": "Sets a measurement's colour.",
            "properties": {"name": name(), "color": color()},
            "required": ["name", "color"],
            "examples": [{"cmd": "set_measurement_color", "name": "Distance1", "color": "#ff8000"}]
        }),
        "set_measurement_visible" => json!({
            "description": "Shows or hides a measurement (its light bulb).",
            "properties": {"name": name(), "visible": {"type": "boolean"}},
            "required": ["name", "visible"],
            "examples": [{"cmd": "set_measurement_visible", "name": "Distance1", "visible": false}]
        }),
        "set_measurement_kind" => json!({
            "description": "Changes what a measurement shows of its picks.",
            "properties": {"name": name(), "kind": kind("What it shows; a dimension stays a dimension, a note a note")},
            "required": ["name", "kind"],
            "examples": [{"cmd": "set_measurement_kind", "name": "Distance1", "kind": "offset"}]
        }),
        "delete_measurement" => json!({
            "description": "Deletes a measurement.",
            "properties": {"name": name()},
            "required": ["name"],
            "examples": [{"cmd": "delete_measurement", "name": "Distance1"}]
        }),
        "add_annotation" => json!({
            "description": "Adds an annotation: a dimension of one or two picks or a leader note (a note, datum label, feature control frame, hole callout, material or process note) on one, ISO 129 style in mm. Result: name, kind; with preview the annotation it would add, nothing stored.",
            "properties": {
                "a": pick("The first pick"),
                "b": pick("The second pick"),
                "kind": kind("What it is; left out: what the picks give first, note when a text is given"),
                "name": text("Its name; default the next free <Kind><n>"),
                "color": color(),
                "group": text("A group's name (Manufacturing)"),
                "visible": {"type": "boolean", "default": true, "description": "Shown"},
                "plane": plane(),
                "line_offset": line_offset(),
                "text_offset": text_offset(),
                "format": format(false),
                "tolerance": tolerance(),
                "text": text("A note's text, lines split at \\n"),
                "at": vec3("A note's, datum's or frame's point as picked, in the pick's own coordinates"),
                "view": {
                    "type": "object",
                    "description": "The view it is added in, which suggests its plane and side: direction (the way the view looks), up, pixel (taken, not used)",
                    "properties": {"direction": vec3("The way the view looks, from the eye"), "up": vec3("The view's up"), "pixel": {"type": "number"}},
                    "required": ["direction"],
                    "additionalProperties": false
                },
                "letter": text("A datum label's letter; default the next free one"),
                "frame": frame(),
                "on": text("A frame: the dimension (its name) it stands under instead of a leader"),
                "preview": flag(false, "Only what it would add, nothing stored")
            },
            "required": ["a"],
            "examples": [
                {"cmd": "add_annotation", "a": {"datum": "xy"}, "kind": "note", "text": "Bead blast", "at": [0, 0, 0]},
                {"cmd": "add_annotation", "a": {"body": "F2.b0", "face": "F2:top"}, "b": {"body": "F2.b0", "face": "F2:bottom"}, "kind": "offset",
                 "tolerance": {"type": "symmetric", "value": 0.1}, "format": {"decimals": 1}}
            ]
        }),
        "edit_annotation" => json!({
            "description": "Changes one annotation (name) or several (names); null sets a field back to its default. Result: changed.",
            "properties": {
                "name": name(),
                "names": ids("Several annotations by their names"),
                "new_name": text("Its new name"),
                "kind": kind("What it is"),
                "color": color(),
                "group": {"type": ["string", "null"], "description": "A group's name; null: none"},
                "visible": {"type": "boolean", "description": "Shown"},
                "a": pick("The first pick"),
                "b": {"description": "The second pick; null: none", "anyOf": [pick("The second pick"), {"type": "null"}]},
                "plane": {"description": "Its plane; null: chosen from the picks", "anyOf": [plane(), {"type": "null"}]},
                "line_offset": {"type": ["number", "null"], "description": "The dimension line's offset or an angle's arc radius, mm; null: automatic"},
                "text_offset": {"description": "[along, across], mm; null: the default place", "anyOf": [text_offset(), {"type": "null"}]},
                "format": format(true),
                "tolerance": {"description": "Its tolerance; null: none", "anyOf": [tolerance(), {"type": "null"}]},
                "text": text("A note's text"),
                "at": {"description": "A note's point; null: the default", "anyOf": [vec3("The point"), {"type": "null"}]},
                "letter": text("A datum label's letter"),
                "frame": frame(),
                "on": {"type": ["string", "null"], "description": "A frame's dimension it stands under; null: a leader again"}
            },
            "required": [],
            "examples": [
                {"cmd": "edit_annotation", "name": "Distance1", "new_name": "Gap", "tolerance": null},
                {"cmd": "edit_annotation", "names": ["Distance1", "Angle1"], "group": "Manufacturing"}
            ]
        }),
        "delete_annotation" => json!({
            "description": "Deletes an annotation (name) or several (names).",
            "properties": {
                "name": name(),
                "names": ids("Several annotations by their names")
            },
            "required": [],
            "examples": [{"cmd": "delete_annotation", "name": "Distance1"}, {"cmd": "delete_annotation", "names": ["Distance1", "Note1"]}]
        }),
        "set_annotation_text_height" => json!({
            "description": "Sets the height of the annotations' texts in the design.",
            "properties": {
                "text_height": {"type": ["number", "null"], "minimum": 0.1, "maximum": 1000, "description": "mm, or null (left out) for the automatic height by the size of what is annotated"}
            },
            "required": [],
            "examples": [{"cmd": "set_annotation_text_height", "text_height": 3.5}, {"cmd": "set_annotation_text_height", "text_height": null}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// An image reference command's schema (fields without `cmd`).
fn image_reference(command: &str) -> Option<Value> {
    let spec = match command {
        "calibrate_image_reference" => json!({
            "description": "Scales an image reference so that two points or two pixels lie a distance apart. Result: scale (a pixel's new size, mm).",
            "properties": {
                "name": image(),
                "points": {"type": "array", "minItems": 2, "maxItems": 2, "items": vec3("A design point, projected onto the image's plane"),
                           "description": "Instead of pixels: two design points"},
                "pixels": {"type": "array", "minItems": 2, "maxItems": 2,
                           "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}},
                           "description": "Instead of points: two pixels [u, v]"},
                "distance": {"type": "number", "exclusiveMinimum": 0, "description": "Their distance, mm"}
            },
            "required": ["name", "distance"],
            "examples": [{"cmd": "calibrate_image_reference", "name": "Image1", "pixels": [[112, 640], [1530, 640]], "distance": 450}]
        }),
        "set_image_reference_visible" => json!({
            "description": "Shows or hides an image reference.",
            "properties": {"name": image(), "visible": {"type": "boolean"}},
            "required": ["name", "visible"],
            "examples": [{"cmd": "set_image_reference_visible", "name": "Image1", "visible": false}]
        }),
        "rename_image_reference" => json!({
            "description": "Renames an image reference.",
            "properties": {
                "name": image(),
                "new_name": json!({"type": "string", "minLength": 1, "description": "Its new name"})
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_image_reference", "name": "Image1", "new_name": "Front photo"}]
        }),
        "delete_image_reference" => json!({
            "description": "Deletes an image reference.",
            "properties": {"name": image()},
            "required": ["name"],
            "examples": [{"cmd": "delete_image_reference", "name": "Image1"}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A command's schema of this family as [`super`] writes its own.
pub(super) fn spec(name: &str) -> Option<Value> {
    annotation(name).or_else(|| image_reference(name))
}
