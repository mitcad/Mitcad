// SPDX-License-Identifier: MIT
//! The JSON Schemas of Flex items, hanging cables and the direction of
//! gravity (mitcad#1713): the `FlexCommand` variants of
//! `core/model-api/src/flex.rs` that MCP permits, with the hanging
//! commands' fields in `flex_hang.rs` (`HangCommand`, `HangStepCommand`,
//! `HangSetCommand`), and the `GravityCommand` variants of `gravity.rs`,
//! written as [`super`] writes the others. The library of Flex types
//! (`flex.save_type`, `flex.edit_type`, `flex.delete_type`) reads and
//! writes files, so MCP has no schema for it.

use serde_json::{Value, json};

use super::{feature_uid, flag, id, ids, number, object};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "flex.add_node",
    "flex.move_node",
    "flex.attach_node",
    "flex.delete_node",
    "flex.set_twist",
    "flex.set_item",
    "gravity.set",
    "gravity.clear",
    "flex.hang",
    "flex.hang_step",
    "flex.hang_set",
    "flex.hang_lock",
    "flex.hang_cancel",
];

/// Where the Flex reference describes the fields.
const FLEX: &str = "model_reference {\"section\": \"Flex (mitcad#1713)\"}";

/// Where the hanging commands are described.
const HANGING: &str = "model_reference {\"section\": \"Hanging cables (mitcad#1713)\"}";

/// Where the direction of gravity is described.
const GRAVITY: &str = "model_reference {\"section\": \"Gravity (mitcad#1713)\"}";

/// What a `flex.*` node command answers.
const ANSWER: &str =
    "Result: flex, node (the node made or changed), nodes (the ids in order), length, warnings.";

/// The hanging item a hanging command answers with.
const HANGING_ITEM: &str = "Result: the hanging item (flex, length, segment, at_rest, time, taut, grabbed, damping, polyline, normals, touching, nodes).";

/// The restriction of `flex.set_item` (`command_class_with` in
/// `core/model-api/src/classes.rs`): a type of a named library is read
/// from its folder.
const SET_ITEM_LIBRARY: &str = "MCP refuses its library form: type with library (user or project) reads that library's folder; leave library out for a built-in type, or write the item whole.";

/// The Flex item a command changes.
fn flex() -> Value {
    feature_uid("The Flex item (the feature's uid)")
}

/// A node of the item.
fn node(description: &str) -> Value {
    json!({"type": "string", "minLength": 1, "description": description})
}

/// A point in the item's component, mm.
fn point(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": format!("{description}, [x, y, z] in mm")})
}

/// What a node is attached to (`on`).
fn on() -> Value {
    object(
        "What the node is attached to, a reference: a face ({\"body\", \"face\"}), an edge, a vertex, a sketch point, a construction point or the origin; with occurrence and context for geometry of another component",
    )
}

/// The cap face a node begins or ends at (`port`).
fn port() -> Value {
    object(
        "The cap face of a Pipe or another Flex item ({\"body\", \"face\"} with a face <F>:start or <F>:end; the end_ports query lists them), at the first or the last node only; with occurrence and context for geometry of another component",
    )
}

/// A value an item's option takes, or null to take it away.
fn option_value(description: &str) -> Value {
    json!({
        "type": ["number", "string", "null"],
        "description": format!("{description}: a number in mm or text read as an expression (a parameter's name); null takes it away")
    })
}

/// The damping of a hanging item.
fn damping(description: &str) -> Value {
    json!({"type": "number", "minimum": 0, "maximum": 1, "description": description})
}

/// A Flex command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "flex.add_node" => json!({
            "description": format!("Adds a node to a Flex item, at an end or between two nodes. {ANSWER} {FLEX}."),
            "properties": {
                "flex": flex(),
                "at": point("Where the node goes"),
                "on": on(),
                "port": port(),
                "twist": {"type": "number", "default": 0, "description": "Radians the section is turned about the route at the node"},
                "place": {
                    "description": "Where it goes in the order: \"end\" (after the last node), \"start\" (before the first) or {\"after\": \"<node>\"} (between that node and the next)",
                    "default": "end",
                    "oneOf": [
                        {"type": "string", "enum": ["start", "end"]},
                        {"type": "object", "properties": {"after": {"type": "string"}}, "required": ["after"], "additionalProperties": false}
                    ]
                }
            },
            "required": ["flex", "at"],
            "examples": [
                {"cmd": "flex.add_node", "flex": "F7", "at": [90, 40, 0]},
                {"cmd": "flex.add_node", "flex": "F7", "at": [30, 10, 20], "on": {"body": "F3.b0", "face": "F3:top"}, "place": {"after": "n1"}}
            ]
        }),
        "flex.move_node" => json!({
            "description": format!("Moves a node: a free node to the point, an attached one to the point of its face, edge or point nearest to it (it stays attached). {ANSWER} {FLEX}."),
            "properties": {
                "flex": flex(),
                "node": node("The node (n<k>)"),
                "at": point("Where it goes")
            },
            "required": ["flex", "node", "at"],
            "examples": [{"cmd": "flex.move_node", "flex": "F7", "node": "n2", "at": [30, 15, 20]}]
        }),
        "flex.attach_node" => json!({
            "description": format!("Attaches a node to a face, an edge, a point or a port instead, or makes it free in space where it is; give one of on, port and free. {ANSWER} {FLEX}."),
            "properties": {
                "flex": flex(),
                "node": node("The node (n<k>)"),
                "on": on(),
                "port": port(),
                "free": flag(false, "true: the node is free in space where it is"),
                "at": point("Where on the new face or edge; left out: where the node is now")
            },
            "required": ["flex", "node"],
            "examples": [
                {"cmd": "flex.attach_node", "flex": "F7", "node": "n2", "free": true},
                {"cmd": "flex.attach_node", "flex": "F7", "node": "n3", "on": {"body": "F3.b0", "face": "F3:side1"}}
            ]
        }),
        "flex.delete_node" => json!({
            "description": format!("Deletes a node; the route goes straight from the node before to the node after. Refused when two nodes would not be left. {ANSWER} {FLEX}."),
            "properties": {
                "flex": flex(),
                "node": node("The node (n<k>)")
            },
            "required": ["flex", "node"],
            "examples": [{"cmd": "flex.delete_node", "flex": "F7", "node": "n3"}]
        }),
        "flex.set_twist" => json!({
            "description": format!("Sets a node's twist: radians the section is turned about the route there, right hand about the route's direction (a half turn of a flat cable is pi). {ANSWER} {FLEX}."),
            "properties": {
                "flex": flex(),
                "node": node("The node (n<k>)"),
                "twist": number("The twist, radians")
            },
            "required": ["flex", "node", "twist"],
            "examples": [{"cmd": "flex.set_twist", "flex": "F7", "node": "n2", "twist": 1.5}]
        }),
        "flex.set_item" => json!({
            "description": format!("Sets what a Flex item is: the item written whole or a Flex type's copy (a built-in type by its id: the flex_types query lists them), its section from a sketch, its own bend radii and its fixed length; give the fields that change. {ANSWER} {FLEX} (Flex types)."),
            "properties": {
                "flex": flex(),
                "item": object("The whole item, as in the flex definition: name, section ({\"type\": \"round\", \"diameter\"}, {\"type\": \"rectangular\", \"width\", \"height\", \"corner_radius\"} or {\"type\": \"outline\", \"loops\"}), wall, bend_radius, bend_radius_edge, max_twist, appearance, material, mass_per_length, origin; not with type"),
                "type": id("Instead of item: the id of a Flex type copied into the item (hose-10, cable-6, flat-cable-12, ...)"),
                "library": id("The library of type: built_in when left out; user or project read a library's folder, which MCP refuses"),
                "project": id("The project's folder, for a type of the project's library"),
                "profile": {"type": ["object", "null"], "description": "A closed region of a sketch before the item as its section, {\"sketch\": \"F<n>\", \"region\": \"r{...}\"}; null: the item's own section again", "properties": {"sketch": {"type": "string"}, "region": {"type": "string"}}, "additionalProperties": false},
                "bend_radius": option_value("The item's own bend radius in place of its type's"),
                "bend_radius_edge": option_value("The item's own bend radius over its edge (a section that is not round)"),
                "fixed_length": option_value("The item's fixed length: its slack hangs toward gravity, or the flex query reports it")
            },
            "required": ["flex"],
            "examples": [
                {"cmd": "flex.set_item", "flex": "F7", "type": "hose-10"},
                {"cmd": "flex.set_item", "flex": "F7", "bend_radius": 40, "fixed_length": "300 mm"}
            ],
            "restrictions": [SET_ITEM_LIBRARY]
        }),
        "gravity.set" => json!({
            "description": format!("Sets the design's direction of gravity, which Flex items hang toward: a vector, or a planar face or a plane (gravity pulls against its normal) or an axis (along it). An item that hangs goes on hanging toward the new direction. Result: as the gravity query (direction, source, source_direction, hanging). {GRAVITY}."),
            "properties": {
                "direction": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": "The direction as [x, y, z], any length (not zero); not with geometry"},
                "geometry": {"type": ["string", "object"], "description": "Instead of direction: a reference to a planar face, a plane (xy, xz, yz, a construction plane) or an axis (x, y, z, a construction axis, a straight edge, a sketch line)"},
                "occurrence": id("With geometry: the occurrence path the geometry was picked at, from the root"),
                "reverse": flag(false, "With geometry: the other way")
            },
            "required": [],
            "examples": [
                {"cmd": "gravity.set", "direction": [0, 0, -1]},
                {"cmd": "gravity.set", "geometry": "xy", "reverse": true}
            ]
        }),
        "gravity.clear" => json!({
            "description": format!("Takes the design's direction of gravity away; an item that hangs is let go (put back as it was). Result: as the gravity query. {GRAVITY}."),
            "properties": {},
            "required": [],
            "examples": [{"cmd": "gravity.clear"}]
        }),
        "flex.hang" => json!({
            "description": format!("Lets a Flex item hang toward the design's direction of gravity: a session that changes nothing in the design until flex.hang_lock. Refused without a direction of gravity, or for an item without a route. {HANGING_ITEM} {HANGING}."),
            "properties": {
                "flex": flex(),
                "pins": ids("The nodes that stay where they are; left out: both ends and every node attached to a face, an edge, a point or a port"),
                "damping": damping("How strongly the air holds it back, 0 to 1 (0.5 when left out)")
            },
            "required": ["flex"],
            "examples": [
                {"cmd": "flex.hang", "flex": "F7"},
                {"cmd": "flex.hang", "flex": "F7", "pins": ["n1", "n4"], "damping": 0.6}
            ]
        }),
        "flex.hang_step" => json!({
            "description": format!("Simulates the hanging item for a time, optionally with a point of it held by the pointer; changes only the session (no undo step). {HANGING_ITEM} {HANGING}."),
            "properties": {
                "seconds": {"type": "number", "exclusiveMinimum": 0, "maximum": 10, "default": 0.0333, "description": "The seconds to simulate (1/30 when left out, at most 10)"},
                "grab": {
                    "type": "object",
                    "description": "A point of the chain held by the pointer: where it took the chain and where the pointer is now; left out, it is let go",
                    "properties": {
                        "point": point("Where the pointer took the chain"),
                        "target": point("Where the pointer is now")
                    },
                    "required": ["point", "target"],
                    "additionalProperties": false
                }
            },
            "required": [],
            "examples": [
                {"cmd": "flex.hang_step", "seconds": 0.5},
                {"cmd": "flex.hang_step", "grab": {"point": [40, 0, 10], "target": [40, 20, 30]}}
            ]
        }),
        "flex.hang_set" => json!({
            "description": format!("Changes the hanging item's pins, length or damping (one or more of them); changes only the session (no undo step). {HANGING_ITEM} {HANGING}."),
            "properties": {
                "pins": ids("The nodes pinned; the others are let go"),
                "length": {"type": "number", "exclusiveMinimum": 0, "description": "The item's length, mm"},
                "damping": damping("How strongly the air holds it back, 0 to 1")
            },
            "required": [],
            "examples": [
                {"cmd": "flex.hang_set", "length": 320},
                {"cmd": "flex.hang_set", "pins": ["n1"], "damping": 0.3}
            ]
        }),
        "flex.hang_lock" => json!({
            "description": format!("Locks the hanging item where it rests: an ordinary Flex item again, one undo step. Result: flex, nodes, length, warnings and deviation (mm, the farthest the chain is from the route through the nodes). {HANGING}."),
            "properties": {},
            "required": [],
            "examples": [{"cmd": "flex.hang_lock"}]
        }),
        "flex.hang_cancel" => json!({
            "description": format!("Ends the hanging session and puts the item back as it was; the design does not change. Result: cancelled (whether an item hung). {HANGING}."),
            "properties": {},
            "required": [],
            "examples": [{"cmd": "flex.hang_cancel"}]
        }),
        _ => return None,
    };
    Some(spec)
}
