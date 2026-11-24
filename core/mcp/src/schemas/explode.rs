// SPDX-License-Identifier: MIT
//! The JSON Schemas of exploded views (mitcad#2113): the commands of
//! `core/model-api/src/explode.rs`, written from their fields as
//! [`super`] writes the others. The drift test
//! `core/model-tests/api/src/mcp_schema_tests.rs` checks them against the
//! API.

use serde_json::{Value, json};

use super::{flag, ids, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "add_explode_view",
    "edit_explode_view",
    "remove_explode_view",
    "auto_explode",
];

/// A step's value: a number or an expression.
fn value(description: &str, unit: &str) -> Value {
    json!({
        "type": ["number", "string"],
        "description": format!("{description}: a number in {unit}, or text read as an expression with units and parameters (\"40 mm\", \"gap * 2\", \"90 deg\")")
    })
}

/// A direction or an axis.
fn axis(description: &str) -> Value {
    json!({
        "description": format!("{description}: [x, y, z] in the view's component's coordinates, an axis of it (\"x\", \"-y\", \"z\"), or {{\"vector\": [x, y, z]}} or {{\"occurrence\": \"<path from the view's component>\", \"geometry\": <an edge, an axis, a face, a plane, an origin axis>}}, each with \"flip\": true for the other way; geometry follows edits of the design, and \"toward\": [x, y, z] sets flip so that it points that way"),
        "anyOf": [
            {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3},
            {"type": "string"},
            {"type": "object", "properties": {
                "vector": {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3},
                "occurrence": {"type": "string"},
                "geometry": {"type": "object"},
                "flip": {"type": "boolean"},
                "toward": {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3}
            }, "additionalProperties": false}
        ]
    })
}

/// A step's fields.
fn step(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "name": text("Its own name; Step <n> when left out"),
            "occurrences": ids("The occurrences it moves, paths from the view's component by uids or names (Arm:1/Pin:1)"),
            "bodies": ids("Bodies of the view's component itself it moves, by uid or name"),
            "distance": value("How far it moves them along direction", "mm"),
            "direction": axis("The direction it moves them in"),
            "angle": value("How far it turns them about axis", "degrees"),
            "axis": axis("The axis it turns them about; a vector alone turns about the moved parts' centre"),
            "trails": flag(true, "Trail lines from each moved part back to where the step found it"),
            "point": {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": "The trails' reference point in the component's coordinates, assembled; each part's box centre when left out"},
            "seconds": {"type": "number", "exclusiveMinimum": 0, "description": "Its time in the animation; 1 when left out"},
            "move": {"type": "boolean", "description": "In set: false takes the step's move away"},
            "turn": {"type": "boolean", "description": "In set: false takes the step's turn away"}
        },
        "additionalProperties": false
    })
}

/// The schema of a command of this family, None for another.
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "add_explode_view" => json!({
            "description": "Adds an explode view of an assembly component: steps that move occurrences apart along directions and/or turn them about axes, each after the earlier ones, with trail lines back to where they were. A view state: changes no placement, joint or timeline; one undo step that recomputes nothing. Result: name.",
            "properties": {
                "name": text("Its name, unique; default Explode<n>"),
                "component": text("The assembly component (uid or name); the root when left out"),
                "steps": {"type": "array", "default": [], "items": step("A step"), "description": "Its steps in order"}
            },
            "required": [],
            "examples": [{"cmd": "add_explode_view", "name": "Apart",
                          "steps": [{"occurrences": ["Lid:1"], "distance": 40, "direction": "z"},
                                    {"occurrences": ["Lid:1"], "angle": 90, "axis": "x"}]}]
        }),
        "edit_explode_view" => json!({
            "description": "Changes an explode view: renames it, replaces its steps, adds, changes, removes or moves one, or turns every step's trail lines on or off. Result: name.",
            "properties": {
                "name": text("The view"),
                "new_name": text("Its new name"),
                "steps": {"type": "array", "items": step("A step"), "description": "Every step, replacing its own"},
                "add_step": step("A step added"),
                "at": {"type": "integer", "minimum": 0, "description": "Where add_step goes (an index from 0); the end when left out"},
                "step": {"type": "integer", "minimum": 0, "description": "The index (from 0) of the step set changes"},
                "set": step("What of the step changes (what is left out stays)"),
                "remove_step": {"type": "integer", "minimum": 0, "description": "The index of a step removed"},
                "move_step": {"type": "integer", "minimum": 0, "description": "The index of a step moved to to"},
                "to": {"type": "integer", "minimum": 0, "description": "Where move_step goes"},
                "trails": {"type": "boolean", "description": "Every step's trail lines on or off"}
            },
            "required": ["name"],
            "examples": [{"cmd": "edit_explode_view", "name": "Apart", "step": 0, "set": {"distance": "60 mm"}},
                         {"cmd": "edit_explode_view", "name": "Apart", "move_step": 1, "to": 0}]
        }),
        "remove_explode_view" => json!({
            "description": "Removes an explode view; the design stays as it is.",
            "properties": {"name": text("The view")},
            "required": ["name"],
            "examples": [{"cmd": "remove_explode_view", "name": "Apart"}]
        }),
        "auto_explode" => json!({
            "description": "Auto Explode: steps that move the chosen occurrences (the component's top-level ones by default) out from the assembly's centre, each along a slider or cylindrical joint's axis where one lets it slide, else along the main axis of the line from the centre to it, by its size times factor, the outermost first. Replaces the named view's steps (after them with append), or makes a new view. One undo step. Result: name, steps (the count).",
            "properties": {
                "name": text("The view; a new one (this name, or Explode<n>) when there is none of this name"),
                "component": text("A new view's component (uid or name); the root when left out"),
                "occurrences": ids("The occurrences it moves, paths from the component; its top-level ones when left out"),
                "factor": {"type": "number", "exclusiveMinimum": 0, "default": 1, "description": "Each part moves by its size (its box's diagonal) times this"},
                "append": flag(false, "After the view's steps instead of in their place")
            },
            "required": [],
            "examples": [{"cmd": "auto_explode", "factor": 1.5}]
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
    }
}
