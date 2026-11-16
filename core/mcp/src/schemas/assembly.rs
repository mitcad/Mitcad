// SPDX-License-Identifier: MIT
//! The JSON Schemas of the component and occurrence commands (mitcad#1918):
//! components and occurrences (`core/model-api/src/components.rs`), Move
//! Features into a Component and Body Transfer (`move_features.rs`), the
//! design policy (`policy.rs`), Reorient Design (`reorient.rs`) and Convert
//! to Features (`convert_derived.rs`), written from their fields as
//! [`super`] writes the others. The drift test
//! `core/model-tests/api/src/mcp_schema_tests.rs` checks them against the
//! API like the others.

use serde_json::{Value, json};

use super::{choice, feature_uid, flag, id, ids, number, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "create_component",
    "components_from_bodies",
    "activate_component",
    "rename_component",
    "ground_occurrence",
    "set_occurrence_visible",
    "set_occurrence_transform",
    "copy_occurrence",
    "paste_new",
    "delete_occurrence",
    "reparent_occurrence",
    "set_component_parameters",
    "set_occurrence_transparent",
    "find_repeated_parts",
    "set_occurrence_flexible",
    "move_to_component",
    "move_history_into_component",
    "set_hybrid_assembly",
    "transfer_bodies",
    "set_policy",
    "reorient_design",
    "convert_derived_part",
];

/// The design policy's settings (`Setting::key`).
const POLICY_SETTINGS: &str = "sketches, features, components, attach_to_others, attachments_from_others, first_sketch_attaches, first_sketch_attachments_from_others, reference_others, references_from_others";

/// A placement (`TransformInput`): rows of a rotation and a translation, or
/// a translation with an optional rotation about an axis through the
/// origin (applied first).
fn transform(description: &str) -> Value {
    json!({
        "description": format!("{description}: 3 or 4 rows of 4 numbers (a rotation and a translation in mm), or {{\"translation\": [x, y, z] (mm, default 0), \"rotation\": {{\"axis\": [x, y, z], \"angle\": radians, right-handed}}}} (the rotation about an axis through the origin, applied first)"),
        "oneOf": [
            {"type": "array", "minItems": 3, "maxItems": 4,
             "items": {"type": "array", "minItems": 4, "maxItems": 4, "items": {"type": "number"}}},
            {"type": "object",
             "properties": {
                 "translation": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}},
                 "rotation": {"type": "object",
                              "properties": {
                                  "axis": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}},
                                  "angle": {"type": "number"}
                              },
                              "required": ["axis", "angle"],
                              "additionalProperties": false}
             },
             "additionalProperties": false}
        ]
    })
}

fn occurrence() -> Value {
    id("The occurrence: its uid (O3), name (Bracket:1) or path from the root (O1/O4)")
}

fn component() -> Value {
    id("The component: its uid (C2; C0 the root) or name")
}

/// An occurrence path from the root.
fn path(description: &str) -> Value {
    json!({"type": "string", "description": format!("{description}: occurrence ids from the root, \"O1/O4\"; \"\" the root itself")})
}

fn bodies(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": format!("{description} (body uids, F2.b0)")})
}

/// Where Move Features into a Component puts what it moves
/// (`MoveDestination`).
fn destination() -> Value {
    json!({
        "description": "An explicit destination: an existing component seen at an occurrence path, or a new component under a parent seen at its occurrence path",
        "oneOf": [
            {"type": "object",
             "properties": {
                 "kind": {"const": "existing"},
                 "component": {"type": "string", "pattern": "^C[0-9]+$"},
                 "occurrence": {"type": "string"}
             },
             "required": ["kind", "component", "occurrence"],
             "additionalProperties": false},
            {"type": "object",
             "properties": {
                 "kind": {"const": "new"},
                 "parent": {"type": "string", "pattern": "^C[0-9]+$"},
                 "parent_occurrence": {"type": "string"},
                 "name": {"type": "string"}
             },
             "required": ["kind", "parent", "parent_occurrence"],
             "additionalProperties": false}
        ]
    })
}

/// A component and occurrence command's schema (fields without `cmd`).
fn components(name: &str) -> Option<Value> {
    let spec = match name {
        "create_component" => json!({
            "description": "Makes a new empty component with one occurrence in the active component. Result: component, occurrence, name; placed_in, instead_of and placement_warning when it went beside an active component that takes no components.",
            "properties": {
                "name": text("Its name; default the next Component<n>"),
                "transform": transform("Where its occurrence is placed; default where it is"),
                "activate": flag(true, "Activate the new component"),
                "policy": {"type": ["string", "object"], "description": format!("Its design policy: a preset (hybrid, part or assembly) or an object of settings (true or false) over the general defaults: {POLICY_SETTINGS}; the general defaults when left out")}
            },
            "required": [],
            "examples": [{"cmd": "create_component", "name": "Bracket"},
                         {"cmd": "create_component", "name": "Arm", "transform": {"translation": [50, 0, 0]}, "activate": false, "policy": "part"}]
        }),
        "components_from_bodies" => json!({
            "description": "Makes a component of each body, the body moved into it. Result: components (feature, component, name each).",
            "properties": {
                "bodies": bodies("The bodies")
            },
            "required": ["bodies"],
            "examples": [{"cmd": "components_from_bodies", "bodies": ["F2.b0", "F5.b0"]}]
        }),
        "activate_component" => json!({
            "description": "Makes a component the active one, which new features go into. Result: component; with opening also previous.",
            "properties": {
                "component": component(),
                "opening": flag(false, "As the document opens: no undo step, no new revision, nothing recomputed")
            },
            "required": ["component"],
            "examples": [{"cmd": "activate_component", "component": "C2"}]
        }),
        "rename_component" => json!({
            "description": "Renames a component; its only body, default-named, is renamed with it. Result: renamed_body (uid, name) or null.",
            "properties": {
                "component": component(),
                "name": {"type": "string", "minLength": 1, "description": "Its new name (unique)"}
            },
            "required": ["component", "name"],
            "examples": [{"cmd": "rename_component", "component": "C2", "name": "Bracket"}]
        }),
        "ground_occurrence" => json!({
            "description": "Grounds an occurrence where it is (joints and drags do not move it), or frees it.",
            "properties": {
                "occurrence": occurrence(),
                "grounded": flag(true, "true grounds it, false frees it")
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "ground_occurrence", "occurrence": "O1"}, {"cmd": "ground_occurrence", "occurrence": "Bracket:1", "grounded": false}]
        }),
        "set_occurrence_visible" => json!({
            "description": "Shows or hides an occurrence, or makes its visibility follow a condition.",
            "properties": {
                "occurrence": occurrence(),
                "visible": {"type": "boolean", "description": "Shown or hidden"},
                "expression": text("Instead of visible: a condition it is shown while it holds (Holes = 'Yes'); \"\" removes it")
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "set_occurrence_visible", "occurrence": "O1", "visible": false},
                         {"cmd": "set_occurrence_visible", "occurrence": "O1", "expression": "w > 40 mm"}],
            "restrictions": ["One of visible and expression, not both."]
        }),
        "set_occurrence_transform" => json!({
            "description": "Places an occurrence. Result: feature (the position captured on the timeline, or null).",
            "properties": {
                "occurrence": occurrence(),
                "transform": transform("Its placement"),
                "capture": flag(false, "Capture the position as a feature on the timeline")
            },
            "required": ["occurrence", "transform"],
            "examples": [{"cmd": "set_occurrence_transform", "occurrence": "O1", "transform": {"translation": [0, 0, 20], "rotation": {"axis": [0, 0, 1], "angle": 1.5}}}]
        }),
        "copy_occurrence" => json!({
            "description": "Another occurrence of the same component (a linked copy). Result: occurrence, name.",
            "properties": {
                "occurrence": occurrence(),
                "transform": transform("Where the copy is placed; default where the original is")
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "copy_occurrence", "occurrence": "O1", "transform": {"translation": [40, 0, 0]}}]
        }),
        "paste_new" => json!({
            "description": "An independent copy of an occurrence's component (Paste New) as a new component with one occurrence. Result: component, occurrence, name.",
            "properties": {
                "occurrence": occurrence(),
                "transform": transform("Where the copy is placed; default where the original is")
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "paste_new", "occurrence": "Bracket:1"}]
        }),
        "delete_occurrence" => json!({
            "description": "Deletes an occurrence (with its component when it was the last). Result: deleted (features); notices for the cut-scope exclusions it dropped.",
            "properties": {
                "occurrence": occurrence()
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "delete_occurrence", "occurrence": "O2"}]
        }),
        "reparent_occurrence" => json!({
            "description": "Moves an occurrence (and others of the same parent) under another parent, staying where it is in space. Result: as model_reference {\"section\": \"Moving an occurrence to another parent\"} says.",
            "properties": {
                "occurrence": path("The occurrence"),
                "parent": {"type": "string", "default": "", "description": "The new parent's occurrence path from the root (\"O1/O4\"); \"\" or left out: the root"},
                "with": {"type": "array", "items": {"type": "string"}, "default": [], "description": "More occurrences of the same parent that go along, by their paths from the root"}
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "reparent_occurrence", "occurrence": "O3", "parent": "O1"},
                         {"cmd": "reparent_occurrence", "occurrence": "O1/O4", "with": ["O1/O5"]}]
        }),
        "set_component_parameters" => json!({
            "description": "Sets an inserted (library or linked) component's options. Result: component, name, part, moves, refused and its options as the library_parts query gives them.",
            "properties": {
                "component": component(),
                "parameters": {"type": "object", "default": {}, "additionalProperties": {"type": ["number", "string", "null"]}, "description": "{option: an expression of this design's parameters or a number}; null removes the component's own value, so the configuration row's holds"},
                "config": text("Another configuration row to start from (not for a BUILD part: build_edit sets its row)")
            },
            "required": ["component"],
            "examples": [{"cmd": "set_component_parameters", "component": "Shelf board", "parameters": {"length": "600 mm", "holes": null}}]
        }),
        "set_occurrence_transparent" => json!({
            "description": "Draws an occurrence, with what it contains, see-through or opaque.",
            "properties": {
                "occurrence": occurrence(),
                "transparent": {"type": "boolean", "description": "true see-through, false opaque"}
            },
            "required": ["occurrence", "transparent"],
            "examples": [{"cmd": "set_occurrence_transparent", "occurrence": "Bracket:1", "transparent": true}]
        }),
        "find_repeated_parts" => json!({
            "description": "Find Repeated Parts: a base feature's copies of one body become occurrences of one component. Result: components (component, name, feature, body, occurrences, copies each), kept.",
            "properties": {
                "feature": feature_uid("The base feature"),
                "tolerance": number("How far copies may differ, mm; default the design's join tolerance, else 0.01 mm")
            },
            "required": ["feature"],
            "examples": [{"cmd": "find_repeated_parts", "feature": "F1"}, {"cmd": "find_repeated_parts", "feature": "F1", "tolerance": 0.05}]
        }),
        "set_occurrence_flexible" => json!({
            "description": "Makes an occurrence flexible (its component's joints move in its parent for it alone) or rigid. Result: flexible.",
            "properties": {
                "occurrence": occurrence(),
                "flexible": flag(true, "true flexible, false rigid")
            },
            "required": ["occurrence"],
            "examples": [{"cmd": "set_occurrence_flexible", "occurrence": "Hinge:1"}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A move, policy, reorientation or conversion command's schema.
fn others(name: &str) -> Option<Value> {
    let spec = match name {
        "move_to_component" => json!({
            "description": "Move Features into a Component: bodies (with the features that make them) or features go into one new component, or each group into its own. Result: components (component, occurrence, name, bodies, features each; created with an explicit destination). The move_to_component query previews it.",
            "properties": {
                "component": component(),
                "bodies": bodies("The bodies to move"),
                "features": ids("Instead of bodies: the features to move (F3)"),
                "groups": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}, "description": "Instead: groups of bodies, a component of each"},
                "name": text("The new component's name (one component)"),
                "names": ids("The components' names, one per group"),
                "together": flag(false, "Bodies a feature ties go into one component"),
                "source_occurrence": path("Where the bodies are seen; default the first path to their component"),
                "destination": destination()
            },
            "required": [],
            "examples": [{"cmd": "move_to_component", "bodies": ["F2.b0"], "name": "Bracket"},
                         {"cmd": "move_to_component", "groups": [["F2.b0"], ["F4.b0"]], "names": ["Base", "Lid"]},
                         {"cmd": "move_to_component", "bodies": ["F2.b0"], "destination": {"kind": "new", "parent": "C1", "parent_occurrence": "O1", "name": "Pin"}}]
        }),
        "move_history_into_component" => json!({
            "description": "Moves a base feature's history into the component its bodies are in. Result: component, name, features.",
            "properties": {
                "feature": feature_uid("The feature")
            },
            "required": ["feature"],
            "examples": [{"cmd": "move_history_into_component", "feature": "F3"}]
        }),
        "set_hybrid_assembly" => json!({
            "description": "Allows components beside modelled bodies in the root (a hybrid assembly) without asking again, or asks again.",
            "properties": {
                "allowed": flag(true, "Allowed")
            },
            "required": [],
            "examples": [{"cmd": "set_hybrid_assembly"}, {"cmd": "set_hybrid_assembly", "allowed": false}]
        }),
        "transfer_bodies" => json!({
            "description": "Body Transfer: moves bodies into an existing component on the timeline. Result: uid, name (the transfer feature), component, component_name, bodies. The transfer_bodies query lists the components they can go into.",
            "properties": {
                "bodies": bodies("The bodies"),
                "source_occurrence": path("Where they are seen; default the first path to their component"),
                "destination": {"type": "object",
                                "properties": {
                                    "component": {"type": "string", "description": "The component (C2) or its name"},
                                    "occurrence": {"type": "string", "description": "Where it is seen: an occurrence path from the root; default its first path"}
                                },
                                "required": ["component"],
                                "additionalProperties": false,
                                "description": "The component they go into"},
                "name": text("The transfer feature's name")
            },
            "required": ["bodies", "destination"],
            "examples": [{"cmd": "transfer_bodies", "bodies": ["F2.b0"], "destination": {"component": "C2", "occurrence": "O1"}}]
        }),
        "set_policy" => json!({
            "description": "Sets a component's design policy: a preset first, then settings changed (the others stay). Result: component, policy.",
            "properties": {
                "component": component(),
                "preset": choice(&["hybrid", "defaults", "part", "assembly"], "A preset applied first (defaults is hybrid)"),
                "policy": {"type": "object", "additionalProperties": {"type": "boolean"}, "description": format!("Settings to change, each true or false: {POLICY_SETTINGS}")}
            },
            "required": [],
            "examples": [{"cmd": "set_policy", "preset": "part"},
                         {"cmd": "set_policy", "component": "C0", "policy": {"components": true, "reference_others": false}}]
        }),
        "reorient_design" => json!({
            "description": "Reorient Design: turns the whole design about an origin axis, or so that an origin plane's frame becomes XY's. Result: turn, matrix, features, moves, occurrences, views, analyses, lights.",
            "properties": {
                "axis": choice(&["x", "y", "z"], "The axis, with degrees"),
                "degrees": {"type": "integer", "enum": [90, 180, 270, -90], "description": "Counter-clockwise looking against the axis (-90 is 270)"},
                "plane": choice(&["xz", "yz", "xy"], "Instead of an axis: the origin plane whose frame becomes XY's")
            },
            "required": [],
            "examples": [{"cmd": "reorient_design", "axis": "x", "degrees": 90}, {"cmd": "reorient_design", "plane": "xz"}],
            "restrictions": ["axis with degrees, or plane alone."]
        }),
        "convert_derived_part" => json!({
            "description": "Convert to Features: a derived part becomes features of its own. Result: check, component, features (uid, name), history, bodies, rewritten.",
            "properties": {
                "feature": feature_uid("The derived_part feature"),
                "component": {"type": "string", "pattern": "^C[0-9]+$", "description": "Instead of feature: the part's component (C2), its derived part"},
                "check": flag(false, "Only check: nothing changes, the answer says what it would make (or the refusal why not)")
            },
            "required": [],
            "examples": [{"cmd": "convert_derived_part", "feature": "F1"}, {"cmd": "convert_derived_part", "component": "C1", "check": true}]
        }),
        _ => return None,
    };
    Some(spec)
}

/// A component family command's schema as [`super`] writes its own.
pub(super) fn spec(name: &str) -> Option<Value> {
    components(name).or_else(|| others(name))
}
