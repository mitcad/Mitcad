// SPDX-License-Identifier: MIT
//! The JSON Schemas of the drawing commands (mitcad#1918): drawings kept in
//! the design, their sheets, views, annotations, revisions, title blocks,
//! the model's items and line styles in views (`core/model-api/src/drawings.rs`,
//! `drawing_annotations.rs`, `drawing_templates.rs`, `drawing_lines.rs`),
//! and cut sheets (`cut_sheets.rs`), written from their fields as [`super`]
//! writes the others.

use serde_json::{Value, json};

use super::{choice, flag, id, ids, number, object, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "add_drawing",
    "edit_drawing",
    "delete_drawing",
    "add_drawing_sheet",
    "edit_drawing_sheet",
    "delete_drawing_sheet",
    "add_drawing_view",
    "edit_drawing_view",
    "delete_drawing_view",
    "add_drawing_annotation",
    "edit_drawing_annotation",
    "delete_drawing_annotation",
    "add_drawing_revision",
    "edit_drawing_revision",
    "delete_drawing_revision",
    "drawing_model_items",
    "keep_drawing_model_items",
    "edit_drawing_line",
    "edit_drawing_title_block",
    "propose_drawing_welds",
    "add_cut_sheet",
    "edit_cut_sheet",
    "add_cut_sheet_parts",
    "edit_cut_sheet_part",
    "delete_cut_sheet_parts",
    "nest_cut_sheet",
];

/// The named sheet sizes (`SheetSize`).
const SHEET_SIZES: &[&str] = &[
    "A4", "A3", "A2", "A1", "A0", "ANSI_A", "ANSI_B", "ANSI_C", "ANSI_D", "ANSI_E",
];

/// The sides of a projected view (`Side`).
const SIDES: &[&str] = &[
    "right",
    "left",
    "above",
    "below",
    "above_right",
    "above_left",
    "below_right",
    "below_left",
];

/// The annotation types a view takes (`AnnotationType`), besides the
/// dimension whose picks choose its type and the drawn text, line and arc.
const ANNOTATIONS: &[&str] = &[
    "linear",
    "angle",
    "radius",
    "diameter",
    "ordinate",
    "center_mark",
    "centerline",
    "note",
    "datum",
    "frame",
    "finish",
    "hole_table",
    "parts_list",
    "balloon",
    "center_pattern",
    "edge_extension",
    "hole_callout",
    "weld",
];

const PEN_WIDTHS: &[&str] = &["thin", "thick", "extra_thick"];
const PEN_PATTERNS: &[&str] = &["continuous", "dashed", "chain", "phantom"];
const PEN_NAMES: &[&str] = &["thin", "thick", "extra_thick", "hidden", "chain"];

fn drawing() -> Value {
    id("The drawing: its id (D1) or name")
}

fn cut_sheet() -> Value {
    id("The cut sheet: its id (D1) or name")
}

fn sheet(description: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "description": description})
}

fn view() -> Value {
    json!({"type": "integer", "minimum": 1, "description": "The view by its id in the drawing (the drawing query lists them)"})
}

/// A point or an offset on the sheet, mm.
fn p2(description: &str) -> Value {
    json!({"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"},
           "description": format!("{description}: [x, y] in sheet mm")})
}

fn size() -> Value {
    json!({
        "description": "The sheet size: a named one, or {\"width\", \"height\"} in mm of the user's own (10 to 10 000 each way)",
        "oneOf": [
            {"type": "string", "enum": SHEET_SIZES},
            {"type": "object", "properties": {"width": {"type": "number"}, "height": {"type": "number"}}, "required": ["width", "height"]}
        ]
    })
}

fn orientation() -> Value {
    choice(&["landscape", "portrait"], "The sheet's orientation")
}

fn projection() -> Value {
    choice(
        &["third_angle", "first_angle"],
        "The projection method (ISO 5456-2)",
    )
}

/// A drawing's dimensioning standard (mitcad#2047).
fn standard(default: &str) -> Value {
    choice(
        &["iso", "asme"],
        &format!(
            "The dimensioning standard: iso (ISO 129-1, values along their dimension lines) or asme (ASME Y14.5, values horizontal in a break of the line; inch values without a leading zero or the unit){default}"
        ),
    )
}

fn title_fields() -> Value {
    json!({
        "type": "object",
        "description": "The title block's fields: left out, a field stays; \"\" clears it",
        "properties": {
            "title": {"type": "string"}, "number": {"type": "string"},
            "author": {"type": "string"}, "date": {"type": "string"}
        },
        "additionalProperties": false
    })
}

fn sheet_annotations() -> Value {
    json!({
        "type": "object",
        "description": "What the sheet works out by itself",
        "properties": {
            "dimensions": {"type": "boolean", "default": false, "description": "Overall sizes and hole diameters"},
            "hole_table": {"type": "boolean", "default": false},
            "sketch_dimensions": {"type": "boolean", "default": false, "description": "Sketch dimensions seen square on"}
        },
        "additionalProperties": false
    })
}

fn units() -> Value {
    text("The sheet's length unit by its symbol (mm, in); \"\" for the design's")
}

fn template() -> Value {
    json!({"type": ["string", "object", "null"], "description": "The sheet's template: a built-in one's id (iso, ansi, blank: no frame and no title block), a template's data as the drawing_templates query lists it, or \"\" or null for the simple title block"})
}

fn scale() -> Value {
    json!({"type": ["string", "number"], "description": "\"1:2\", \"2:1\" or a positive number; \"parent\" for its parent's; \"fit\" for one that fits the sheet (a base view's)"})
}

fn style() -> Value {
    json!({
        "type": "object",
        "description": "How the view is drawn",
        "properties": {
            "hidden_lines": {"type": "boolean", "default": false},
            "tangent_edges": {"type": "boolean", "default": false},
            "shaded": {"type": "boolean", "default": false},
            "polygonal": {"type": "boolean", "default": false, "description": "Projected from a triangulation (quicker on large bodies)"}
        },
        "additionalProperties": false
    })
}

fn direction() -> Value {
    json!({
        "description": "A base view's direction: a standard view (front, back, top, bottom, left, right, iso, iso_top_left, iso_bottom_right, iso_bottom_left), {\"toward_eye\": [x, y, z], \"x_axis\": [x, y, z]} or {\"toward_eye\", \"up\"} (a camera's); the front when left out",
        "type": ["string", "object"]
    })
}

fn subject() -> Value {
    json!({
        "type": "object",
        "description": "What a base view shows: {\"type\": \"design\"} (the bodies shown now), {\"type\": \"occurrence\", \"occurrence\": \"O1/O4\"} or {\"type\": \"bodies\", \"bodies\": [{\"body\": \"F2.b0\", \"occurrence\": \"O1\"}]} or a sheet-metal body's Flat Pattern {\"type\": \"flat_pattern\", \"body\": \"F3.b0\", \"occurrence\": \"O1\", \"bend_notes\": false} (its developed blank seen square on with its bend lines; occurrence and bend_notes optional)",
        "properties": {"type": {"type": "string", "enum": ["design", "occurrence", "bodies", "flat_pattern"]}},
        "required": ["type"]
    })
}

fn picks() -> Value {
    json!({
        "type": "array",
        "description": "What it is attached to: picks of model topology seen in the view ({\"body\", \"occurrence\", \"edge\" or \"face\", \"point\": start|end|center, ...}, as the drawing_pick query answers), or sheet points [x, y] (mm) where the nearest pick of the view is found",
        "items": {"type": ["object", "array"]}
    })
}

fn along() -> Value {
    choice(
        &["horizontal", "vertical", "aligned"],
        "A linear dimension's direction",
    )
}

/// An object whose fields a reference section gives.
fn described(description: &str, section: &str) -> Value {
    object(&format!(
        "{description} (model_reference {{\"section\": \"{section}\"}})"
    ))
}

fn nullable(description: &str, kind: &str) -> Value {
    json!({"type": [kind, "null"], "description": format!("{description}; null sets it back to its default")})
}

fn break_input() -> Value {
    json!({
        "type": "object",
        "description": "A break: two sheet points whose coordinates along direction bound the band taken out",
        "properties": {
            "direction": {"type": "string", "enum": ["horizontal", "vertical"]},
            "from": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}},
            "to": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}},
            "gap": {"type": "number", "description": "Paper left between the sides, mm (6)"}
        },
        "required": ["direction", "from", "to"],
        "additionalProperties": false
    })
}

/// A body a cut sheet's part is made of.
fn part_bodies(description: &str) -> Value {
    json!({
        "type": "array",
        "description": format!("{description}: each a body's uid or name, or {{\"body\", \"occurrence\", \"count\"}} with the occurrence path that places it and how many of it (the command's count when left out)"),
        "items": {"type": ["string", "object"]}
    })
}

fn count() -> Value {
    json!({"type": "integer", "minimum": 1, "maximum": 1000, "description": "How many of each"})
}

/// A cut sheet's parts as their bodies' Flat Patterns (mitcad#1980).
fn flat() -> Value {
    json!({"type": "boolean", "default": false, "description": "Each body's Flat Pattern as its part (its developed blank, outline and bend lines) instead of a face's outline; a sheet-metal body only, and no face"})
}

/// A drawing command's schema (fields without `cmd`).
fn drawings(name: &str) -> Option<Value> {
    let spec = match name {
        "add_drawing" => json!({
            "description": "Adds a drawing to a component's Drawings folder with one sheet, or the sheets an export lays out (layout). Result: drawing, name, warnings.",
            "properties": {
                "component": id("The component (uid or name); the root's when left out"),
                "name": text("Its name; default the next Drawing<n>"),
                "size": size(),
                "orientation": orientation(),
                "projection": projection(),
                "title_block": title_fields(),
                "units": units(),
                "annotations": sheet_annotations(),
                "layout": described("export_drawing's options and bodies: its sheets laid out as File > Export Drawing does (the root's drawings only)", "Drawings in the design"),
                "template": template(),
                "decimals": {"type": "integer", "minimum": 0, "description": "The decimals of its dimensions and annotations without their own"},
                "text_height": number("The letters' height of its annotations without their own, mm"),
                "standard": standard("; the template's when left out (ANSI's asme), else iso")
            },
            "required": [],
            "examples": [
                {"cmd": "add_drawing", "name": "Bracket", "size": "A3", "projection": "first_angle"},
                {"cmd": "add_drawing", "size": {"width": 500, "height": 300}, "template": "iso", "title_block": {"title": "Bracket", "author": "MK"}}
            ]
        }),
        "edit_drawing" => json!({
            "description": "Renames a drawing and sets the defaults of its dimensions and annotations. Result: drawing.",
            "properties": {
                "drawing": drawing(),
                "name": text("Its new name"),
                "decimals": nullable("The decimals of its dimensions and annotations without their own", "integer"),
                "text_height": nullable("The letters' height of its annotations without their own, mm", "number"),
                "standard": standard("; every dimension of the drawing follows")
            },
            "required": ["drawing"],
            "examples": [{"cmd": "edit_drawing", "drawing": "D1", "name": "Bracket", "decimals": 1}, {"cmd": "edit_drawing", "drawing": "D1", "standard": "asme"}]
        }),
        "delete_drawing" => json!({
            "description": "Deletes a drawing (or a cut sheet). Result: drawing.",
            "properties": {"drawing": drawing()},
            "required": ["drawing"],
            "examples": [{"cmd": "delete_drawing", "drawing": "D1"}]
        }),
        "add_drawing_sheet" => json!({
            "description": "Adds a sheet with the last sheet's setup, without its views and notes. Result: drawing, sheet (its index).",
            "properties": {
                "drawing": drawing(),
                "sheet": sheet("Where it goes, from 0; the end when left out"),
                "name": text("Its name; default the next Sheet<n>"),
                "size": size(),
                "orientation": orientation(),
                "projection": projection(),
                "title_block": title_fields(),
                "units": units(),
                "annotations": sheet_annotations(),
                "template": template()
            },
            "required": ["drawing"],
            "examples": [{"cmd": "add_drawing_sheet", "drawing": "D1", "size": "A2"}]
        }),
        "edit_drawing_sheet" => json!({
            "description": "Changes a sheet's setup; what is given changes. Result: drawing, sheet.",
            "properties": {
                "drawing": drawing(),
                "sheet": sheet("The sheet, from 0; the first when left out"),
                "name": text("Its new name"),
                "size": size(),
                "orientation": orientation(),
                "projection": projection(),
                "title_block": title_fields(),
                "units": units(),
                "annotations": sheet_annotations(),
                "template": template()
            },
            "required": ["drawing"],
            "examples": [{"cmd": "edit_drawing_sheet", "drawing": "D1", "sheet": 0, "orientation": "portrait", "title_block": {"number": "100-01"}}]
        }),
        "delete_drawing_sheet" => json!({
            "description": "Deletes a sheet with its views; a drawing keeps one sheet at least. Result: drawing.",
            "properties": {
                "drawing": drawing(),
                "sheet": sheet("The sheet, from 0")
            },
            "required": ["drawing", "sheet"],
            "examples": [{"cmd": "delete_drawing_sheet", "drawing": "D1", "sheet": 1}]
        }),
        "add_drawing_view" => json!({
            "description": "Adds a view to a sheet: a base view of the model, or a projected, section or detail view of another view; points in sheet mm. Not on a cut sheet. Result: the view's id and place.",
            "properties": {
                "drawing": drawing(),
                "sheet": sheet("The sheet, from 0 (default 0)"),
                "type": choice(&["base", "projected", "section", "detail"], "The view's type"),
                "direction": direction(),
                "subject": subject(),
                "parent": json!({"type": "integer", "description": "The view it comes from (projected, section and detail views)"}),
                "side": choice(SIDES, "A projected view's side of its parent; from at when left out"),
                "at": p2("Where the view's middle goes (a detail's centre); beside its parent or in the sheet's middle when left out"),
                "line": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}}, "description": "A section's line on its parent: two sheet points [[x, y], [x, y]]"},
                "flip": flag(false, "A section seen the other way"),
                "bounded": flag(false, "A section limited to its line: only that stretch is cut and hatched, and its mark runs along it; the whole plane, marked across the whole parent, when false"),
                "letter": text("A section's or a detail's letter; the next free one when left out"),
                "hatch": json!({"type": "boolean", "description": "A section's cut faces hatched (on when left out)"}),
                "center": p2("A detail's circle on its parent: its centre"),
                "radius": number("A detail's circle on its parent: its radius on the sheet, mm"),
                "scale": scale(),
                "style": style(),
                "label": text("Its label; the default when left out"),
                "center_marks": json!({"type": "boolean", "description": "Centre marks and centre lines worked out from the model (on when left out)"}),
                "retrieved_dimensions": json!({"type": "boolean", "description": "The sketch dimensions it sees square on (off when left out)"}),
                "model_annotations": json!({"type": "boolean", "description": "The model's annotations it sees square on (off when left out)"}),
                "explode": text("A base view's explode view by name: its subject at the explode's places with thin phantom trail lines; assembled when left out")
            },
            "required": ["drawing", "type"],
            "examples": [
                {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front", "at": [120, 150], "scale": "1:2"},
                {"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1, "side": "right"},
                {"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1, "line": [[100, 150], [140, 150]]}
            ]
        }),
        "edit_drawing_view" => json!({
            "description": "Changes a view; what is given changes. On a cut sheet a part is moved and turned only (place, move, rotation).",
            "properties": {
                "drawing": drawing(),
                "view": view(),
                "place": p2("Its anchor's new place"),
                "move": p2("Moved by this (a drag)"),
                "scale": scale(),
                "style": style(),
                "label": text("Its label; \"\" for the default"),
                "label_offset": p2("Where its label goes from under the middle of its box, sheet mm; [0, 0] puts it back"),
                "label_move": p2("Its label moved by this on the sheet (a drag)"),
                "direction": direction(),
                "subject": subject(),
                "side": choice(SIDES, "A projected view's side of its parent"),
                "letter": text("A section's or a detail's letter"),
                "hatch": json!({"type": "boolean", "description": "A section's cut faces hatched"}),
                "flip": flag(false, "A section seen the other way"),
                "bounded": json!({"type": "boolean", "description": "A section limited to its line, or of the whole plane (one with its line on a parent)"}),
                "center": p2("A detail's circle: its centre"),
                "radius": number("A detail's circle: its radius on the sheet, mm"),
                "breaks": {"type": "array", "items": break_input(), "description": "The breaks, all of them (an empty list removes them)"},
                "add_break": break_input(),
                "aligned": json!({"type": "boolean", "description": "Kept in line with its parent"}),
                "center_marks": json!({"type": "boolean", "description": "Centre marks and centre lines worked out from the model"}),
                "retrieved_dimensions": json!({"type": "boolean", "description": "The sketch dimensions it sees square on"}),
                "model_annotations": json!({"type": "boolean", "description": "The model's annotations it sees square on"}),
                "rotation": number("How far it is turned on the sheet, degrees counter-clockwise"),
                "explode": text("A base view's explode view by name; \"\" shows the design assembled"),
                "bend_notes": json!({"type": "boolean", "description": "A Flat Pattern view's bend notes (direction, angle and inside radius beside each bend line) on or off"})
            },
            "required": ["drawing", "view"],
            "examples": [
                {"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "scale": "1:1", "style": {"hidden_lines": true}},
                {"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "move": [10, 0]}
            ]
        }),
        "delete_drawing_view" => json!({
            "description": "Deletes a view with the views that come from it (on a cut sheet: a part). Result: drawing, deleted (the views' ids).",
            "properties": {
                "drawing": drawing(),
                "view": view()
            },
            "required": ["drawing", "view"],
            "examples": [{"cmd": "delete_drawing_view", "drawing": "D1", "view": 3}]
        }),
        "add_drawing_annotation" => json!({
            "description": "Adds an annotation on a view (a dimension, centre mark, datum, frame, finish, table, balloon, ...), a text note on a sheet (type text without view), or a line, arc or text drawn on the sheet or in a view. Result: the annotation's id (ids for a series).",
            "properties": {
                "drawing": drawing(),
                "view": json!({"type": "integer", "description": "The view it is on (every type but a sheet's text note, line and arc)"}),
                "sheet": sheet("A text note's, a line's or an arc's sheet, from 0 (default 0)"),
                "type": json!({"type": "string", "description": format!("Its type: {}, dimension (its picks choose the type), text, line or arc", ANNOTATIONS.join(", "))}),
                "picks": picks(),
                "along": along(),
                "at": p2("Where its dimension line or text goes; a text note's place"),
                "format": described("Its value's format: prefix, decimals, own text, fit", "Annotations on drawing views"),
                "tolerance": described("Its tolerance", "Annotations on drawing views"),
                "text": text("A note's or a text's words"),
                "letter": text("A datum label's letter"),
                "frame": described("A feature control frame", "Annotations on drawing views"),
                "finish": described("A surface finish symbol", "Annotations on drawing views"),
                "weld": described("A weld symbol's welds, sizes and tail; a fillet on the arrow side when left out", "Weld symbols on drawing views"),
                "height": number("Its letters' height, mm"),
                "series": choice(&["baseline", "chain"], "Linear dimensions made at once: from the first pick to each other one (baseline) or from each to the next (chain)"),
                "table": described("A hole table's or a parts list's settings (at: its place)", "Tables on drawing views"),
                "balloons": flag(false, "A parts list's balloons, one on each row's bodies the view shows"),
                "points": {"type": "array", "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}}, "description": "A line's two ends, an arc's centre, start and end, sheet mm"},
                "pen": choice(PEN_NAMES, "A line's or an arc's pen by its older name"),
                "width": choice(PEN_WIDTHS, "A line's or an arc's width (over pen)"),
                "pattern": choice(PEN_PATTERNS, "A line's or an arc's pattern (over pen)"),
                "terminator": choice(&["triangle", "dot"], "A datum label's terminator; by its pick when left out")
            },
            "required": ["drawing", "type"],
            "examples": [
                {"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text", "text": "Break sharp edges", "at": [20, 30]},
                {"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear", "picks": [[100, 150], [140, 150]], "along": "horizontal", "at": [120, 170]},
                {"cmd": "add_drawing_annotation", "drawing": "D1", "type": "line", "points": [[10, 10], [60, 10]], "width": "thick"}
            ]
        }),
        "edit_drawing_annotation" => json!({
            "description": "Changes an annotation; what is given changes, null sets a field back to its default.",
            "properties": {
                "drawing": drawing(),
                "annotation": json!({"type": "integer", "description": "The annotation by its id"}),
                "move": p2("Its line or text moved by this (a drag)"),
                "slide": p2("A datum label's triangle or a frame's arrow dragged by this: its tip slides along its pick"),
                "at": p2("Its dimension line or text through this point; a text note's new place"),
                "picks": picks(),
                "along": along(),
                "format": described("What of its value's format changes", "Annotations on drawing views"),
                "tolerance": json!({"type": ["object", "null"], "description": "Its tolerance; null removes it (model_reference {\"section\": \"Annotations on drawing views\"})"}),
                "text": text("Its words"),
                "letter": text("A datum label's letter"),
                "frame": described("A feature control frame", "Annotations on drawing views"),
                "finish": json!({"type": ["object", "null"], "description": "A surface finish symbol; null removes it"}),
                "weld": described("A weld symbol's welds, sizes and tail as a whole", "Weld symbols on drawing views"),
                "line_offset": nullable("Its dimension line's offset, mm", "number"),
                "text_offset": json!({"type": ["array", "null"], "items": {"type": "number"}, "description": "Its text's offset [x, y], mm; null sets it back"}),
                "height": nullable("Its letters' height, mm (null: the drawing's)", "number"),
                "table": described("A hole table's or a parts list's settings", "Tables on drawing views"),
                "points": {"type": "array", "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}}, "description": "A line's or an arc's points, as when it was added"},
                "pen": choice(PEN_NAMES, "A line's or an arc's pen by its older name"),
                "width": choice(PEN_WIDTHS, "A line's or an arc's width"),
                "pattern": choice(PEN_PATTERNS, "A line's or an arc's pattern"),
                "terminator": json!({"type": ["string", "null"], "enum": ["triangle", "dot", null], "description": "A datum label's terminator; null: by its pick"})
            },
            "required": ["drawing", "annotation"],
            "examples": [
                {"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 3, "move": [5, 0]},
                {"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 4, "height": null, "text": "2x"}
            ]
        }),
        "delete_drawing_annotation" => json!({
            "description": "Deletes annotations (one, or several at once).",
            "properties": {
                "drawing": drawing(),
                "annotation": json!({"type": "integer", "description": "One annotation by its id"}),
                "annotations": {"type": "array", "items": {"type": "integer"}, "description": "Instead of annotation: several by their ids"}
            },
            "required": ["drawing"],
            "examples": [{"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": 3}, {"cmd": "delete_drawing_annotation", "drawing": "D1", "annotations": [3, 4]}]
        }),
        "add_drawing_revision" => json!({
            "description": "Adds a revision to a drawing's revision table.",
            "properties": {
                "drawing": drawing(),
                "revision": text("Not given for a new revision (refused)"),
                "letter": text("Its letter; the next one when left out"),
                "description": text("What changed"),
                "date": text("YYYY-MM-DD; today when left out"),
                "author": text("Who made it")
            },
            "required": ["drawing"],
            "examples": [{"cmd": "add_drawing_revision", "drawing": "D1", "description": "Hole moved", "author": "MK"}]
        }),
        "edit_drawing_revision" => json!({
            "description": "Changes a revision; what is given changes.",
            "properties": {
                "drawing": drawing(),
                "revision": text("The revision edited, by its letter"),
                "letter": text("Its new letter"),
                "description": text("What changed"),
                "date": text("YYYY-MM-DD; today when cleared"),
                "author": text("Who made it")
            },
            "required": ["drawing"],
            "examples": [{"cmd": "edit_drawing_revision", "drawing": "D1", "revision": "A", "description": "Hole moved 5 mm"}]
        }),
        "delete_drawing_revision" => json!({
            "description": "Deletes a revision.",
            "properties": {
                "drawing": drawing(),
                "revision": text("The revision by its letter")
            },
            "required": ["drawing", "revision"],
            "examples": [{"cmd": "delete_drawing_revision", "drawing": "D1", "revision": "A"}]
        }),
        "drawing_model_items" => json!({
            "description": "The given views of a sheet (all its views when none are) retrieve the sketch dimensions and the model's annotations they see square on; true switches each on, false leaves it. One undo step.",
            "properties": {
                "drawing": drawing(),
                "sheet": sheet("The sheet, from 0 (default 0)"),
                "views": {"type": "array", "items": {"type": "integer"}, "description": "The views by their ids; all the sheet's when left out"},
                "sketch_dimensions": json!({"type": "boolean", "default": true, "description": "Sketch dimensions"}),
                "model_annotations": json!({"type": "boolean", "default": true, "description": "The model's annotations"})
            },
            "required": ["drawing"],
            "examples": [{"cmd": "drawing_model_items", "drawing": "D1", "views": [1, 2], "model_annotations": false}]
        }),
        "keep_drawing_model_items" => json!({
            "description": "Keeps the model's annotations a view draws as the drawing's own annotations of that view, to be moved and edited without changing the model (the design's drawings only). One undo step.",
            "properties": {
                "drawing": drawing(),
                "view": view(),
                "names": ids("The model's annotations by the names the drawing query lists; all of the view's when left out")
            },
            "required": ["drawing", "view"],
            "examples": [{"cmd": "keep_drawing_model_items", "drawing": "D1", "view": 1}]
        }),
        "edit_drawing_line" => json!({
            "description": "A width and a pattern for the lines of an edge, a body or an occurrence in one view, or (reset) the view's own lines again. Not on a cut sheet.",
            "properties": {
                "drawing": drawing(),
                "view": view(),
                "at": p2("A point on the view: the edge a click there picks"),
                "of": choice(&["edge", "body", "component"], "What of the edge picked at at: its lines (the default), its body's or its occurrence's"),
                "body": id("Instead of at: the body (F2.b0)"),
                "occurrence": id("With body: the occurrence path that places it (O1/O4)"),
                "edge": id("With body: the edge's topological name"),
                "width": choice(PEN_WIDTHS, "The lines' width"),
                "pattern": choice(PEN_PATTERNS, "The lines' pattern"),
                "reset": flag(false, "The view's own lines again: the choice removed")
            },
            "required": ["drawing", "view"],
            "examples": [{"cmd": "edit_drawing_line", "drawing": "D1", "view": 1, "at": [110, 140], "of": "body", "width": "thick"}]
        }),
        "edit_drawing_title_block" => json!({
            "description": "Sets a sheet's title block as the title block editor leaves it: its size, lines, fields and images, kept in the sheet's copy of its template, and whether the template draws its frame and its title block.",
            "properties": {
                "drawing": drawing(),
                "sheet": sheet("The sheet, from 0 (default 0)"),
                "title_block": {
                    "type": "object",
                    "description": "The title block in the frame's lower right corner, mm from its lower left corner (model_reference {\"section\": \"Drawing templates of one's own: sizes, images and the title block (mitcad#1625)\"})",
                    "properties": {
                        "width": {"type": "number"},
                        "height": {"type": "number"},
                        "lines": {"type": "array", "items": {"type": "array", "minItems": 4, "maxItems": 4, "items": {"type": "number"}}, "description": "Thin lines [x0, y0, x1, y1]"},
                        "fields": {"type": "array", "items": {"type": "object"}, "description": "Its fields: a label and the value shown"},
                        "projection_symbol": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "Where the projection symbol starts"},
                        "images": {"type": "array", "items": {"type": "object"}, "description": "Images such as a logo"}
                    },
                    "required": ["width", "height"],
                    "additionalProperties": false
                },
                "show_frame": {"type": "boolean", "description": "Whether the sheet's template draws its frame (mitcad#2325); as it is when left out"},
                "show_title_block": {"type": "boolean", "description": "Whether the sheet's template draws its title block, its fields and the revision table (mitcad#2325); as it is when left out. A sheet without a title block is refused unless this is true"}
            },
            "required": ["drawing", "title_block"],
            "examples": [{"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": {"width": 180, "height": 40, "lines": [[0, 20, 180, 20], [90, 0, 90, 20]]}}]
        }),
        "propose_drawing_welds" => json!({
            "description": "Proposes a weld symbol for each BUILD joint of the given members (all the view shows when left out) whose parts the view shows: a fillet round the meeting part's section, all around where the joint closes, on an edge of the joint. Proposing again keeps edited proposals, adds new joints' and removes those of joints no longer there. One undo step.",
            "properties": {
                "drawing": drawing(),
                "view": view(),
                "members": ids("The members (BUILD parts) by their components; every joint the view shows when left out"),
                "picks": {"type": "array", "items": {"type": "object"}, "description": "Instead of members or with them: picks on the view (as an annotation's), each the member its body is of"},
                "weld": described("The weld each new proposal gets (sizes, tail); a fillet on the arrow side when left out", "Weld symbols on drawing views")
            },
            "required": ["drawing", "view"],
            "examples": [{"cmd": "propose_drawing_welds", "drawing": "D1", "view": 1},
                         {"cmd": "propose_drawing_welds", "drawing": "D1", "view": 1, "members": ["C2", "C3"],
                          "weld": {"arrow_side": {"symbol": "fillet", "size": 4}, "tail": "135"}}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A cut sheet command's schema (fields without `cmd`).
fn cut_sheets(name: &str) -> Option<Value> {
    let spec = match name {
        "add_cut_sheet" => json!({
            "description": "Adds a cut sheet to a component's Drawings folder (the root's when left out): its material's size and the parts it starts with, laid out. Result: drawing, name, component, parts (their ids), warnings.",
            "properties": {
                "component": id("The component (uid or name)"),
                "name": text("Its name; default the next free one"),
                "width": number("The material's width, mm (default 600)"),
                "height": number("The material's height, mm (default 300)"),
                "bodies": part_bodies("The bodies it starts with as parts"),
                "count": count(),
                "flat": flat(),
                "kerf": number("The cutting beam's width, mm (0 to 10): the export moves outer outlines out and holes in by half of it"),
                "nest": flag(false, "The parts placed by simple automatic nesting"),
                "gap": number("The room between nested parts and from the edges, mm; 5 when left out"),
                "cutting": cutting()
            },
            "required": [],
            "examples": [
                {"cmd": "add_cut_sheet", "name": "Plywood 12", "width": 2440, "height": 1220},
                {"cmd": "add_cut_sheet", "bodies": ["Side", {"body": "Shelf", "occurrence": "O2"}], "count": 2}
            ]
        }),
        "edit_cut_sheet" => json!({
            "description": "Renames a cut sheet and changes its material's size.",
            "properties": {
                "drawing": cut_sheet(),
                "name": text("Its new name"),
                "width": number("The material's width, mm"),
                "height": number("The material's height, mm"),
                "kerf": number("The cutting beam's width, mm (0 to 10; 0: none)"),
                "cutting": cutting()
            },
            "required": ["drawing"],
            "examples": [{"cmd": "edit_cut_sheet", "drawing": "D1", "width": 1220}]
        }),
        "add_cut_sheet_parts" => json!({
            "description": "Adds bodies as parts of a cut sheet, each count times. Result: parts (their ids), warnings.",
            "properties": {
                "drawing": cut_sheet(),
                "bodies": part_bodies("The bodies"),
                "face": id("The planar face they are seen along, by its topological name (one body only); the largest when left out"),
                "count": count(),
                "at": p2("Where the first one's middle goes; a free place when left out"),
                "flat": flat(),
                "marks": marks()
            },
            "required": ["drawing", "bodies"],
            "examples": [{"cmd": "add_cut_sheet_parts", "drawing": "D1", "bodies": ["Side"], "count": 2}]
        }),
        "edit_cut_sheet_part" => json!({
            "description": "Moves, turns or reorients a part; what is given changes.",
            "properties": {
                "drawing": cut_sheet(),
                "part": json!({"type": "integer", "description": "The part by its id"}),
                "place": p2("Its new place"),
                "move": p2("Moved by this"),
                "rotation": number("Its angle, degrees counter-clockwise"),
                "turn": number("Turned by this many degrees more (90: a quarter turn left)"),
                "face": json!({"type": ["string", "null"], "description": "A planar face of its body it is seen along, by its topological name; null for the largest"}),
                "marks": marks()
            },
            "required": ["drawing", "part"],
            "examples": [{"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 2, "turn": 90}]
        }),
        "delete_cut_sheet_parts" => json!({
            "description": "Deletes parts of a cut sheet.",
            "properties": {
                "drawing": cut_sheet(),
                "parts": {"type": "array", "minItems": 1, "items": {"type": "integer"}, "description": "The parts by their ids"}
            },
            "required": ["drawing", "parts"],
            "examples": [{"cmd": "delete_cut_sheet_parts", "drawing": "D1", "parts": [2, 3]}]
        }),
        "nest_cut_sheet" => json!({
            "description": "Places every part of a cut sheet by simple automatic nesting: each turned so its longer side lies along the sheet's width, in rows from the lower left corner, the tallest first (mitcad#1939). Result: drawing, warnings.",
            "properties": {
                "drawing": cut_sheet(),
                "gap": number("The room between parts and from the edges, mm; 5 when left out")
            },
            "required": ["drawing"],
            "examples": [{"cmd": "nest_cut_sheet", "drawing": "D1", "gap": 5}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A part's marks (mitcad#1939): sketches engraved or scored on it.
fn marks() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "sketch": {"type": "string", "description": "A sketch by its uid (F9)"},
                "kind": {"type": "string", "enum": ["engrave", "score"], "description": "Engraved or scored"}
            },
            "required": ["sketch", "kind"],
            "additionalProperties": false
        },
        "description": "Sketches whose curves and texts are engraved or scored on the part, laid along the face it is seen along"
    })
}

/// A cut sheet's plasma torch settings (mitcad#2553).
fn cutting() -> Value {
    json!({
        "type": "object",
        "properties": {
            "feed": number("The cut feed, mm/min (2000)"),
            "pierce_height": number("The torch's height while it pierces, mm (3.8)"),
            "pierce_delay": number("How long it waits after it fires, s (0.5)"),
            "cut_height": number("The torch's height while it cuts, mm (1.5)"),
            "travel_height": number("The height it moves across at, mm (10)"),
            "lead_in": number("The longest lead-in, mm (4)"),
            "units": choice(&["millimetres", "inches"], "The program's units"),
            "line_numbers": flag(false, "N words on every line")
        },
        "additionalProperties": false,
        "description": "The plasma torch's settings of the cut sheet's grbl program; add_cut_sheet: each left out takes its default, edit_cut_sheet: each left out stays as it is"
    })
}

/// A drawing or cut sheet command's schema as [`super`] writes its own.
pub(super) fn spec(name: &str) -> Option<Value> {
    drawings(name).or_else(|| cut_sheets(name))
}
