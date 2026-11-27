// SPDX-License-Identifier: MIT
//! The JSON Schemas of the measurement and annotation commands, of the
//! image references' (mitcad#1918), of the point clouds' (mitcad#2240) and
//! of the decals' that MCP permits (mitcad#2244, `decals.rs`), written from
//! their fields in
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
    // Point clouds (mitcad#2240, `core/model-api/src/point_clouds.rs`).
    "move_point_cloud",
    "place_point_cloud",
    "set_point_cloud_visible",
    "rename_point_cloud",
    "delete_point_cloud",
    // Decals (mitcad#2244): insert_decal and edit_decal read files, so MCP
    // does not permit them.
    "set_decal_visible",
    "rename_decal",
    "delete_decal",
    // The point cloud tools (mitcad#2516, `point_clouds.rs`).
    "align_point_cloud",
    "filter_point_cloud",
    "reconstruct_point_cloud",
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

fn decal() -> Value {
    json!({"type": "string", "minLength": 1, "description": "The decal by its name (Decal1)"})
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

fn cloud() -> Value {
    json!({"type": "string", "minLength": 1, "description": "The point cloud by its name (PointCloud1; point_clouds query)"})
}

/// A point cloud command's schema (mitcad#2240; fields without `cmd`).
fn point_cloud(command: &str) -> Option<Value> {
    let spec = match command {
        "move_point_cloud" => json!({
            "description": "Moves a point cloud in its component, after its placement: turned (rotate), then moved (translate).",
            "properties": {
                "name": cloud(),
                "translate": vec3("The move along x, y and z, mm"),
                "rotate": {
                    "type": "object",
                    "description": "A turn about an axis, counter-clockwise looking against it",
                    "properties": {
                        "origin": vec3("A point of the axis, mm; the component's origin when left out"),
                        "axis": vec3("The axis' direction"),
                        "angle": number("Degrees")
                    },
                    "required": ["axis", "angle"],
                    "additionalProperties": false
                }
            },
            "required": ["name"],
            "examples": [{"cmd": "move_point_cloud", "name": "PointCloud1", "translate": [0, 0, 5], "rotate": {"axis": [0, 0, 1], "angle": 90}}]
        }),
        "place_point_cloud" => json!({
            "description": "Puts a point cloud into another occurrence's component, its placement kept, and/or back where it came in (reset).",
            "properties": {
                "name": cloud(),
                "occurrence": text("The occurrence path from the root (O1/O4); \"\" for the root"),
                "reset": flag(false, "Back where it came in: no placement")
            },
            "required": ["name"],
            "examples": [{"cmd": "place_point_cloud", "name": "PointCloud1", "reset": true}]
        }),
        "set_point_cloud_visible" => json!({
            "description": "Shows or hides a point cloud.",
            "properties": {"name": cloud(), "visible": {"type": "boolean"}},
            "required": ["name", "visible"],
            "examples": [{"cmd": "set_point_cloud_visible", "name": "PointCloud1", "visible": false}]
        }),
        "rename_point_cloud" => json!({
            "description": "Renames a point cloud; its deviations follow it.",
            "properties": {
                "name": cloud(),
                "new_name": json!({"type": "string", "minLength": 1, "description": "Its new name"})
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_point_cloud", "name": "PointCloud1", "new_name": "Scan"}]
        }),
        "delete_point_cloud" => json!({
            "description": "Deletes a point cloud; a deviation of it then says it is gone.",
            "properties": {"name": cloud()},
            "required": ["name"],
            "examples": [{"cmd": "delete_point_cloud", "name": "PointCloud1"}]
        }),
        _ => return point_cloud_tool(command),
    };
    Some(spec)
}

/// A box in the design's coordinates (a region or a crop).
fn cloud_box(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "min": vec3("The lowest corner, mm"),
            "max": vec3("The highest corner, mm"),
            "inside": flag(true, "Keeps the points inside (false: those outside)")
        },
        "required": ["min", "max"],
        "additionalProperties": false
    })
}

/// The point cloud tools' schemas (mitcad#2516; fields without `cmd`).
fn point_cloud_tool(command: &str) -> Option<Value> {
    let spec = match command {
        "align_point_cloud" => json!({
            "description": "Moves a point cloud onto a body (its faces, or some of them), a mesh body or another cloud: from where it is, from picked point pairs or from its shape, refined by the best fit (iterative closest points); answers the motion and the remaining deviation. The point_cloud_alignment query takes the same fields and moves nothing.",
            "properties": {
                "name": cloud(),
                "body": text("The target body's uid (bodies query); or give target_cloud"),
                "faces": ids("Only these faces of the body are matched; all when left out"),
                "target_cloud": text("Another point cloud as the target, by its name"),
                "start": choice(&["placed", "shape", "pairs"], "Where the best fit starts: where the cloud is (default), its middle and main axes matched to the target's, or the pairs"),
                "pairs": {
                    "type": "array",
                    "description": "Picked point pairs, at least three: a point on the cloud and its place on the target, in the design's coordinates",
                    "items": {
                        "type": "object",
                        "properties": {
                            "cloud": vec3("The point on the cloud, mm"),
                            "target": vec3("Its place on the target, mm")
                        },
                        "required": ["cloud", "target"],
                        "additionalProperties": false
                    }
                },
                "best_fit": flag(true, "Refines the start by the best fit"),
                "region": cloud_box("Only the cloud's points in this box take part (inside is not read)"),
                "max_distance": number("Points farther than this from the target (mm) are left out; the mean distance and three standard deviations when left out"),
                "max_iterations": {"type": "integer", "minimum": 1, "maximum": 1000, "default": 200, "description": "The best fit's rounds at most"},
                "max_samples": {"type": "integer", "minimum": 3, "maximum": 1000000, "default": 20000, "description": "The cloud's points matched at most (every n-th of a larger cloud)"}
            },
            "required": ["name"],
            "examples": [{"cmd": "align_point_cloud", "name": "PointCloud1", "body": "F1.b0", "start": "shape"}]
        }),
        "filter_point_cloud" => json!({
            "description": "Makes a new point cloud of a cloud's points without its outliers and noise and cropped by a box or a lasso; the cloud it came from is kept, hidden.",
            "properties": {
                "name": cloud(),
                "new_name": text("The new cloud's name; '<name> Filtered' when left out"),
                "keep_shown": flag(false, "The cloud it came from stays shown"),
                "statistical": {
                    "type": "object",
                    "description": "Statistical outliers: points whose neighbors are farther than the cloud's mean by more than ratio standard deviations",
                    "properties": {
                        "neighbors": {"type": "integer", "minimum": 1, "maximum": 200, "default": 16},
                        "ratio": {"type": "number", "default": 2.0}
                    },
                    "additionalProperties": false
                },
                "radius": {
                    "type": "object",
                    "description": "Outliers by radius: points with fewer than neighbors others within radius mm",
                    "properties": {
                        "radius": number("mm"),
                        "neighbors": {"type": "integer", "minimum": 1, "maximum": 200, "default": 4}
                    },
                    "required": ["radius"],
                    "additionalProperties": false
                },
                "smooth": {
                    "type": "object",
                    "description": "Noise: each point moved onto the plane of its nearest points",
                    "properties": {
                        "neighbors": {"type": "integer", "minimum": 3, "maximum": 200, "default": 16},
                        "passes": {"type": "integer", "minimum": 1, "maximum": 20, "default": 1}
                    },
                    "additionalProperties": false
                },
                "crop_box": cloud_box("A crop by a box in the design's coordinates"),
                "lasso": {
                    "type": "object",
                    "description": "A crop by a lasso: a polygon on a plane facing the viewer, the points seen inside it along normal",
                    "properties": {
                        "normal": vec3("Toward the viewer"),
                        "points": {"type": "array", "minItems": 3, "items": vec3("A corner, mm")},
                        "inside": flag(true, "Keeps the points inside (false: those outside)")
                    },
                    "required": ["normal", "points"],
                    "additionalProperties": false
                }
            },
            "required": ["name"],
            "examples": [{"cmd": "filter_point_cloud", "name": "PointCloud1", "statistical": {"neighbors": 16, "ratio": 2.0}}]
        }),
        "reconstruct_point_cloud" => json!({
            "description": "Builds a closed mesh of a point cloud (normals estimated, a signed distance from their tangent planes meshed) as a new mesh body in the cloud's component.",
            "properties": {
                "name": cloud(),
                "detail": {"type": "integer", "minimum": 8, "maximum": 256, "default": 64, "description": "Cubes of the grid along the cloud's longest side"},
                "neighbors": {"type": "integer", "minimum": 3, "maximum": 100, "default": 12, "description": "The nearest points a normal is estimated of"}
            },
            "required": ["name"],
            "examples": [{"cmd": "reconstruct_point_cloud", "name": "PointCloud1", "detail": 64}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A decal command's schema (fields without `cmd`; mitcad#2244,
/// `core/model-api/src/decals.rs`).
fn decal_command(command: &str) -> Option<Value> {
    let spec = match command {
        "set_decal_visible" => json!({
            "description": "Shows or hides a decal.",
            "properties": {"name": decal(), "visible": {"type": "boolean"}},
            "required": ["name", "visible"],
            "examples": [{"cmd": "set_decal_visible", "name": "Decal1", "visible": false}]
        }),
        "rename_decal" => json!({
            "description": "Renames a decal.",
            "properties": {
                "name": decal(),
                "new_name": json!({"type": "string", "minLength": 1, "description": "Its new name"})
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_decal", "name": "Decal1", "new_name": "Warning label"}]
        }),
        "delete_decal" => json!({
            "description": "Deletes a decal.",
            "properties": {"name": decal()},
            "required": ["name"],
            "examples": [{"cmd": "delete_decal", "name": "Decal1"}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A command's schema of this family as [`super`] writes its own.
pub(super) fn spec(name: &str) -> Option<Value> {
    annotation(name)
        .or_else(|| image_reference(name))
        .or_else(|| point_cloud(name))
        .or_else(|| decal_command(name))
}
