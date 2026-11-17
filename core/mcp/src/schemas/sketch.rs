// SPDX-License-Identifier: MIT
//! The JSON Schemas of the sketch commands (mitcad#1912): every permitted
//! `sketch.*` command (planar sketches) and `sketch3d.*` command (3D
//! sketches), written from their fields in `core/model-api/src/sketch.rs`,
//! `sketch_plane.rs` and `sketch3d.rs` as [`super`] writes the others. The
//! drift test `core/model-tests/api/src/mcp_schema_tests.rs` checks them
//! against the API like the others.

use serde_json::{Value, json};

use super::{choice, feature_uid, flag, id, ids, number, object, text, value_input};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "sketch.create",
    "sketch.add_rectangle",
    "sketch.add_circle",
    "sketch.add_point",
    "sketch.add_line",
    "sketch.rectangle",
    "sketch.circle",
    "sketch.arc",
    "sketch.polygon",
    "sketch.slot",
    "sketch.ellipse",
    "sketch.spline",
    "sketch.add_text",
    "sketch.edit_text",
    "sketch.add_constraint",
    "sketch.add_dimension",
    "sketch.set_dimension",
    "sketch.set_driven",
    "sketch.set_signed",
    "sketch.set_dimension_text",
    "sketch.set_active",
    "sketch.redefine_plane",
    "sketch.remove",
    "sketch.set_construction",
    "sketch.set_centerline",
    "sketch.set_fixed",
    "sketch.drag",
    "sketch.move",
    "sketch.trim",
    "sketch.extend",
    "sketch.insert_spline_point",
    "sketch.fillet",
    "sketch.chamfer",
    "sketch.offset",
    "sketch.edit_offset",
    "sketch.mirror",
    "sketch.circular_pattern",
    "sketch.rectangular_pattern",
    "sketch.edit_pattern",
    "sketch.project",
    "sketch.add_code",
    "sketch.edit_code",
    "sketch.clean_up_projections",
    "sketch.clean_up_stale_projections",
    "sketch.project_again",
    "sketch.convert_projected",
    "sketch.add_equal_spacing",
    "sketch.edit_equal_spacing",
    "sketch.axis",
    "sketch.paste",
    "sketch.scale",
    "sketch3d.create",
    "sketch3d.add_point",
    "sketch3d.add_line",
    "sketch3d.add_arc",
    "sketch3d.add_circle",
    "sketch3d.add_spline",
    "sketch3d.add_constraint",
    "sketch3d.add_dimension",
    "sketch3d.set_dimension",
    "sketch3d.set_driven",
    "sketch3d.set_construction",
    "sketch3d.fix",
    "sketch3d.detach",
    "sketch3d.attach",
    "sketch3d.include",
    "sketch3d.replace_reference",
    "sketch3d.release_from_plane",
    "sketch3d.release_plane",
    "sketch3d.remove",
    "sketch3d.drag",
    "sketch3d.convert",
    "sketch3d.convert_to_2d",
    "sketch3d.fillet",
    "sketch3d.add_ellipse",
    "sketch3d.add_rectangle",
    "sketch3d.add_polygon",
    "sketch3d.add_text",
    "sketch3d.paste",
];

/// What a coordinate of a drawing tool's new point may be (mitcad#1897,
/// mitcad#1912).
const EXPRESSIONS: &str = "a coordinate is a number (mm) or a text, an expression of parameters (\"w\", \"w/2\", \"h + 3\"), kept as a dimension from the sketch's origin so that the point follows the parameters (model_reference {\"section\": \"Point coordinates as expressions\"})";

/// The planar sketch's constraint types (`ConstraintKind`).
const CONSTRAINTS: &[&str] = &[
    "coincident",
    "horizontal",
    "vertical",
    "horizontal_points",
    "vertical_points",
    "parallel",
    "perpendicular",
    "collinear",
    "tangent",
    "smooth",
    "equal",
    "concentric",
    "midpoint",
    "symmetric",
    "equal_spacing",
];

/// The planar sketch's dimension types (`DimensionKind`).
const DIMENSIONS: &[&str] = &[
    "distance",
    "horizontal_distance",
    "vertical_distance",
    "point_line_distance",
    "line_distance",
    "length",
    "tangent_distance",
    "concentric_distance",
    "angle",
    "radius",
    "diameter",
    "arc_length",
    "major_radius",
    "minor_radius",
    "linear_diameter",
    "spacing",
    "tangent_circle_distance",
    "tangent_point_distance",
];

/// The 3D sketch's dimension types (`SpatialDimensionKind`; `in_plane` is a
/// converted sketch's and refused here).
const SPATIAL_DIMENSIONS: &[&str] = &[
    "length",
    "offset",
    "radius",
    "diameter",
    "arc_length",
    "spline_length",
    "major_radius",
    "minor_radius",
    "angle",
    "tangent_magnitude",
    "offset_along",
];

fn sketch() -> Value {
    feature_uid("The sketch (a planar sketch's uid)")
}

fn sketch3d() -> Value {
    feature_uid("The 3D sketch (its uid)")
}

/// A planar point input whose new point's coordinates may be expressions.
fn point(description: &str) -> Value {
    json!({
        "description": format!("{description}: an existing point \"p<n>\", or [x, y] for a new point; {EXPRESSIONS}"),
        "oneOf": [
            {"type": "string", "pattern": "^p[0-9]+$"},
            {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": ["number", "string"]}}
        ]
    })
}

/// A planar point input read once: an existing point or numbers.
fn point_once(description: &str) -> Value {
    json!({
        "description": format!("{description}: an existing point \"p<n>\", or [x, y] in mm (numbers: it is read once and cannot follow parameters)"),
        "oneOf": [
            {"type": "string", "pattern": "^p[0-9]+$"},
            {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}}
        ]
    })
}

/// A planar position of numbers.
fn position(description: &str) -> Value {
    json!({"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"},
           "description": format!("{description}: [x, y] in mm, sketch coordinates (numbers only)")})
}

/// A 3D sketch's point input whose new point's coordinates may be
/// expressions.
fn point3(description: &str) -> Value {
    json!({
        "description": format!("{description}: an existing point \"p<n>\", or [x, y, z] in the sketch's frame for a new point; {EXPRESSIONS}"),
        "oneOf": [
            {"type": "string", "pattern": "^p[0-9]+$"},
            {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": ["number", "string"]}}
        ]
    })
}

/// A 3D position or vector of numbers.
fn position3(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"},
           "description": format!("{description}: [x, y, z] in the sketch's frame (numbers only)")})
}

/// A definition's item by its `type`, its fields in the reference.
fn typed(types: &[&str], description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}; model_reference {{\"feature\": \"sketch\"}} gives each type's fields"),
        "properties": {"type": {"type": "string", "enum": types}},
        "required": ["type"]
    })
}

fn constraint() -> Value {
    typed(
        CONSTRAINTS,
        "A constraint as in the sketch definition, without id",
    )
}

fn dimension() -> Value {
    typed(
        DIMENSIONS,
        "What the dimension measures, as in the sketch definition, without id, value and driven",
    )
}

/// A frame axis of a 3D sketch.
fn axis3(description: &str) -> Value {
    choice(&["x", "y", "z"], description)
}

fn sense(description: &str) -> Value {
    json!({"type": "string", "enum": ["positive", "negative"], "default": "positive", "description": description})
}

/// A plane-bound shape's plane in a 3D sketch.
fn plane3() -> Value {
    json!({
        "description": "The plane the shape lies on, through its first point given: a frame plane (xy, yz or zx), or a rotated plane {\"x\": [x, y, z], \"normal\": [x, y, z]}",
        "oneOf": [
            {"type": "string", "enum": ["xy", "yz", "zx"]},
            {"type": "object", "properties": {"x": {"type": "array"}, "normal": {"type": "array"}}, "required": ["x", "normal"]}
        ]
    })
}

/// A source of a 3D sketch's linked reference.
fn source(description: &str) -> Value {
    json!({
        "description": format!("{description}: \"origin\", or an object that names its kind by its keys ({{\"body\", \"vertex\"}}, {{\"body\", \"edge\", \"at\"}}, {{\"body\", \"face\"}}, {{\"sketch\", \"point\"}}, {{\"sketch\", \"curve\", \"at\"}}, {{\"feature\"}}), with \"occurrence\" (and \"context\") for another component's geometry"),
        "type": ["string", "object"]
    })
}

/// A planar sketch command's schema (fields without `cmd`).
fn planar(name: &str) -> Option<Value> {
    let spec = match name {
        "sketch.create" => json!({
            "description": "Adds a planar sketch at the timeline marker. Result: uid, name.",
            "properties": {
                "plane": {"description": "Its plane: xy (the default), xz, yz, a construction plane \"F5\", a planar face {\"face\", \"body\"}, or a curved face {\"face\", \"body\", \"developed\": {\"angle\"}}", "type": ["string", "object"]},
                "frame": object("Its own frame on the plane"),
                "name": text("Its name; default the next Sketch<n>"),
                "occurrence": text("Where the plane was picked: the occurrence path of another component's plane"),
                "context": text("Where the active component is seen (its first occurrence path when left out)"),
                "upright": flag(false, "Without frame: the frame that shows the design upright")
            },
            "required": [],
            "examples": [{"cmd": "sketch.create"}, {"cmd": "sketch.create", "plane": "xz", "name": "Profile"}]
        }),
        "sketch.add_rectangle" => json!({
            "description": "A dimensioned rectangle: a fixed first corner, horizontal and vertical sides, width and height dimensions.",
            "properties": {
                "sketch": sketch(),
                "corner": position("The first corner, a new fixed point"),
                "width": value_input("The width"),
                "height": value_input("The height")
            },
            "required": ["sketch", "corner", "width", "height"],
            "examples": [{"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": "w", "height": 20}]
        }),
        "sketch.add_circle" => json!({
            "description": "A circle with a fixed centre and a diameter dimension.",
            "properties": {
                "sketch": sketch(),
                "center": position("The centre, a new fixed point"),
                "diameter": value_input("The diameter")
            },
            "required": ["sketch", "center", "diameter"],
            "examples": [{"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10}]
        }),
        "sketch.add_point" => json!({
            "description": "A new point.",
            "properties": {
                "sketch": sketch(),
                "at": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": ["number", "string"]}, "description": format!("Where: [x, y]; {EXPRESSIONS}")},
                "fixed": flag(false, "Fixed where it is")
            },
            "required": ["sketch", "at"],
            "examples": [{"cmd": "sketch.add_point", "sketch": "F1", "at": ["w/2", 5]}]
        }),
        "sketch.add_line" => json!({
            "description": "A line. Result: made (the line), inputs (the point at each point input).",
            "properties": {
                "sketch": sketch(),
                "mode": choice(&["two_point", "midpoint"], "two_point (the default: start, end) or midpoint (center, end: through center to end and as far on the other side)"),
                "start": point("Its start (two_point)"),
                "center": point("Its middle (midpoint)"),
                "end": point("Its end"),
                "construction": flag(false, "A construction line"),
                "centerline": flag(false, "A centre line")
            },
            "required": ["sketch", "end"],
            "examples": [{"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": ["w", 0]},
                         {"cmd": "sketch.add_line", "sketch": "F1", "start": "p3", "end": [10, 20], "construction": true}]
        }),
        "sketch.rectangle" => json!({
            "description": "Four joined lines, counter-clockwise; with radius four arcs too. Result: made (the lines, then the arcs).",
            "properties": {
                "sketch": sketch(),
                "mode": choice(&["two_point", "three_point", "center"], "two_point (a, b: opposite corners), three_point (side a-b, c on the opposite side) or center (center, corner)"),
                "a": point("A corner"),
                "b": point("A corner"),
                "c": position("three_point: a place on the opposite side"),
                "center": point("center: the centre"),
                "corner": point("center: a corner"),
                "construction": flag(false, "Construction lines"),
                "radius": value_input("Rounded corners of this radius")
            },
            "required": ["sketch", "mode"],
            "examples": [{"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": [0, 0], "b": ["w", "h"]},
                         {"cmd": "sketch.rectangle", "sketch": "F1", "mode": "center", "center": "p1", "corner": [20, 10], "radius": 2}]
        }),
        "sketch.circle" => json!({
            "description": "A circle.",
            "properties": {
                "sketch": sketch(),
                "mode": choice(&["center", "two_point", "three_point", "two_tangent", "three_tangent"], "center (center, radius), two_point (a, b: a diameter), three_point (a, b, c), two_tangent (lines [2], radius, near), three_tangent (lines [3])"),
                "center": point("center: the centre"),
                "radius": number("The radius, mm (a number: dimension it afterwards)"),
                "a": point("A point it passes; an existing point is held on it"),
                "b": point("A point it passes"),
                "c": point("A point it passes"),
                "lines": ids("The lines it touches (c<n>)"),
                "near": position("two_tangent: which of the solutions"),
                "construction": flag(false, "A construction circle")
            },
            "required": ["sketch", "mode"],
            "examples": [{"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": ["w/2", 0], "radius": 5}]
        }),
        "sketch.arc" => json!({
            "description": "An arc, counter-clockwise unless clockwise.",
            "properties": {
                "sketch": sketch(),
                "mode": choice(&["three_point", "center", "tangent", "normal"], "three_point (start, through, end), center (center, start, end, clockwise), tangent (from, end) or normal (from, end, toward)"),
                "start": point("Its start"),
                "through": point("three_point: a point it passes; a position makes no point (numbers only), an existing point is held on it"),
                "end": point("Its end; center mode: the direction of the end, a position [x, y] of numbers"),
                "center": point("center: its centre"),
                "clockwise": flag(false, "center: clockwise"),
                "from": id("tangent, normal: the end point of a curve it continues (p<n>)"),
                "toward": position("normal: the side the arc leaves to"),
                "construction": flag(false, "A construction arc")
            },
            "required": ["sketch", "mode"],
            "examples": [{"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point", "start": [0, 0], "through": [5, 5], "end": [10, 0]}]
        }),
        "sketch.polygon" => json!({
            "description": "A regular polygon of joined lines on a construction circle.",
            "properties": {
                "sketch": sketch(),
                "center": point("Its centre"),
                "vertex": point("Its first corner (inscribed), or a side's middle (circumscribed: a position of numbers)"),
                "sides": {"type": "integer", "minimum": 3, "maximum": 256, "description": "Its number of sides"},
                "inscribed": flag(true, "Inscribed (vertex is a corner) or circumscribed"),
                "construction": flag(false, "Construction lines")
            },
            "required": ["sketch", "center", "vertex", "sides"],
            "examples": [{"cmd": "sketch.polygon", "sketch": "F1", "center": [0, 0], "vertex": [10, 0], "sides": 6}]
        }),
        "sketch.slot" => json!({
            "description": "A slot: two end arcs, two tangent sides and a construction centre line or arc.",
            "properties": {
                "sketch": sketch(),
                "mode": choice(&["center_to_center", "center_point", "arc"], "center_to_center (a, b), center_point (center, end) or arc (center, start, end)"),
                "width": number("Its width, mm"),
                "a": point("An end's centre"),
                "b": point("An end's centre"),
                "center": point("The middle, or the arc slot's centre"),
                "start": point("arc: the first end's centre"),
                "end": point("center_point: an end's centre; arc: the direction of the far end, a position of numbers")
            },
            "required": ["sketch", "mode", "width"],
            "examples": [{"cmd": "sketch.slot", "sketch": "F1", "mode": "center_to_center", "width": 6, "a": [0, 0], "b": [30, 0]}]
        }),
        "sketch.ellipse" => json!({
            "description": "An ellipse, or with start and end an elliptical arc.",
            "properties": {
                "sketch": sketch(),
                "center": point("Its centre"),
                "major": point("The end of its major axis"),
                "minor_radius": number("Its minor radius, mm"),
                "start": point("An arc's start"),
                "end": point("An arc's end"),
                "construction": flag(false, "A construction ellipse")
            },
            "required": ["sketch", "center", "major", "minor_radius"],
            "examples": [{"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 0], "major": [20, 0], "minor_radius": 10}]
        }),
        "sketch.spline" => json!({
            "description": "A spline through its points, or with degree by its control points.",
            "properties": {
                "sketch": sketch(),
                "points": {"type": "array", "minItems": 2, "items": point("A point"), "description": "Its points in order"},
                "degree": {"type": "integer", "minimum": 1, "description": "A B-spline by these control points; left out: a cubic through them"},
                "closed": flag(false, "Fit points only: closed and smooth through the first point"),
                "construction": flag(false, "A construction spline")
            },
            "required": ["sketch", "points"],
            "examples": [{"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 0], [10, 5], ["w", 0]]}]
        }),
        "sketch.add_text" => json!({
            "description": "A text whose glyphs bound profiles.",
            "properties": {
                "sketch": sketch(),
                "text": text("The text; may be left out with value"),
                "at": position("Where it starts (a text has no point there)"),
                "height": number("Its height, mm"),
                "angle": number("Its angle, radians"),
                "font": text("An installed font's family"),
                "bold": flag(false, "Bold"),
                "italic": flag(false, "Italic"),
                "align": choice(&["left", "center", "right"], "Horizontal alignment"),
                "valign": choice(&["baseline", "bottom", "middle", "top"], "Vertical alignment"),
                "spacing": number("Extra letter spacing"),
                "frame": {"type": ["array", "object"], "description": "Instead of at: three point ids, or {\"corner\": [x, y], \"diagonal\": [x, y]}"},
                "path": object("Along a curve: {\"curve\", \"above\", \"fit\"}"),
                "flip_x": flag(false, "Mirrored across"),
                "flip_y": flag(false, "Mirrored up and down"),
                "cap_height": flag(false, "height is the capitals' height"),
                "wrap": flag(false, "Wrapped in its frame"),
                "line_spacing": number("Line spacing"),
                "pen_align": flag(false, "Aligned by the pen"),
                "ink_top": flag(false, "Top at the ink"),
                "no_kerning": flag(false, "Without kerning"),
                "glyphs": object("Stored outlines (as the sketch definition's texts)"),
                "font_file": text("A font file's path instead of font"),
                "value": value_input("A text parameter's name or a text expression the text follows")
            },
            "required": ["sketch", "height"],
            "examples": [{"cmd": "sketch.add_text", "sketch": "F1", "text": "MitCad", "at": [0, 0], "height": 5}]
        }),
        "sketch.edit_text" => json!({
            "description": "Changes a text; the fields left out stay.",
            "properties": {
                "sketch": sketch(),
                "id": id("The text (t<n>)"),
                "text": text("Its new text"),
                "at": position("Where it starts"),
                "height": number("Its height, mm"),
                "angle": number("Its angle, radians"),
                "font": text("An installed font's family"),
                "bold": {"type": "boolean"},
                "italic": {"type": "boolean"},
                "align": choice(&["left", "center", "right"], "Horizontal alignment"),
                "valign": choice(&["baseline", "bottom", "middle", "top"], "Vertical alignment"),
                "spacing": number("Extra letter spacing"),
                "path": object("Along a curve: {\"curve\", \"above\", \"fit\"}"),
                "flip_x": {"type": "boolean"},
                "flip_y": {"type": "boolean"},
                "cap_height": {"type": "boolean"},
                "wrap": {"type": "boolean"},
                "line_spacing": number("Line spacing"),
                "pen_align": {"type": "boolean"},
                "ink_top": {"type": "boolean"},
                "no_kerning": {"type": "boolean"},
                "font_file": text("A font file's path; \"\" leaves the file for font"),
                "value": value_input("A text parameter or expression to follow; \"\" stops it following one")
            },
            "required": ["sketch", "id"],
            "examples": [{"cmd": "sketch.edit_text", "sketch": "F1", "id": "t5", "text": "v2", "bold": true}]
        }),
        "sketch.add_constraint" => json!({
            "description": "Adds a constraint; a coincident between two curves' end points merges them into one point.",
            "properties": {
                "sketch": sketch(),
                "constraint": constraint(),
                "auto": flag(false, "Only when safe: refused when the others imply it, it conflicts or the solve moves anything")
            },
            "required": ["sketch", "constraint"],
            "examples": [{"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "horizontal", "line": "c3"}},
                         {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "coincident", "point": "p2", "entity": "p5"}}]
        }),
        "sketch.add_dimension" => json!({
            "description": "Adds a dimension, driving (with a parameter) or driven.",
            "properties": {
                "sketch": sketch(),
                "dimension": dimension(),
                "value": value_input("Its value; left out: what it measures now"),
                "driven": flag(false, "Only measures"),
                "text": position("Where its value is shown"),
                "scale_sketch": flag(false, "The first driving dimension scales the sketch to itself")
            },
            "required": ["sketch", "dimension"],
            "examples": [{"cmd": "sketch.add_dimension", "sketch": "F1", "dimension": {"type": "distance", "a": "p1", "b": "p2"}, "value": "w"}]
        }),
        "sketch.set_dimension" => json!({
            "description": "Changes a dimension's value; a driven one becomes driving.",
            "properties": {
                "sketch": sketch(),
                "dimension": id("The dimension (k<n>)"),
                "value": value_input("Its value: a number, a parameter's name, an expression, or name = expression")
            },
            "required": ["sketch", "dimension", "value"],
            "examples": [{"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": "k3", "value": "w / 2"}]
        }),
        "sketch.set_driven" => json!({
            "description": "Makes a dimension driven or driving.",
            "properties": {"sketch": sketch(), "dimension": id("The dimension (k<n>)"), "driven": flag(true, "Driven (true) or driving")},
            "required": ["sketch", "dimension"],
            "examples": [{"cmd": "sketch.set_driven", "sketch": "F1", "dimension": "k3"}]
        }),
        "sketch.set_signed" => json!({
            "description": "Makes a horizontal or vertical distance signed (b - a) or not; the point stays.",
            "properties": {"sketch": sketch(), "dimension": id("The dimension (k<n>)"), "signed": flag(true, "Signed")},
            "required": ["sketch", "dimension"],
            "examples": [{"cmd": "sketch.set_signed", "sketch": "F1", "dimension": "k3"}]
        }),
        "sketch.set_dimension_text" => json!({
            "description": "Where a dimension's value is shown.",
            "properties": {"sketch": sketch(), "dimension": id("The dimension (k<n>)"), "text": position("Where")},
            "required": ["sketch", "dimension", "text"],
            "examples": [{"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k3", "text": [10, 12]}]
        }),
        "sketch.set_active" => json!({
            "description": "Makes constraints and dimensions inactive (kept, left out of the solve) or active again.",
            "properties": {
                "sketch": sketch(),
                "items": ids("The constraints and dimensions (k<n>)"),
                "active": flag(true, "Active"),
                "reason": text("Why inactive (with active false)")
            },
            "required": ["sketch", "items"],
            "examples": [{"cmd": "sketch.set_active", "sketch": "F1", "items": ["k4"], "active": false}]
        }),
        "sketch.redefine_plane" => json!({
            "description": "Moves the sketch to another plane or face and places it there.",
            "properties": {
                "sketch": sketch(),
                "plane": {"description": "The new plane, as sketch.create takes it; the sketch's own when left out", "type": ["string", "object"]},
                "occurrence": text("As in sketch.create"),
                "context": text("As in sketch.create"),
                "offset": value_input("The offset from the plane; left out: kept, a plain zero: none"),
                "x": value_input("The place along x; left out: kept"),
                "y": value_input("The place along y; left out: kept"),
                "angle": value_input("The turn on the plane; left out: kept"),
                "flip": {"type": "boolean", "description": "Flipped"},
                "upright": flag(false, "On a new plane: the frame that shows the design upright")
            },
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.redefine_plane", "sketch": "F1", "plane": "xz", "offset": 5}]
        }),
        "sketch.remove" => json!({
            "description": "Removes points, curves, texts, constraints, dimensions, patterns and offsets, with what depends on them.",
            "properties": {"sketch": sketch(), "items": ids("p<n>, c<n>, t<n>, k<n>")},
            "required": ["sketch", "items"],
            "examples": [{"cmd": "sketch.remove", "sketch": "F1", "items": ["c4", "k6"]}]
        }),
        "sketch.set_construction" => json!({
            "description": "Makes curves construction curves or ordinary ones.",
            "properties": {"sketch": sketch(), "curves": ids("The curves (c<n>)"), "construction": flag(true, "Construction")},
            "required": ["sketch", "curves"],
            "examples": [{"cmd": "sketch.set_construction", "sketch": "F1", "curves": ["c3"]}]
        }),
        "sketch.set_centerline" => json!({
            "description": "Makes lines centre lines or not.",
            "properties": {"sketch": sketch(), "lines": ids("The lines (c<n>)"), "centerline": flag(true, "Centre lines")},
            "required": ["sketch", "lines"],
            "examples": [{"cmd": "sketch.set_centerline", "sketch": "F1", "lines": ["c3"]}]
        }),
        "sketch.set_fixed" => json!({
            "description": "Fixes points and curves where they are, or frees them.",
            "properties": {"sketch": sketch(), "entities": ids("The points and curves (p<n>, c<n>)"), "fixed": flag(true, "Fixed")},
            "required": ["sketch", "entities"],
            "examples": [{"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["p1"]}]
        }),
        "sketch.drag" => json!({
            "description": "Pulls geometry as far as the constraints allow.",
            "properties": {
                "sketch": sketch(),
                "entity": id("A point (with to) or a curve (with by or radius)"),
                "entities": ids("Instead of entity: a selection dragged together by by"),
                "to": position("A point's target"),
                "by": position("An offset [dx, dy]"),
                "radius": number("A circle's or arc's target radius, mm")
            },
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.drag", "sketch": "F1", "entity": "p3", "to": [12, 4]}]
        }),
        "sketch.move" => json!({
            "description": "Moves, rotates or copies geometry once; the constraints are kept. Result: made (the copies).",
            "properties": {
                "sketch": sketch(),
                "entities": ids("The points and curves (p<n>, c<n>); entities, texts or both"),
                "texts": ids("The texts and QR codes (t<n>) moved or copied with them; one in a frame moves its frame, one on a path is refused"),
                "by": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "An offset [dx, dy] in mm (numbers: a move is read once and cannot follow parameters)"},
                "from": point_once("Point to Point: from"),
                "to": point_once("Point to Point: to"),
                "rotate": {"type": "object", "description": "A rotation {\"center\": a point input of numbers or an existing point (left out: the entities' middle), \"angle\": radians}", "properties": {"center": point_once("The pivot"), "angle": number("Radians")}, "required": ["angle"]},
                "copy": flag(false, "Copies instead of moving"),
                "hold": {"type": "array", "items": constraint(), "description": "Constraints added after the move where it was dropped; a moved entity stands for its copy"}
            },
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.move", "sketch": "F1", "entities": ["c3", "c4"], "by": [10, 0]},
                         {"cmd": "sketch.move", "sketch": "F1", "entities": ["c3"], "rotate": {"angle": 0.5}, "copy": true}]
        }),
        "sketch.trim" => json!({
            "description": "Removes the piece of a curve around a place between the curves that cross it.",
            "properties": {"sketch": sketch(), "curve": id("The curve (c<n>)"), "at": position("The piece's place")},
            "required": ["sketch", "curve", "at"],
            "examples": [{"cmd": "sketch.trim", "sketch": "F1", "curve": "c3", "at": [5, 0]}]
        }),
        "sketch.extend" => json!({
            "description": "Extends the end of a line or arc nearest a place to the next curve. Result: joined (the point reached).",
            "properties": {"sketch": sketch(), "curve": id("The curve (c<n>)"), "at": position("Near the end to extend")},
            "required": ["sketch", "curve", "at"],
            "examples": [{"cmd": "sketch.extend", "sketch": "F1", "curve": "c3", "at": [9, 0]}]
        }),
        "sketch.insert_spline_point" => json!({
            "description": "A point on a spline where it is nearest a place. Result: made (the point).",
            "properties": {"sketch": sketch(), "curve": id("The spline (c<n>)"), "at": position("Near where")},
            "required": ["sketch", "curve", "at"],
            "examples": [{"cmd": "sketch.insert_spline_point", "sketch": "F1", "curve": "c3", "at": [5, 2]}]
        }),
        "sketch.fillet" => json!({
            "description": "A tangent arc with a radius dimension at a corner, or at several corners.",
            "properties": {
                "sketch": sketch(),
                "a": id("A curve of the corner (c<n>)"),
                "b": id("The other curve"),
                "curves": ids("Several corners: where two of these lines end together"),
                "points": ids("Several corners: these corner points (p<n>)"),
                "radius": value_input("The radius")
            },
            "required": ["sketch", "radius"],
            "examples": [{"cmd": "sketch.fillet", "sketch": "F1", "a": "c1", "b": "c2", "radius": 3}]
        }),
        "sketch.chamfer" => json!({
            "description": "A line across a corner with its distance (and distance or angle) dimensions.",
            "properties": {
                "sketch": sketch(),
                "a": id("A line of the corner (c<n>)"),
                "b": id("The other line"),
                "distance": value_input("The distance along a"),
                "distance2": value_input("The distance along b"),
                "angle": value_input("Instead of distance2: the angle")
            },
            "required": ["sketch", "a", "b", "distance"],
            "examples": [{"cmd": "sketch.chamfer", "sketch": "F1", "a": "c1", "b": "c2", "distance": 2}]
        }),
        "sketch.offset" => json!({
            "description": "Offsets a chain of joined curves, or a closed curve, to its right (a closed curve: positive is out).",
            "properties": {
                "sketch": sketch(),
                "curves": ids("The chain (c<n>)"),
                "distance": value_input("The distance"),
                "corners": json!({"type": "string", "enum": ["sharp", "round"], "default": "sharp", "description": "The outer corners"})
            },
            "required": ["sketch", "curves", "distance"],
            "examples": [{"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", "c2", "c3", "c4"], "distance": 2}]
        }),
        "sketch.edit_offset" => json!({
            "description": "Changes an offset's distance, side or corners.",
            "properties": {
                "sketch": sketch(),
                "offset": id("The offset (k<n>)"),
                "distance": value_input("Its distance"),
                "flip": flag(false, "To the other side"),
                "corners": choice(&["sharp", "round"], "Its corners; left out: as they are")
            },
            "required": ["sketch", "offset"],
            "examples": [{"cmd": "sketch.edit_offset", "sketch": "F1", "offset": "k8", "distance": 3}]
        }),
        "sketch.mirror" => json!({
            "description": "Mirrored copies with symmetric constraints.",
            "properties": {
                "sketch": sketch(),
                "entities": ids("The points and curves (p<n>, c<n>)"),
                "axis": id("A line (c<n>), or the sketch's own axis \"x\" or \"y\"")
            },
            "required": ["sketch", "entities", "axis"],
            "examples": [{"cmd": "sketch.mirror", "sketch": "F1", "entities": ["c1", "c2"], "axis": "y"}]
        }),
        "sketch.circular_pattern" => json!({
            "description": "Copies turned about a centre. Result: constraints (the pattern's id).",
            "properties": {
                "sketch": sketch(),
                "entities": ids("The originals (p<n>, c<n>)"),
                "center": point("The centre: an existing point, or a new point (fixed when its coordinates are numbers, held by dimensions when one is an expression)"),
                "count": {"type": "integer", "minimum": 2, "maximum": 1000, "description": "Instances, the originals included"},
                "angle": value_input("The angle; default a full turn")
            },
            "required": ["sketch", "entities", "center", "count"],
            "examples": [{"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": ["c5"], "center": ["w/2", 0], "count": 6}]
        }),
        "sketch.rectangular_pattern" => json!({
            "description": "Copies in rows and columns. Result: constraints (the pattern's id).",
            "properties": {
                "sketch": sketch(),
                "entities": ids("The originals (p<n>, c<n>)"),
                "direction": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "default": [1, 0], "description": "The first direction"},
                "direction2": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "The second direction, not parallel to the first; default a quarter turn from the first"},
                "count": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "integer", "minimum": 1}, "description": "[n1, n2]"},
                "spacing": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": ["number", "string"]}, "description": "[s1, s2]: values, signed, not zero"},
                "symmetric": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "boolean"}, "description": "Per direction: the originals in the middle (an odd count)"}
            },
            "required": ["sketch", "entities", "count", "spacing"],
            "examples": [{"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c5"], "count": [3, 2], "spacing": [10, "pitch"]}]
        }),
        "sketch.edit_pattern" => json!({
            "description": "Changes a pattern; the copies follow.",
            "properties": {
                "sketch": sketch(),
                "pattern": id("The pattern (k<n>)"),
                "count": {"type": ["integer", "array"], "description": "n, or [n1, n2]"},
                "angle": value_input("A circular pattern's angle"),
                "spacing": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": ["number", "string"]}, "description": "A rectangular pattern's spacing"},
                "direction": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "A rectangular pattern's first direction"},
                "direction2": {"type": ["array", "null"], "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "A rectangular pattern's second direction; null: a quarter turn from the first"},
                "center": point("A circular pattern's centre"),
                "entities": ids("The originals")
            },
            "required": ["sketch", "pattern"],
            "examples": [{"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": "k5", "count": 8}]
        }),
        "sketch.project" => json!({
            "description": "Projects edges, faces, vertices or another sketch's geometry into the sketch as fixed reference entities.",
            "properties": {
                "sketch": sketch(),
                "source": {"type": ["string", "object"], "description": "An edge, face or vertex name of a body before the sketch, {\"sketch\": \"F1\", \"curve\": \"c4\"}, {\"sketch\": \"F1\", \"point\": \"p3\"}, {\"sketch\": \"F1\"} or {\"feature\": \"F5\"}; left out only for a whole body's section"},
                "body": text("The body of a topological source (F2.b0)"),
                "linked": flag(false, "Follows its source when the model changes"),
                "occurrence": text("Another component's geometry: the occurrence path that shows it"),
                "context": text("Where the active component is seen"),
                "section": flag(false, "Where the sketch's plane cuts the source instead")
            },
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.project", "sketch": "F3", "source": {"sketch": "F1", "curve": "c4"}, "linked": true}]
        }),
        "sketch.add_code" => json!({
            "description": "A QR code of a text.",
            "properties": {
                "sketch": sketch(),
                "text": text("The content"),
                "at": position("Its lower left corner; default [0, 0]"),
                "module": json!({"type": "number", "default": 1, "description": "A module's size, mm"}),
                "angle": number("Its angle, radians"),
                "kind": choice(&["qr"], "The code"),
                "ecc": json!({"type": "string", "enum": ["L", "M", "Q", "H"], "default": "M", "description": "The error correction level"})
            },
            "required": ["sketch", "text"],
            "examples": [{"cmd": "sketch.add_code", "sketch": "F1", "text": "https://example.org", "module": 0.8}]
        }),
        "sketch.edit_code" => json!({
            "description": "Changes a QR code.",
            "properties": {
                "sketch": sketch(),
                "id": id("The code (t<n>)"),
                "text": text("The content"),
                "at": position("Its lower left corner"),
                "module": number("A module's size, mm"),
                "angle": number("Its angle, radians"),
                "ecc": choice(&["L", "M", "Q", "H"], "The error correction level")
            },
            "required": ["sketch", "id"],
            "examples": [{"cmd": "sketch.edit_code", "sketch": "F1", "id": "t5", "ecc": "H"}]
        }),
        "sketch.clean_up_projections" => json!({
            "description": "Removes duplicate and overlapping projected curves.",
            "properties": {"sketch": sketch(), "merge": flag(true, "Also lines on or overlapping a collinear one")},
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.clean_up_projections", "sketch": "F1"}]
        }),
        "sketch.clean_up_stale_projections" => json!({
            "description": "Removes or trims projected curves that no longer match their source.",
            "properties": {"sketch": sketch()},
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.clean_up_stale_projections", "sketch": "F1"}]
        }),
        "sketch.project_again" => json!({
            "description": "Projects anew a linked projection that stopped following its source.",
            "properties": {
                "sketch": sketch(),
                "projection": {"type": ["integer", "string"], "description": "Its index, an entity of it (c4) or its source's name; all that do not follow when left out"},
                "successor": text("For one whose source edge is gone: one of its candidates")
            },
            "required": ["sketch"],
            "examples": [{"cmd": "sketch.project_again", "sketch": "F2"}]
        }),
        "sketch.convert_projected" => json!({
            "description": "Makes projected curves and points ordinary free sketch geometry.",
            "properties": {"sketch": sketch(), "entities": ids("The curves and points (c<n>, p<n>)")},
            "required": ["sketch", "entities"],
            "examples": [{"cmd": "sketch.convert_projected", "sketch": "F2", "entities": ["c4"]}]
        }),
        "sketch.add_equal_spacing" => json!({
            "description": "Holds anchors equally spaced, with a common spacing dimension when asked.",
            "properties": {
                "sketch": sketch(),
                "anchors": ids("The points and curves (p<n>, c<n>)"),
                "mode": choice(&["horizontal", "vertical", "along", "point_to_point"], "What the intervals measure"),
                "direction": id("along: the line (c<n>)"),
                "keep_order": flag(false, "Keep the anchors' order"),
                "dimension": flag(false, "A common spacing dimension"),
                "spacing": value_input("Its value (driving)"),
                "driven": flag(false, "The dimension only measures"),
                "text": position("Where its value is shown")
            },
            "required": ["sketch", "anchors", "mode"],
            "examples": [{"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": ["p1", "p2", "p3"], "mode": "horizontal", "spacing": 10}]
        }),
        "sketch.edit_equal_spacing" => json!({
            "description": "Changes an equal spacing.",
            "properties": {
                "sketch": sketch(),
                "constraint": id("The equal spacing (k<n>)"),
                "anchors": ids("Its anchors"),
                "mode": choice(&["horizontal", "vertical", "along", "point_to_point"], "What the intervals measure"),
                "direction": id("along: the line (c<n>)"),
                "keep_order": flag(false, "Keep the anchors' order")
            },
            "required": ["sketch", "constraint"],
            "examples": [{"cmd": "sketch.edit_equal_spacing", "sketch": "F1", "constraint": "k7", "mode": "vertical"}]
        }),
        "sketch.axis" => json!({
            "description": "The sketch's own axis or origin as an entity: the one there is, or a new one.",
            "properties": {"sketch": sketch(), "axis": choice(&["x", "y", "origin"], "Which")},
            "required": ["sketch", "axis"],
            "examples": [{"cmd": "sketch.axis", "sketch": "F1", "axis": "x"}]
        }),
        "sketch.paste" => json!({
            "description": "Pastes copied sketch geometry (a sketch_fragment answer's fragment).",
            "properties": {
                "sketch": sketch(),
                "fragment": object("The fragment"),
                "at": position("Where its base point goes; where it was when left out")
            },
            "required": ["sketch", "fragment"],
            "examples": [{"cmd": "sketch.paste", "sketch": "F1", "fragment": {"version": 1, "base": [0, 0], "entities": []}, "at": [20, 0]}]
        }),
        "sketch.scale" => json!({
            "description": "Scales the whole sketch once by a factor about a point.",
            "properties": {
                "sketch": sketch(),
                "factor": number("The factor"),
                "center": point_once("The centre; the sketch origin when left out")
            },
            "required": ["sketch", "factor"],
            "examples": [{"cmd": "sketch.scale", "sketch": "F1", "factor": 2, "center": "p1"}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A 3D sketch command's schema (fields without `cmd`).
fn spatial(name: &str) -> Option<Value> {
    let spec = match name {
        "sketch3d.create" => json!({
            "description": "Adds a 3D sketch at the timeline marker. Result: uid, name.",
            "properties": {
                "support": {"type": ["string", "object"], "description": "What its frame follows: xy (the default), xz, yz, a construction plane \"F5\" or a planar face {\"face\", \"body\"}"},
                "name": text("Its name; default the next 3DSketch<n>")
            },
            "required": [],
            "examples": [{"cmd": "sketch3d.create"}, {"cmd": "sketch3d.create", "support": "xz", "name": "Route"}]
        }),
        "sketch3d.add_point" => json!({
            "description": "A new point. Result: made.",
            "properties": {
                "sketch": sketch3d(),
                "at": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": ["number", "string"]}, "description": format!("Where: [x, y, z]; {EXPRESSIONS}")},
                "source": source("Attached to a source point, or put on a source curve or face"),
                "fixed": flag(false, "Fixed where it is")
            },
            "required": ["sketch", "at"],
            "examples": [{"cmd": "sketch3d.add_point", "sketch": "F1", "at": ["w", 0, "h"]}]
        }),
        "sketch3d.add_line" => json!({
            "description": "A line. Result: made, start, end.",
            "properties": {
                "sketch": sketch3d(),
                "start": point3("Its start"),
                "end": point3("Its end"),
                "construction": flag(false, "A construction line"),
                "axis": axis3("Kept along this frame axis"),
                "sense": sense("Along the axis from start to end"),
                "length": value_input("A driving true length"),
                "offsets": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": ["number", "string", "null"]}, "description": "Driving signed offsets from start to end along X, Y and Z; null leaves one free"},
                "start_source": source("Attaches the start"),
                "end_source": source("Attaches the end"),
                "frame": object("A rotated placement plane {\"x\", \"normal\"}: axis and offsets along its axes")
            },
            "required": ["sketch", "start", "end"],
            "examples": [{"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0], "end": [40, 0, 0], "axis": "x", "length": "w", "start_source": "origin"}]
        }),
        "sketch3d.add_arc" => json!({
            "description": "An arc, counter-clockwise about its normal. Result: made, center, start, end.",
            "properties": {
                "sketch": sketch3d(),
                "mode": choice(&["three_point", "center", "tangent"], "three_point (through), center (center, normal or normal_axis) or tangent (from)"),
                "start": point3("Its start"),
                "end": point3("Its end"),
                "through": position3("three_point: a place it passes (no point is made there)"),
                "center": point3("center: its centre"),
                "normal": position3("center: its normal"),
                "from": id("tangent: the curve whose end start is (c<n>)"),
                "construction": flag(false, "A construction arc"),
                "normal_axis": axis3("Keep Plane: the normal along this frame axis"),
                "normal_sense": sense("The normal's sense"),
                "radius": value_input("A driving radius"),
                "arc_length": value_input("A driving arc length"),
                "start_source": source("Attaches the start"),
                "end_source": source("Attaches the end")
            },
            "required": ["sketch", "mode", "start", "end"],
            "examples": [{"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "three_point", "start": [0, 0, 0], "through": [5, 5, 0], "end": [10, 0, 0]}]
        }),
        "sketch3d.add_circle" => json!({
            "description": "A circle with a driving radius dimension.",
            "properties": {
                "sketch": sketch3d(),
                "center": point3("Its centre"),
                "radius": value_input("Its radius (with diameter: its diameter)"),
                "normal": position3("Its normal"),
                "normal_axis": axis3("Its normal kept along this frame axis"),
                "normal_sense": sense("The normal's sense"),
                "plane": plane3(),
                "through": ids("Existing points held on it (p<n>)"),
                "dimension": flag(true, "A driving radius dimension; false: radius is a number that only starts the solve"),
                "diameter": flag(false, "The dimension and radius a diameter"),
                "construction": flag(false, "A construction circle")
            },
            "required": ["sketch", "center", "radius"],
            "examples": [{"cmd": "sketch3d.add_circle", "sketch": "F1", "center": [0, 0, "h"], "radius": 5, "normal_axis": "z"}]
        }),
        "sketch3d.add_spline" => json!({
            "description": "A spline through its points, or with degree by its control points. Result: made, spline_points.",
            "properties": {
                "sketch": sketch3d(),
                "points": {"type": "array", "minItems": 2, "items": point3("A point"), "description": "Its points in order"},
                "degree": {"type": "integer", "minimum": 1, "description": "A B-spline by these control points; left out: a cubic through them"},
                "closed": flag(false, "Fit points only: periodic through the first point"),
                "ends": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "string", "enum": ["natural", "free"]}, "description": "An open fit-point spline's end conditions [start, end]"},
                "construction": flag(false, "A construction spline")
            },
            "required": ["sketch", "points"],
            "examples": [{"cmd": "sketch3d.add_spline", "sketch": "F1", "points": [[0, 0, 0], [10, 5, 2], [20, 0, "h"]]}]
        }),
        "sketch3d.add_constraint" => json!({
            "description": "Adds a relation. Result: made.",
            "properties": {
                "sketch": sketch3d(),
                "constraint": {"type": "object", "properties": {"type": {"type": "string"}}, "required": ["type"], "description": "A relation as in the definition, without id: coincident, fix, attach, align, point_on, midpoint, tangent, equal, normal_align, on_sketch_plane, parallel, perpendicular, symmetric, concentric, curvature, on_plane; model_reference {\"command\": \"sketch3d.add_constraint\"} gives their fields"}
            },
            "required": ["sketch", "constraint"],
            "examples": [{"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint": {"type": "coincident", "a": "p1", "b": "p4"}}]
        }),
        "sketch3d.add_dimension" => json!({
            "description": "Adds a dimension, driving or driven. Result: made.",
            "properties": {
                "sketch": sketch3d(),
                "dimension": {"type": "object", "properties": {"type": {"type": "string", "enum": SPATIAL_DIMENSIONS}}, "required": ["type"], "description": "What it measures, as in the definition without id and value"},
                "value": value_input("Its value; left out: what it measures now"),
                "driven": flag(false, "Only measures (takes no value)")
            },
            "required": ["sketch", "dimension"],
            "examples": [{"cmd": "sketch3d.add_dimension", "sketch": "F1", "dimension": {"type": "length", "line": "c3"}, "value": 40}]
        }),
        "sketch3d.set_dimension" => json!({
            "description": "Changes a dimension's value; a driven one becomes driving.",
            "properties": {"sketch": sketch3d(), "dimension": id("The dimension (k<n>)"), "value": value_input("Its value")},
            "required": ["sketch", "dimension", "value"],
            "examples": [{"cmd": "sketch3d.set_dimension", "sketch": "F1", "dimension": "k3", "value": "w"}]
        }),
        "sketch3d.set_driven" => json!({
            "description": "Makes a dimension driven or driving.",
            "properties": {"sketch": sketch3d(), "dimension": id("The dimension (k<n>)"), "driven": flag(true, "Driven (true) or driving")},
            "required": ["sketch", "dimension"],
            "examples": [{"cmd": "sketch3d.set_driven", "sketch": "F1", "dimension": "k3"}]
        }),
        "sketch3d.set_construction" => json!({
            "description": "Makes curves construction curves or ordinary ones.",
            "properties": {"sketch": sketch3d(), "curves": ids("The curves (c<n>)"), "construction": flag(true, "Construction")},
            "required": ["sketch", "curves"],
            "examples": [{"cmd": "sketch3d.set_construction", "sketch": "F1", "curves": ["c3"]}]
        }),
        "sketch3d.fix" => json!({
            "description": "Fixes points where they are, or removes their Fix.",
            "properties": {"sketch": sketch3d(), "points": ids("The points (p<n>)"), "fixed": flag(true, "Fixed")},
            "required": ["sketch", "points"],
            "examples": [{"cmd": "sketch3d.fix", "sketch": "F1", "points": ["p1"]}]
        }),
        "sketch3d.detach" => json!({
            "description": "Removes points' attachments and their relations with reference curves.",
            "properties": {"sketch": sketch3d(), "points": ids("The points (p<n>)")},
            "required": ["sketch", "points"],
            "examples": [{"cmd": "sketch3d.detach", "sketch": "F1", "points": ["p1"]}]
        }),
        "sketch3d.attach" => json!({
            "description": "Attaches a point to a source point, or puts it on a source curve or face.",
            "properties": {
                "sketch": sketch3d(),
                "point": id("The point (p<n>)"),
                "source": source("The source"),
                "projected": flag(false, "The source projected onto the sketch's plane instead of in true 3D")
            },
            "required": ["sketch", "point", "source"],
            "examples": [{"cmd": "sketch3d.attach", "sketch": "F1", "point": "p1", "source": "origin"}]
        }),
        "sketch3d.include" => json!({
            "description": "Makes (or reuses) a source curve's reference curve.",
            "properties": {"sketch": sketch3d(), "source": source("The source curve"), "projected": flag(false, "Projected onto the sketch's plane")},
            "required": ["sketch", "source"],
            "examples": [{"cmd": "sketch3d.include", "sketch": "F2", "source": {"sketch": "F1", "curve": "c3"}}]
        }),
        "sketch3d.replace_reference" => json!({
            "description": "Points an attachment or a reference curve at another source of its kind.",
            "properties": {"sketch": sketch3d(), "item": id("An attachment (k<n>) or a reference curve (c<n>)"), "source": source("The new source")},
            "required": ["sketch", "item", "source"],
            "examples": [{"cmd": "sketch3d.replace_reference", "sketch": "F2", "item": "k3", "source": "origin"}]
        }),
        "sketch3d.release_from_plane" => json!({
            "description": "Takes points out of the On Sketch Plane relation.",
            "properties": {"sketch": sketch3d(), "points": ids("The points (p<n>)")},
            "required": ["sketch", "points"],
            "examples": [{"cmd": "sketch3d.release_from_plane", "sketch": "F1", "points": ["p2"]}]
        }),
        "sketch3d.release_plane" => json!({
            "description": "Frees plane-bound arcs' and circles' normals.",
            "properties": {"sketch": sketch3d(), "curves": ids("The curves (c<n>)")},
            "required": ["sketch", "curves"],
            "examples": [{"cmd": "sketch3d.release_plane", "sketch": "F1", "curves": ["c4"]}]
        }),
        "sketch3d.remove" => json!({
            "description": "Removes relations, dimensions, curves and points.",
            "properties": {"sketch": sketch3d(), "items": ids("p<n>, c<n>, k<n>"), "dependents": flag(false, "Also what uses a listed point")},
            "required": ["sketch", "items"],
            "examples": [{"cmd": "sketch3d.remove", "sketch": "F1", "items": ["c3"]}]
        }),
        "sketch3d.drag" => json!({
            "description": "Pulls points and spline end tangents toward targets as far as the relations allow.",
            "properties": {
                "sketch": sketch3d(),
                "points": {"type": "array", "items": {"type": "object", "properties": {"point": id("p<n>"), "to": position3("Its target")}, "required": ["point", "to"], "additionalProperties": false}, "description": "Points and their targets"},
                "tangents": {"type": "array", "items": {"type": "object", "properties": {"curve": id("An open fit-point spline (c<n>)"), "end": choice(&["start", "end"], "Which end"), "to": position3("The wanted end tangent")}, "required": ["curve", "end", "to"], "additionalProperties": false}, "description": "End tangents and their targets"},
                "preview": flag(false, "Only say where they would go")
            },
            "required": ["sketch"],
            "examples": [{"cmd": "sketch3d.drag", "sketch": "F1", "points": [{"point": "p2", "to": [12, 0, 3]}]}]
        }),
        "sketch3d.convert" => json!({
            "description": "Turns a planar sketch into a 3D sketch in place.",
            "properties": {"sketch": feature_uid("The planar sketch")},
            "required": ["sketch"],
            "examples": [{"cmd": "sketch3d.convert", "sketch": "F1"}]
        }),
        "sketch3d.convert_to_2d" => json!({
            "description": "Turns a 3D sketch whose geometry lies in its plane back into a planar sketch.",
            "properties": {"sketch": sketch3d()},
            "required": ["sketch"],
            "examples": [{"cmd": "sketch3d.convert_to_2d", "sketch": "F1"}]
        }),
        "sketch3d.fillet" => json!({
            "description": "Rounds corners between lines.",
            "properties": {
                "sketch": sketch3d(),
                "a": id("A line of the corner (c<n>)"),
                "b": id("The other line"),
                "points": ids("Corners: points where two ordinary lines end"),
                "radius": value_input("The radius")
            },
            "required": ["sketch", "radius"],
            "examples": [{"cmd": "sketch3d.fillet", "sketch": "F1", "points": ["p2"], "radius": 5}]
        }),
        "sketch3d.add_ellipse" => json!({
            "description": "A plane-bound ellipse.",
            "properties": {
                "sketch": sketch3d(),
                "center": point3("Its centre"),
                "major": point3("The end of its major axis"),
                "minor_radius": number("Where the solve starts the minor radius, mm"),
                "plane": plane3(),
                "major_value": value_input("A driving major radius"),
                "minor_value": value_input("A driving minor radius"),
                "construction": flag(false, "A construction ellipse")
            },
            "required": ["sketch", "center", "major", "minor_radius", "plane"],
            "examples": [{"cmd": "sketch3d.add_ellipse", "sketch": "F1", "center": [0, 0, 0], "major": [20, 0, 0], "minor_radius": 10, "plane": "xy"}]
        }),
        "sketch3d.add_rectangle" => json!({
            "description": "A plane-bound rectangle. Result: made, corners, center, construction, relations.",
            "properties": {
                "sketch": sketch3d(),
                "mode": choice(&["two_point", "center", "three_point"], "two_point (a, b), center (center, corner) or three_point (a, b, c)"),
                "a": point3("A corner"),
                "b": point3("A corner"),
                "c": position3("three_point: a place on the opposite side"),
                "center": point3("center: the centre"),
                "corner": point3("center: a corner"),
                "plane": plane3(),
                "width": value_input("A driving length of the first side"),
                "height": value_input("A driving length of the second side"),
                "construction": flag(false, "Construction lines")
            },
            "required": ["sketch", "mode", "plane"],
            "examples": [{"cmd": "sketch3d.add_rectangle", "sketch": "F1", "mode": "two_point", "a": [0, 0, "h"], "b": [20, 10, "h"], "plane": "xy"}]
        }),
        "sketch3d.add_polygon" => json!({
            "description": "A plane-bound regular polygon.",
            "properties": {
                "sketch": sketch3d(),
                "center": point3("Its centre"),
                "vertex": point3("Its first corner (inscribed), or a side's middle (circumscribed: a place of numbers)"),
                "sides": {"type": "integer", "minimum": 3, "description": "Its number of sides"},
                "inscribed": flag(true, "Inscribed or circumscribed"),
                "plane": plane3(),
                "radius": value_input("A driving radius of its construction circle"),
                "construction": flag(false, "Construction lines")
            },
            "required": ["sketch", "center", "vertex", "sides", "plane"],
            "examples": [{"cmd": "sketch3d.add_polygon", "sketch": "F1", "center": [0, 0, 0], "vertex": [10, 0, 0], "sides": 6, "plane": "xy"}]
        }),
        "sketch3d.add_text" => json!({
            "description": "A plane-bound text.",
            "properties": {
                "sketch": sketch3d(),
                "text": text("The text"),
                "at": position3("Its anchor (a text has no point there)"),
                "plane": plane3(),
                "height": number("Its height, mm"),
                "frame": position3("The frame's opposite corner: a text in a frame"),
                "angle": number("Its angle, radians"),
                "font": text("An installed font's family"),
                "bold": flag(false, "Bold"),
                "italic": flag(false, "Italic"),
                "align": choice(&["left", "center", "right"], "Horizontal alignment"),
                "valign": choice(&["baseline", "bottom", "middle", "top"], "Vertical alignment")
            },
            "required": ["sketch", "text", "at", "plane", "height"],
            "examples": [{"cmd": "sketch3d.add_text", "sketch": "F1", "text": "A", "at": [0, 0, 0], "plane": "xy", "height": 5}]
        }),
        "sketch3d.paste" => json!({
            "description": "Pastes copied sketch geometry (a sketch_fragment answer's fragment, of a 3D or a planar sketch).",
            "properties": {
                "sketch": sketch3d(),
                "fragment": object("The fragment"),
                "at": position3("Where the copy's base point goes"),
                "plane": choice(&["xy", "yz", "zx"], "The frame plane a planar copy lies on, through at")
            },
            "required": ["sketch", "fragment"],
            "examples": [{"cmd": "sketch3d.paste", "sketch": "F1", "fragment": {"version": 1, "base": [0, 0], "entities": []}, "at": [0, 0, 10]}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A sketch command's schema as [`super`] writes its own.
pub(super) fn spec(name: &str) -> Option<Value> {
    planar(name).or_else(|| spatial(name))
}
