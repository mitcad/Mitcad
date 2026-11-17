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
];

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
    json!({
        "type": "object",
        "description": "What the analysis is, by its type: section or print",
        "oneOf": [section, print]
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
            "description": "Recomputes the design. Result: notices (when an older design's drawings were pinned to what they show).",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "recompute"}]
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
            "description": "The eye of a component's Bodies, Sketches or Construction folder: a hidden folder hides all it holds.",
            "properties": {
                "component": id("The component by uid or name; the root's when left out"),
                "folder": choice(&["bodies", "sketches", "construction"], "The folder"),
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
            "description": "Adds a section or print analysis, shown (the others hidden); no geometry changes. Result: name.",
            "properties": {
                "def": analysis_def(),
                "name": text("Its name; the next free Section<n> or Print<n> when left out")
            },
            "required": ["def"],
            "examples": [
                {"cmd": "add_analysis", "def": {"type": "section", "plane": "xz", "offset": 10}},
                {"cmd": "add_analysis", "name": "Print1", "def": {"type": "print", "bodies": ["F2.b0"], "overhang": {"angle": 50}}}
            ]
        }),
        "edit_analysis" => json!({
            "description": "Replaces an analysis' definition (section or print).",
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
        "set_body_folder" => json!({
            "description": "Puts a body in a folder of its component's Bodies folder, or back in the Bodies folder itself.",
            "properties": {
                "uid": body("The body"),
                "folder": {"type": ["string", "null"], "description": "The folder's name; null or left out: the Bodies folder itself"}
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_body_folder", "uid": "F2.b0", "folder": "Fasteners"}]
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
