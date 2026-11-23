// SPDX-License-Identifier: MIT
//! The JSON Schemas of the document's own commands (mitcad#1918): the
//! timeline, parameters, bodies and the view's display state, named views,
//! timeline groups, analyses, Flat Patterns, the Center of Mass marker, a
//! body's print bed face and folder, and a flange's profile sketch, written
//! from their fields in `core/model-api/src/lib.rs` (`Command`) and the
//! types they read (`core/model/src/document/analyses.rs`, `display.rs`,
//! `sheet/flat.rs`) as [`super`] writes the others. The drift test
//! `core/model-tests/api/src/mcp_schema_tests.rs` checks them against the
//! API like the others.

use serde_json::{Value, json};

use super::{choice, feature_uid, flag, id, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "reorder_feature",
    "rename_body",
    "set_marker",
    "set_units",
    "rename_parameter",
    "remove_parameter_name",
    "delete_parameter",
    "recompute",
    "set_body_visible",
    "merge_undo",
    "set_feature_visible",
    "set_datum_display_size",
    "acknowledge_warnings",
    "add_named_view",
    "delete_named_view",
    "rename_named_view",
    "set_origin_visible",
    "set_isolation",
    "add_center_of_mass",
    "set_center_of_mass_visible",
    "delete_center_of_mass",
    "set_view_show",
    "set_folder_visible",
    "set_sketch_dimensions_shown",
    "set_body_transparent",
    "group_features",
    "ungroup",
    "rename_group",
    "add_analysis",
    "edit_analysis",
    "set_analysis_visible",
    "rename_analysis",
    "delete_analysis",
    "set_flat_pattern",
    "remove_flat_pattern",
    "set_print_bed_face",
    "sheet_flange_profile",
    "set_body_folder",
    "add_flat_pack",
    "add_standard_box",
    "edit_standard_box",
    "add_glue_flap",
    // Sketch blocks (mitcad#2110).
    "add_sketch_block",
    "rename_sketch_block",
    "remove_sketch_block",
    "edit_sketch_block",
    "finish_sketch_block",
    // Selection sets (mitcad#2192).
    "add_selection_set",
    "set_selection_set_members",
    "rename_selection_set",
    "delete_selection_set",
];

/// A selection set's member (mitcad#2192): a body, a feature, or an
/// occurrence alone.
fn set_member() -> Value {
    json!({
        "type": "object",
        "description": "A body (body: uid or name at the marker) or a feature (feature: uid or name; sketches, construction geometry and joints are features), with occurrence: the path from the root that places it when it is not in the root; or a component placed in the design (occurrence alone)",
        "properties": {
            "body": text("A body's uid (F2.b0) or name"),
            "feature": text("A feature's uid (F5) or name"),
            "occurrence": text("The occurrence path from the root, by uids (O1/O4) or names (Arm:1/Pin:2)")
        },
        "additionalProperties": false
    })
}

fn set_members(description: &str) -> Value {
    json!({"type": "array", "items": set_member(), "description": description})
}

/// The standard box styles (mitcad#1939).
const BOX_STYLES: &[&str] = &[
    "fefco_0201",
    "ecma_a20",
    "fefco_0203",
    "fefco_0206",
    "fefco_0421",
    "fefco_0426",
    "fefco_0427",
    "fefco_0470",
    "fefco_0471",
];

/// A length in mm, or null to take it away.
fn number_or_null(description: &str) -> Value {
    json!({"type": ["number", "null"], "description": description})
}

/// The options of add_standard_box (mitcad#1939).
fn standard_box_fields() -> Value {
    json!({
        "style": choice(BOX_STYLES, "fefco_0201 (a slotted box), ecma_a20 (a reverse tuck end carton), fefco_0203 (a full overlap slotted box), fefco_0206 (a slotted box with meeting end flaps and full overlap side flaps), or a folder with a hinged lid: fefco_0421 (double end walls, a tuck), fefco_0427 (double end walls, lid ears and a front flap with ears), fefco_0426 (a double front wall, corner flaps, lid ears and a front flap), fefco_0470 (end walls on the back wall with floor flaps, a tuck) or fefco_0471 (as 0470 with a front flap with ears)"),
        "length": {"type": "number", "description": "Inside length, mm"},
        "width": {"type": "number", "description": "Inside width, mm"},
        "height": {"type": "number", "description": "Inside height, mm"},
        "board": text("A board of the board profile (the sheet_process query's boards): its thickness, flute and crease deduction"),
        "thickness": {"type": "number", "description": "The board's thickness, mm (needed without a board)"},
        "deduction": {"type": "number", "description": "The measured deduction of a 90 degree crease, mm"},
        "radius": {"type": "number", "description": "The creases' inside radius, mm; an eighth of the thickness when left out"},
        "glue_flap": {"type": "number", "description": "The glue flap's width, mm; 30 when left out"},
        "tuck": {"type": "number", "description": "A tuck end's tuck depth, or a folder lid's tuck or front flap depth below the lid, mm"},
        "tabs": {"type": "integer", "minimum": 0, "maximum": 12, "description": "A folder's locking tabs on each turned over wall; the style's when left out (2; 3 for fefco_0426; 1 for fefco_0470 and fefco_0471)"},
        "flute": text("The board's flute"),
        "folded": flag(true, "Folded; false: the flat blank with its creases"),
        "component": flag(false, "In a new component of its own, activated: a part to save into a library")
    })
}

/// A body by its uid.
fn body(description: &str) -> Value {
    json!({"type": "string", "pattern": "^F(0|[1-9][0-9]*)\\.b(0|[1-9][0-9]*)$", "description": format!("{description} (its uid, F2.b0)")})
}

/// A model-space point or vector.
fn point3(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": description})
}

/// A geometric reference as features take them.
fn reference(description: &str) -> Value {
    json!({
        "type": ["string", "object"],
        "description": format!("{description}: an origin datum (\"xy\", \"xz\", \"yz\", \"x\", \"y\", \"z\"), a construction feature \"F5\", or an object such as {{\"body\", \"face\"}}, {{\"body\", \"edge\"}} or {{\"sketch\", \"curve\"}} (model_reference {{\"section\": \"Geometric references\"}})")
    })
}

/// A check of a print analysis: on or off and its setting.
fn check(setting: &str, setting_schema: Value, description: &str) -> Value {
    let mut properties = json!({"enabled": flag(true, "The check is made")});
    properties[setting] = setting_schema;
    json!({"type": "object", "properties": properties, "additionalProperties": false, "description": description})
}

/// An analysis' definition (`AnalysisDef`), by its `type`.
fn analysis_def() -> Value {
    let positive = |description: &str| json!({"type": "number", "exclusiveMinimum": 0, "description": description});
    let section = json!({
        "type": "object",
        "description": "The bodies cut at a plane: the side the section's normal points to is cut away",
        "properties": {
            "type": {"const": "section"},
            "plane": reference("The plane: an origin plane, a construction plane, a planar face or a fixed plane"),
            "occurrence": text("The plane's occurrence, a path from the root (O1/O4)"),
            "offset": json!({"type": "number", "default": 0, "description": "How far the section is from the plane along its normal, mm"}),
            "flip": flag(false, "The other side is cut away"),
            "angle": json!({"type": "number", "default": 0, "description": "How far the plane is turned about axis before the offset, radians, right-handed"}),
            "axis": reference("The line the plane turns about (a straight edge, a sketch line, a construction or origin axis); needed for an angle unless the plane is a face or an edge with an axis of its own"),
            "axis_occurrence": text("The axis' occurrence, a path from the root")
        },
        "required": ["type", "plane"],
        "additionalProperties": false
    });
    let print = json!({
        "type": "object",
        "description": "Where bodies will print badly and what they will take, printed with the bed face down (or along -Z without one)",
        "properties": {
            "type": {"const": "print"},
            "bodies": {"type": "array", "minItems": 1, "items": {"type": "string"}, "description": "The bodies (uids, F2.b0)"},
            "bed_face": json!({"type": "object", "description": "A planar face of one of the bodies on the bed, {\"body\", \"face\"}; it only sets the direction"}),
            "overhang": check("angle", json!({"type": "number", "minimum": 0, "maximum": 90, "default": 45, "description": "Faces steeper than this from the vertical, degrees"}), "Overhangs"),
            "thin_wall": check("minimum", positive("Walls thinner than this, mm (default 0.8)"), "Thin walls"),
            "min_gap": check("gap", json!({"type": ["number", "null"], "description": "Pairs of the bodies closer than this, mm, which fuse when printed in place; null: the parameter print_in_place_gap, else 0.3"}), "Narrow gaps between the bodies"),
            "estimate": json!({
                "type": "object",
                "description": "The print estimate",
                "properties": {
                    "enabled": flag(true, "The estimate is made"),
                    "material": {"type": ["string", "null"], "description": "A material by its id (pla) or name; null: each body's own, else PLA"},
                    "line_width": positive("mm (default 0.4)"),
                    "walls": {"type": "integer", "minimum": 0, "default": 2},
                    "layer_height": positive("mm (default 0.2)"),
                    "top_bottom_layers": {"type": "integer", "minimum": 0, "default": 4},
                    "infill": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.15, "description": "A fraction"},
                    "filament_diameter": positive("mm (default 1.75)"),
                    "price_per_kg": {"type": ["number", "null"], "description": "Null: no cost"}
                },
                "additionalProperties": false
            })
        },
        "required": ["type", "bodies"],
        "additionalProperties": false
    });
    // Draft Analysis (mitcad#2166).
    let draft = json!({
        "type": "object",
        "description": "The bodies' faces by their draft from a pull direction: enough draft towards the pull, too little either way, enough the other way (the other mold half)",
        "properties": {
            "type": {"const": "draft"},
            "bodies": {"type": "array", "minItems": 1, "items": {"type": "string"}, "description": "The bodies (uids, F2.b0)"},
            "pull": json!({"type": ["string", "object", "null"], "description": "The pull direction: a plane's normal (an origin or construction plane, a planar face) or a line's direction (an origin or construction axis, a straight edge, a sketch line); null: the design's +Z (model_reference {\"section\": \"Geometric references\"})"}),
            "occurrence": text("The pull reference's occurrence, a path from the root (O1/O4)"),
            "flip": flag(false, "The pull direction reversed"),
            "angle": json!({"type": "number", "minimum": 0, "exclusiveMaximum": 90, "default": 1, "description": "The least draft a face needs, degrees"})
        },
        "required": ["type", "bodies"],
        "additionalProperties": false
    });
    // Point Cloud Deviation (mitcad#2240).
    let deviation = json!({
        "type": "object",
        "description": "A point cloud's points by their distance from a body's faces, signed for a closed solid (negative inside the material); the statistics come from the point_cloud_deviation query",
        "properties": {
            "type": {"const": "point_cloud_deviation"},
            "cloud": text("The point cloud, by its name (point_clouds query)"),
            "body": text("The body (uid, F2.b0)"),
            "faces": {"type": "array", "items": {"type": "string"}, "description": "Measured from these faces only (F2:top); every face when left out"},
            "tolerance": positive("A point at most this far from the faces is within tolerance, mm (default 0.1)"),
            "max_samples": {"type": "integer", "minimum": 1, "maximum": 2000000, "default": 100000, "description": "At most this many points are measured: every n-th of a larger cloud"},
            "signed": {"type": ["boolean", "null"], "description": "true asks for the sign (refused for an open body), false measures without it; left out: signed for a closed solid"},
            "limit": positive("The colour scale runs from minus this to this, mm; left out: the largest distance")
        },
        "required": ["type", "cloud", "body"],
        "additionalProperties": false
    });
    json!({
        "type": "object",
        "description": "What the analysis is, by its type: section, print, draft or point_cloud_deviation",
        "oneOf": [section, print, draft, deviation]
    })
}

/// A document command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let parameter =
        || json!({"type": "string", "minLength": 1, "description": "The parameter by its name"});
    let view =
        || json!({"type": "string", "minLength": 1, "description": "The named view by its name"});
    let group = || json!({"type": "string", "minLength": 1, "description": "The timeline group by its name"});
    let analysis =
        || json!({"type": "string", "minLength": 1, "description": "The analysis by its name"});
    let spec = match name {
        "reorder_feature" => json!({
            "description": "Moves a feature to another timeline position; refused when it would come before what it depends on.",
            "properties": {
                "uid": feature_uid("The feature"),
                "index": {"type": "integer", "minimum": 0, "description": "Its new position (the number of features before it)"}
            },
            "required": ["uid", "index"],
            "examples": [{"cmd": "reorder_feature", "uid": "F4", "index": 1}]
        }),
        "rename_body" => json!({
            "description": "Renames a body.",
            "properties": {
                "uid": body("The body"),
                "name": {"type": "string", "minLength": 1, "description": "Its new name"}
            },
            "required": ["uid", "name"],
            "examples": [{"cmd": "rename_body", "uid": "F2.b0", "name": "Lid"}]
        }),
        "set_marker" => json!({
            "description": "Moves the timeline marker: the features from position on are rolled back.",
            "properties": {
                "position": {"type": "integer", "minimum": 0, "description": "The number of features before the marker"},
                "temporary": flag(false, "For a command's preview only: no undo step, no new revision; the marker must be put back before anything is committed or saved")
            },
            "required": ["position"],
            "examples": [{"cmd": "set_marker", "position": 2}]
        }),
        "set_units" => json!({
            "description": "Sets the design's length unit. Result: changed (the parameters whose values changed).",
            "properties": {
                "length": {"type": "string", "minLength": 1, "description": "A length unit: mm, cm, m, in, ft"}
            },
            "required": ["length"],
            "examples": [{"cmd": "set_units", "length": "mm"}]
        }),
        "rename_parameter" => json!({
            "description": "Renames a parameter; the expressions that use it follow.",
            "properties": {
                "name": parameter(),
                "new_name": {"type": "string", "minLength": 1, "description": "Its new name: not a unit, function or constant"}
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_parameter", "name": "w", "new_name": "width"}]
        }),
        "remove_parameter_name" => json!({
            "description": "Gives a named parameter its automatic name back, writing its value into the expressions that used the name. Result: name (the automatic name), rewritten.",
            "properties": {"name": parameter()},
            "required": ["name"],
            "examples": [{"cmd": "remove_parameter_name", "name": "width"}]
        }),
        "delete_parameter" => json!({
            "description": "Deletes a user parameter no expression uses.",
            "properties": {"name": parameter()},
            "required": ["name"],
            "examples": [{"cmd": "delete_parameter", "name": "w"}]
        }),
        "recompute" => json!({
            "description": "Recomputes the design. Result: notices (when an older design's drawings were pinned to what they show); with all, changed (whether it made the undo step Recompute All).",
            "properties": {
                "all": {"type": "boolean", "description": "Every feature evaluated again from the start, without cached or stored results"}
            },
            "required": [],
            "examples": [{"cmd": "recompute"}, {"cmd": "recompute", "all": true}]
        }),
        "set_body_visible" => json!({
            "description": "Shows or hides a body, or makes its visibility follow a condition; one of visible and expression.",
            "properties": {
                "uid": body("The body"),
                "visible": {"type": "boolean", "description": "Shown or hidden"},
                "expression": text("Instead of visible: a condition the visibility follows (Lid = 'Yes'); \"\" removes it")
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}]
        }),
        "merge_undo" => json!({
            "description": "Merges the undo steps made since an undo depth into one step with this label. Result: undo_depth.",
            "properties": {
                "depth": {"type": "integer", "minimum": 0, "description": "The undo depth before the steps (at most the current depth)"},
                "label": {"type": "string", "description": "The merged step's label"}
            },
            "required": ["depth", "label"],
            "examples": [{"cmd": "merge_undo", "depth": 2, "label": "Draw the outline"}],
            "restrictions": ["The live server makes each model_command call one undo step already."]
        }),
        "set_feature_visible" => json!({
            "description": "The light bulb of a sketch or a construction feature.",
            "properties": {
                "uid": feature_uid("The sketch or construction feature"),
                "visible": {"type": "boolean", "description": "Shown or hidden"}
            },
            "required": ["uid", "visible"],
            "examples": [{"cmd": "set_feature_visible", "uid": "F1", "visible": false}]
        }),
        "set_datum_display_size" => json!({
            "description": "How large a construction plane (its square's side) or axis (its line) is drawn; only the drawing changes.",
            "properties": {
                "uid": feature_uid("The construction plane or axis"),
                "size": {"type": ["number", "null"], "exclusiveMinimum": 0, "description": "mm; null or left out: the default size"}
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_datum_display_size", "uid": "F3", "size": 80}]
        }),
        "acknowledge_warnings" => json!({
            "description": "Marks features' warnings as seen (the timeline's acknowledged), or shows them again; lapses when a warning changes. Failed features are refused.",
            "properties": {
                "uids": {"type": "array", "items": {"type": "string", "pattern": "^F(0|[1-9][0-9]*)$"}, "description": "The features with warnings (uids)"},
                "acknowledged": flag(true, "Acknowledged; false shows them again")
            },
            "required": ["uids"],
            "examples": [{"cmd": "acknowledge_warnings", "uids": ["F4"]},
                         {"cmd": "acknowledge_warnings", "uids": ["F4"], "acknowledged": false}]
        }),
        "add_named_view" => json!({
            "description": "Saves a camera as a named view. Result: name.",
            "properties": {
                "name": text("Its name; the next free NamedView<n> when left out"),
                "eye": point3("Where the camera is, mm"),
                "target": point3("What it looks at, mm"),
                "up": point3("Its up direction"),
                "perspective": flag(false, "A perspective view (else orthographic)"),
                "height": {"type": "number", "exclusiveMinimum": 0, "description": "The height the view shows, mm"},
                "replace": flag(false, "Replace a view of the same name instead of refusing")
            },
            "required": ["eye", "target", "up", "height"],
            "examples": [{"cmd": "add_named_view", "name": "Front", "eye": [0, -200, 0], "target": [0, 0, 0], "up": [0, 0, 1], "height": 120}]
        }),
        "delete_named_view" => json!({
            "description": "Deletes a named view.",
            "properties": {"name": view()},
            "required": ["name"],
            "examples": [{"cmd": "delete_named_view", "name": "Front"}]
        }),
        "rename_named_view" => json!({
            "description": "Renames a named view.",
            "properties": {
                "name": view(),
                "new_name": {"type": "string", "minLength": 1, "description": "Its new name"}
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_named_view", "name": "Front", "new_name": "Front view"}]
        }),
        "set_origin_visible" => json!({
            "description": "Shows or hides the root's origin planes, axes and point.",
            "properties": {
                "visible": {"type": "boolean", "description": "Shown or hidden"},
                "opening": flag(false, "As a new design opens: no undo step, no new revision")
            },
            "required": ["visible"],
            "examples": [{"cmd": "set_origin_visible", "visible": true}]
        }),
        "set_isolation" => json!({
            "description": "Isolates the view to bodies and occurrences; none (an empty list) shows everything again.",
            "properties": {
                "items": {"type": "array", "default": [], "items": {
                    "type": "object",
                    "properties": {
                        "body": body("A body"),
                        "occurrence": text("The occurrence that places the body (\"\" for the root's), or without body an occurrence by its path from the root (O1/O4)")
                    },
                    "additionalProperties": false
                }, "description": "What stays shown"}
            },
            "required": [],
            "examples": [{"cmd": "set_isolation", "items": [{"body": "F2.b0"}, {"occurrence": "O1"}]}, {"cmd": "set_isolation", "items": []}]
        }),
        "add_center_of_mass" => json!({
            "description": "Adds the Center of Mass marker under Origin.",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "add_center_of_mass"}]
        }),
        "set_center_of_mass_visible" => json!({
            "description": "Shows or hides the Center of Mass marker.",
            "properties": {"visible": {"type": "boolean", "description": "Shown or hidden"}},
            "required": ["visible"],
            "examples": [{"cmd": "set_center_of_mass_visible", "visible": false}]
        }),
        "delete_center_of_mass" => json!({
            "description": "Deletes the Center of Mass marker.",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "delete_center_of_mass"}]
        }),
        "set_view_show" => json!({
            "description": "View > Show of the whole design: shows or hides every origin, construction feature or sketch whatever their own light bulbs say; a kind left out stays as it is.",
            "properties": {
                "origins": {"type": "boolean", "description": "The design's origin and the joint origins"},
                "construction": {"type": "boolean", "description": "Construction planes, axes and points"},
                "sketches": {"type": "boolean", "description": "Sketches"}
            },
            "required": [],
            "examples": [{"cmd": "set_view_show", "sketches": false}]
        }),
        "set_folder_visible" => json!({
            "description": "The eye of a component's Bodies, Sketches or Construction folder, or of the root's Annotations folder: a hidden folder hides all it holds.",
            "properties": {
                "component": id("The component by uid or name; the root's when left out (the only one with annotations)"),
                "folder": choice(&["bodies", "sketches", "construction", "annotations"], "The folder"),
                "visible": {"type": "boolean", "description": "Shown or hidden"}
            },
            "required": ["folder", "visible"],
            "examples": [{"cmd": "set_folder_visible", "folder": "sketches", "visible": false}]
        }),
        "set_sketch_dimensions_shown" => json!({
            "description": "Show Dimensions of a sketch: its dimensions drawn while it is shown and not edited.",
            "properties": {
                "sketch": feature_uid("The sketch"),
                "shown": {"type": "boolean", "description": "Drawn or not"}
            },
            "required": ["sketch", "shown"],
            "examples": [{"cmd": "set_sketch_dimensions_shown", "sketch": "F1", "shown": true}]
        }),
        "set_body_transparent" => json!({
            "description": "Draws a body see-through or opaque.",
            "properties": {
                "uid": body("The body"),
                "transparent": {"type": "boolean", "description": "See-through or opaque"}
            },
            "required": ["uid", "transparent"],
            "examples": [{"cmd": "set_body_transparent", "uid": "F2.b0", "transparent": true}]
        }),
        "group_features" => json!({
            "description": "Groups a run of features on the timeline. Result: name.",
            "properties": {
                "features": {"type": "array", "minItems": 1, "items": {"type": "string", "pattern": "^F(0|[1-9][0-9]*)$"}, "description": "The features (uids, in any order), next to each other on the timeline"},
                "name": text("The group's name; the next free Group<n> when left out")
            },
            "required": ["features"],
            "examples": [{"cmd": "group_features", "features": ["F2", "F3"], "name": "Base"}]
        }),
        "ungroup" => json!({
            "description": "Takes a timeline group away; its features stay.",
            "properties": {"name": group()},
            "required": ["name"],
            "examples": [{"cmd": "ungroup", "name": "Base"}]
        }),
        "rename_group" => json!({
            "description": "Renames a timeline group.",
            "properties": {
                "name": group(),
                "new_name": {"type": "string", "minLength": 1, "description": "Its new name"}
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_group", "name": "Base", "new_name": "Housing"}]
        }),
        "add_analysis" => json!({
            "description": "Adds a section, print, draft or point cloud deviation analysis, shown (the others hidden); no geometry changes. Result: name.",
            "properties": {
                "def": analysis_def(),
                "name": text("Its name; the next free Section<n>, Print<n>, Draft<n> or Deviation<n> when left out")
            },
            "required": ["def"],
            "examples": [
                {"cmd": "add_analysis", "def": {"type": "section", "plane": "xz", "offset": 10}},
                {"cmd": "add_analysis", "name": "Print1", "def": {"type": "print", "bodies": ["F2.b0"], "overhang": {"angle": 50}}},
                {"cmd": "add_analysis", "def": {"type": "draft", "bodies": ["F2.b0"], "pull": "xy", "angle": 2}}
            ]
        }),
        "edit_analysis" => json!({
            "description": "Replaces an analysis' definition (section, print or draft).",
            "properties": {
                "name": analysis(),
                "def": analysis_def(),
                "visible": {"type": "boolean", "description": "Shows it (hiding the others) or hides it; as it was when left out"}
            },
            "required": ["name", "def"],
            "examples": [{"cmd": "edit_analysis", "name": "Section1", "def": {"type": "section", "plane": "xz", "offset": 20, "flip": true}}]
        }),
        "set_analysis_visible" => json!({
            "description": "Shows an analysis (hiding the others) or hides it.",
            "properties": {
                "name": analysis(),
                "visible": {"type": "boolean", "description": "Shown or hidden"}
            },
            "required": ["name", "visible"],
            "examples": [{"cmd": "set_analysis_visible", "name": "Section1", "visible": false}]
        }),
        "rename_analysis" => json!({
            "description": "Renames an analysis.",
            "properties": {
                "name": analysis(),
                "new_name": {"type": "string", "minLength": 1, "description": "Its new name"}
            },
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_analysis", "name": "Section1", "new_name": "Mid section"}]
        }),
        "delete_analysis" => json!({
            "description": "Deletes an analysis.",
            "properties": {"name": analysis()},
            "required": ["name"],
            "examples": [{"cmd": "delete_analysis", "name": "Section1"}]
        }),
        "set_flat_pattern" => json!({
            "description": "Creates or changes a sheet-metal body's Flat Pattern; a field left out keeps its setting (a new one needs stationary).",
            "properties": {
                "body": body("The sheet-metal body"),
                "stationary": {"type": "object", "properties": {
                    "feature": {"type": "string", "description": "The sheet feature (F3)"},
                    "slot": {"type": "integer", "minimum": 0, "description": "The panel's number in it"}
                }, "required": ["feature", "slot"], "description": "The panel that stays fixed and gives the 2D frame (as the sheet query names panels)"},
                "x_axis": {"type": "object", "properties": {"type": {"const": "panel_x"}}, "required": ["type"], "additionalProperties": false, "description": "The 2D x axis: the fixed panel's own material x axis"},
                "reference_side": {"type": "string", "enum": ["front", "back"], "default": "front", "description": "The material side the drawing looks at; back mirrors it"},
                "shown": {"type": "boolean", "description": "The view shows the developed solid in place of the body"}
            },
            "required": ["body"],
            "examples": [{"cmd": "set_flat_pattern", "body": "F3.b0", "stationary": {"feature": "F3", "slot": 0}, "reference_side": "front"}]
        }),
        "remove_flat_pattern" => json!({
            "description": "Removes a body's Flat Pattern.",
            "properties": {"body": body("The sheet-metal body")},
            "required": ["body"],
            "examples": [{"cmd": "remove_flat_pattern", "body": "F3.b0"}]
        }),
        "set_print_bed_face" => json!({
            "description": "Sets the planar face of a body that lies on the print bed, or takes it away.",
            "properties": {
                "uid": body("The body"),
                "face": {"type": ["string", "null"], "description": "A planar face of the body at the marker by its topological name; null or left out takes it away"}
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_print_bed_face", "uid": "F2.b0", "face": "F1:top"}]
        }),
        "sheet_flange_profile" => json!({
            "description": "Makes a flange's profile sketch on the wall where the flange puts it (a flange_wall construction plane and a sketch on it, at the marker, one undo step), or moves the plane of the profile the definition names onto the wall. Result: sketch, plane, added.",
            "properties": {"def": {}},
            "required": ["def"],
            "examples": [{"cmd": "sheet_flange_profile",
                "def": {"type": "sheet_flange", "body": "F3.b0",
                        "boundary": {"panel": {"feature": "F3", "slot": 0},
                                     "curve": {"type": "sketch", "sketch": "F2", "segment": "c2[c1,c3]"}},
                        "angle": 1.5, "length": {"mode": "tangent", "value": 15},
                        "position": "inside"}}],
            "restrictions": ["def is a sheet_flange definition (model_reference {\"feature\": \"sheet_flange\"})."]
        }),
        "add_flat_pack" => json!({
            "description": "Turns a box body into the six finger-jointed plates of a laser-cut box (a flat_pack feature) and lays them out on a new cut sheet, nested, in one undo step (mitcad#1939). Result: uid, bodies, drawing, warnings.",
            "properties": {
                "body": body("The box: a rectangular block along the component's axes"),
                "thickness": {"type": ["number", "string"], "description": "The sheet's thickness, mm (or an expression)"},
                "inside": flag(false, "The box is the room inside the plates"),
                "finger_width": {"type": ["number", "string"], "description": "The fingers' width, mm; 2.5 times the thickness when left out"},
                "clearance": {"type": ["number", "string"], "description": "Each finger's clearance in its slot on each side, mm; 0.1 when left out"},
                "width": {"type": "number", "description": "The cut sheet's width, mm"},
                "height": {"type": "number", "description": "The cut sheet's height, mm"},
                "kerf": {"type": "number", "description": "The cut sheet's kerf, mm (0 to 10)"}
            },
            "required": ["body", "thickness"],
            "examples": [{"cmd": "add_flat_pack", "body": "F1.b0", "thickness": 6, "finger_width": 15, "width": 1000, "height": 600}]
        }),
        "add_standard_box" => json!({
            "description": "Makes a board box of a standard style from its options: board rules, its blank and creases as sketches, a Face and a Fold at every crease, and its Flat Pattern, in one undo step (mitcad#1939). Result: rules, outline, creases, face, body, folds, component.",
            "properties": standard_box_fields(),
            "required": ["style", "length", "width", "height"],
            "examples": [{"cmd": "add_standard_box", "style": "fefco_0201", "length": 300, "width": 200, "height": 150, "board": "c_flute"}]
        }),
        "edit_standard_box" => json!({
            "description": "Changes a standard box's options: its rules, blank and creases are made again and its Face and Folds follow, in one undo step; null takes an option away (mitcad#1939). Result: rules, outline, creases, face, folds.",
            "properties": {
                "rules": feature_uid("The box's sheet rules"),
                "style": choice(BOX_STYLES, "Its style; another style is refused"),
                "length": number_or_null("Inside length, mm"),
                "width": number_or_null("Inside width, mm"),
                "height": number_or_null("Inside height, mm"),
                "board": {"type": ["string", "null"], "description": "A board of the board profile (the sheet_process query's boards)"},
                "thickness": number_or_null("The board's thickness, mm"),
                "deduction": number_or_null("The measured deduction of a 90 degree crease, mm"),
                "radius": number_or_null("The creases' inside radius, mm"),
                "glue_flap": number_or_null("The glue flap's width, mm"),
                "tuck": number_or_null("A tuck end's tuck depth, or a folder lid's tuck or front flap depth, mm"),
                "tabs": {"type": ["integer", "null"], "minimum": 0, "maximum": 12, "description": "A folder's locking tabs on each turned over wall: the blank and creases are made again with them and the Face and Folds follow (mitcad#2285)"},
                "flute": {"type": ["string", "null"], "description": "The board's flute"}
            },
            "required": ["rules"],
            "examples": [{"cmd": "edit_standard_box", "rules": "F1", "height": 200}]
        }),
        "add_glue_flap" => json!({
            "description": "Adds a glue flap: a flange whose wall is a trapezoid, its ends slanting in by the taper, made from the flange's profile sketch, in one undo step (mitcad#1939). Result: uid, profile.",
            "properties": {
                "def": {},
                "taper": {"type": "number", "minimum": 0, "maximum": 60, "description": "How far each end slants in, degrees; 15 when left out"},
                "inset": {"type": "number", "minimum": 0, "description": "How far the flap keeps in from each end of the edge, mm; 0 when left out"}
            },
            "required": ["def"],
            "examples": [{"cmd": "add_glue_flap", "taper": 15,
                "def": {"type": "sheet_flange", "body": "F3.b0",
                        "boundary": {"panel": {"feature": "F3", "slot": 0},
                                     "curve": {"type": "sketch", "sketch": "F2", "segment": "c6[c5,c7]"}},
                        "angle": 1.5, "length": {"mode": "tangent", "value": 30},
                        "position": "inside"}}],
            "restrictions": ["def is a sheet_flange definition (model_reference {\"feature\": \"sheet_flange\"})."]
        }),
        "set_body_folder" => json!({
            "description": "Puts a body in a folder of its component's Bodies folder, or back in the Bodies folder itself.",
            "properties": {
                "uid": body("The body"),
                "folder": {"type": ["string", "null"], "description": "The folder's name; null or left out: the Bodies folder itself"}
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_body_folder", "uid": "F2.b0", "folder": "Fasteners"}]
        }),
        // Sketch blocks (mitcad#2110).
        "add_sketch_block" => json!({
            "description": "Create Block: a sketch's points and curves become a named block of the design and an instance of it in place (keeping their ids); the block's own constraints and dimensions leave the sketch. Projected geometry and texts are refused. Result: the sketch edit's fields, block, name, sketch, instance.",
            "properties": {
                "sketch": feature_uid("The sketch (a planar sketch's uid)"),
                "items": {"type": "array", "items": {"type": "string", "pattern": "^[pc][0-9]+$"}, "description": "Its points and curves (p3, c4)"},
                "name": text("Its name; Block<n> when left out"),
                "base": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "Its base point, [x, y] in the sketch; the first item's first point when left out"},
                "angle": {"type": "number", "default": 0, "description": "Its base direction from the sketch's x axis, radians"}
            },
            "required": ["sketch", "items"],
            "examples": [{"cmd": "add_sketch_block", "sketch": "F1", "items": ["c1", "c2", "c3", "c4"], "name": "Slot", "base": [10, 0]}]
        }),
        "rename_sketch_block" => json!({
            "description": "Renames a sketch block; a name another block has is refused.",
            "properties": {"block": id("The block, by its name or its number"), "name": text("Its new name")},
            "required": ["block", "name"],
            "examples": [{"cmd": "rename_sketch_block", "block": "Slot", "name": "Long slot"}]
        }),
        "remove_sketch_block" => json!({
            "description": "Deletes a sketch block; refused while it has instances in sketches or in other blocks.",
            "properties": {"block": id("The block, by its name or its number")},
            "required": ["block"],
            "examples": [{"cmd": "remove_sketch_block", "block": "Slot"}]
        }),
        "edit_sketch_block" => json!({
            "description": "Edit Block: opens a block for an edit, in place at an instance (sketch and instance: its geometry free with the block's constraints and dimensions) or, without them, in a sketch made for the edit (its base point at the origin). What the sketch editing commands make there joins the block; finish_sketch_block ends the edit. One block is open at a time. Result: sketch, instance.",
            "properties": {
                "block": id("The block, by its name or its number"),
                "sketch": feature_uid("The sketch of the instance to edit it at"),
                "instance": {"type": "string", "pattern": "^k[0-9]+$", "description": "The instance (k<n>)"}
            },
            "required": ["block"],
            "examples": [{"cmd": "edit_sketch_block", "block": "Slot", "sketch": "F1", "instance": "k6"},
                         {"cmd": "edit_sketch_block", "block": "Slot"}]
        }),
        "finish_sketch_block" => json!({
            "description": "Finish Block: the open block's geometry becomes the block and every instance follows (constraints on removed geometry are reported in warnings), or with cancel the block stays as it was. A sketch made for the edit goes.",
            "properties": {"cancel": flag(false, "Leave the block as it was")},
            "required": [],
            "examples": [{"cmd": "finish_sketch_block"}]
        }),
        // Selection sets (mitcad#2192): links to items, nothing recomputed.
        "add_selection_set" => json!({
            "description": "Adds a named selection set: links to bodies, components and features (sketches, construction geometry, joints) that leave them where they are. Changes no geometry, timeline or bill of materials. A name another set has is refused. Result: name, members (the count).",
            "properties": {
                "name": text("Its name; the next free SelectionSet<n> when left out"),
                "members": set_members("Its members, each once")
            },
            "required": [],
            "examples": [{"cmd": "add_selection_set", "name": "Favourites", "members": [{"body": "F2.b0"}, {"feature": "F1"}]}]
        }),
        "set_selection_set_members" => json!({
            "description": "Changes a selection set's members: members replaces them all, then add adds those not in it yet and remove takes those out. Result: name, members (the count).",
            "properties": {
                "name": text("The set"),
                "members": set_members("All its members; left out: those it has"),
                "add": set_members("Members to add"),
                "remove": set_members("Members to take out")
            },
            "required": ["name"],
            "examples": [{"cmd": "set_selection_set_members", "name": "Favourites", "add": [{"occurrence": "O1"}]},
                         {"cmd": "set_selection_set_members", "name": "Favourites", "remove": [{"body": "F2.b0"}]}]
        }),
        "rename_selection_set" => json!({
            "description": "Renames a selection set; a name another set has is refused. Result: name.",
            "properties": {"name": text("The set"), "new_name": text("Its new name")},
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_selection_set", "name": "SelectionSet1", "new_name": "Favourites"}]
        }),
        "delete_selection_set" => json!({
            "description": "Deletes a selection set; its items stay.",
            "properties": {"name": text("The set")},
            "required": ["name"],
            "examples": [{"cmd": "delete_selection_set", "name": "Favourites"}]
        }),
        _ => return None,
    };
    Some(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_here_has_its_spec() {
        for name in COMMANDS {
            assert!(spec(name).is_some(), "{name}");
        }
        let add = spec("add_analysis").unwrap();
        assert_eq!(
            add["properties"]["def"]["oneOf"][0]["properties"]["type"]["const"],
            "section"
        );
    }
}
