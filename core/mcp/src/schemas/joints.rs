// SPDX-License-Identifier: MIT
//! The JSON Schemas of joints, motion links, gear and belt drives and
//! motion studies (mitcad#1918): the commands of
//! `core/model-api/src/joints.rs`, `link_gears` (`gears.rs`), `drives.rs`
//! and `motion.rs`, written from their fields as [`super`] writes the
//! others. The drift test `core/model-tests/api/src/mcp_schema_tests.rs`
//! checks them against the API.

use serde_json::{Value, json};

use super::{choice, feature_uid, flag, id, ids, text, value_input};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "add_joint",
    "add_as_built_joint",
    "add_rigid_group",
    "drive_joint",
    "drag_occurrence",
    "add_constraint_set",
    "add_motion_link",
    "link_gears",
    "add_gear_pair",
    "add_belt_drive",
    "add_motion_study",
    "edit_motion_study",
    "delete_motion_study",
    "set_study_key",
    "remove_study_key",
    "play_study",
    "add_contact_set",
    "edit_contact_set",
    "delete_contact_set",
];

/// A joint's kinds (`JointKind`).
const KINDS: &[&str] = &[
    "rigid",
    "revolute",
    "slider",
    "cylindrical",
    "pin_slot",
    "planar",
    "ball",
];

/// The free motions of a joint (`Motion`).
const MOTIONS: &[&str] = &["tx", "ty", "tz", "rx", "ry", "rz"];

/// What a motion's value is.
const MOTION_VALUES: &str = "keyed by motion (tx, ty, tz: mm; rx, ry, rz: radians), each a number or an expression (\"30 deg\")";

fn kind() -> Value {
    choice(KINDS, "The joint's kind: its free motions")
}

fn slide_axis() -> Value {
    json!({"type": "string", "enum": ["x", "y", "z"], "default": "z", "description": "A slider's axis in the joint's frame"})
}

fn motion(description: &str) -> Value {
    choice(MOTIONS, description)
}

/// A place in the component's coordinates, mm.
fn vec3(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"},
           "description": format!("{description}: [x, y, z] in mm")})
}

/// A joint origin: geometry of an occurrence, with an optional frame
/// override.
fn origin(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "occurrence": {"type": "string", "default": "", "description": "The occurrence by a path from the joint's component, uids or names (\"O1/O4\", \"Arm:1\"); \"\" or left out: the component's own geometry"},
            "geometry": {"type": ["string", "object"], "description": "The geometry the frame is on: an origin plane, axis or point (\"xy\", \"z\", \"origin\"), a construction feature (\"F5\"), or an object naming a face, edge or vertex of a body ({\"body\", \"face\"}), a sketch's curve or point ({\"sketch\", \"curve\"}, {\"sketch\", \"point\"}); model_reference {\"section\": \"Joints\"}"},
            "frame_override": {"type": "object", "description": "The frame moved or turned: origin, z_axis, x_axis, shift ([x, y, z] each, all optional); relative: {origin, z_axis, x_axis, nearest} in the resolved frame's own axes instead (BUILD's parts on a plane or a face)",
                "properties": {"origin": {"type": "array"}, "z_axis": {"type": "array"}, "x_axis": {"type": "array"}, "shift": {"type": "array"},
                    "relative": {"type": "object", "properties": {"origin": {"type": "array"}, "z_axis": {"type": "array"}, "x_axis": {"type": "array"}, "nearest": {"type": "boolean"}}, "additionalProperties": false}},
                "additionalProperties": false}
        },
        "required": ["geometry"],
        "additionalProperties": false
    })
}

/// The limits of the free motions.
fn limits() -> Value {
    json!({"type": "object", "description": format!("The free motions' limits, {MOTION_VALUES}: {{\"min\", \"max\", \"rest\"}} each, all optional"),
           "propertyNames": {"enum": MOTIONS}})
}

/// The values free motions are driven to.
fn position() -> Value {
    json!({"type": "object", "description": format!("The values the free motions are driven to, {MOTION_VALUES}"),
           "propertyNames": {"enum": MOTIONS}})
}

fn component() -> Value {
    text("The component (uid or name) it goes into; the active component when left out")
}

fn joint() -> Value {
    text("A joint by its uid or name")
}

fn study() -> Value {
    text("The motion study by its name")
}

/// A side of a motion link.
fn link_side(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "joint": joint(),
            "motion": motion("The joint's motion"),
            "occurrence": text("A joint of a flexible occurrence's component: that occurrence (uid or name), placed in the link's component")
        },
        "required": ["joint", "motion"],
        "additionalProperties": false
    })
}

/// A key of a motion study.
fn key() -> Value {
    json!({
        "type": "object",
        "properties": {
            "joint": joint(),
            "motion": motion("The joint's motion"),
            "points": {"type": "array", "items": {"type": "array", "minItems": 2, "maxItems": 2, "prefixItems": [{"type": "number"}, {"type": ["number", "string"]}]},
                       "description": "[step, value] points: the value (mm or radians, or an expression) at that step"}
        },
        "required": ["joint", "motion", "points"],
        "additionalProperties": false
    })
}

/// A part of a pair: a feature definition without its type.
fn pair_part(kind: &str) -> Value {
    json!({"type": "object", "description": format!("A {kind} definition as add_feature's def takes it, without type (model_reference {{\"feature\": \"{kind}\"}}), usually with its at placement")})
}

fn names() -> Value {
    json!({"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "string"}, "description": "The two features' names [first, second]"})
}

/// A joints, drives or motion command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "add_joint" => json!({
            "description": "Adds a joint: origin a on geometry of one occurrence joined to origin b on another's. Result: uid, name, parameters, state, solved.",
            "properties": {
                "kind": kind(),
                "slide_axis": slide_axis(),
                "a": origin("The first origin (the occurrence that moves)"),
                "b": origin("The second origin"),
                "offset": value_input("An offset along the joint's z axis"),
                "angle": value_input("An angle about the joint's z axis"),
                "flip": flag(false, "Flips the joint's z axis"),
                "limits": limits(),
                "position": position(),
                "start": {"type": "object", "description": format!("The motions' values where the joint starts, {MOTION_VALUES} (numbers only)"),
                          "propertyNames": {"enum": MOTIONS}},
                "name": text("Its name; default the next Joint<n>"),
                "component": component()
            },
            "required": ["kind", "a", "b"],
            "examples": [
                {"cmd": "add_joint", "kind": "revolute", "a": {"occurrence": "Arm:1", "geometry": "z"}, "b": {"geometry": "z"}},
                {"cmd": "add_joint", "kind": "slider", "slide_axis": "x", "a": {"occurrence": "Carriage:1", "geometry": "xy"}, "b": {"geometry": "xy"},
                 "limits": {"tx": {"min": 0, "max": "stroke"}}, "position": {"tx": 10}, "name": "Slide"}
            ]
        }),
        "add_as_built_joint" => json!({
            "description": "Joins two occurrences where they are; an origin sets the frame of the free motions. Result: uid, name, parameters, state, solved.",
            "properties": {
                "kind": kind(),
                "slide_axis": slide_axis(),
                "a": text("The first occurrence: a path from the joint's component, uids or names"),
                "b": text("The second occurrence"),
                "origin": origin("The frame of the free motions"),
                "limits": limits(),
                "position": position(),
                "name": text("Its name"),
                "component": component()
            },
            "required": ["kind", "a", "b"],
            "examples": [{"cmd": "add_as_built_joint", "kind": "revolute", "a": "Arm:1", "b": "Base:1", "origin": {"occurrence": "Arm:1", "geometry": "z"}}]
        }),
        "add_rigid_group" => json!({
            "description": "Adds a rigid group: occurrences that move as one. Result: uid, name.",
            "properties": {
                "occurrences": ids("The occurrences (uids or names), placed in the group's component"),
                "name": text("Its name"),
                "component": component()
            },
            "required": ["occurrences"],
            "examples": [{"cmd": "add_rigid_group", "occurrences": ["Bolt:1", "Nut:1"], "name": "Fastener"}]
        }),
        "drive_joint" => json!({
            "description": "Drives a joint's free motions. Result: uid, parameters, state, values; contact and warnings when a contact set stopped it.",
            "properties": {
                "joint": joint(),
                "values": {"type": "object", "description": format!("The values, {MOTION_VALUES}; null takes a motion's value away"),
                           "propertyNames": {"enum": MOTIONS}},
                "occurrence": text("A flexible occurrence: the joint of its component driven for it alone")
            },
            "required": ["joint", "values"],
            "examples": [{"cmd": "drive_joint", "joint": "Revolute1", "values": {"rz": "30 deg"}}]
        }),
        "drag_occurrence" => json!({
            "description": "Drags an occurrence as far as its joints let it. Result: what moved and the joints' values.",
            "properties": {
                "occurrence": text("The occurrence: a path from the root (\"O1/O3\", \"Hinge:1/Arm:1\") or a uid or name"),
                "point": vec3("The point dragged, on the occurrence; its origin when left out"),
                "target": vec3("Where the point is dragged to")
            },
            "required": ["occurrence", "target"],
            "examples": [{"cmd": "drag_occurrence", "occurrence": "Arm:1", "point": [50, 0, 0], "target": [40, 30, 0]}]
        }),
        "add_constraint_set" => json!({
            "description": "Adds a constraint set: rows that hold occurrences against each other. Result: uid, name, parameters, state, solved.",
            "properties": {
                "rows": {"type": "array", "minItems": 1, "items": {"type": "object",
                    "description": "A row: type (align, angle, center, tangent, insert, insert_axis), a and b (origins {occurrence, geometry, frame_override}, occurrences also by names), optional a2, b2, offset, angle, flip, limits {min, max, rest}, lock_rotation; model_reference {\"section\": \"Assembly constraints\"}",
                    "properties": {"type": {"type": "string", "enum": ["align", "angle", "center", "tangent", "insert", "insert_axis"]}},
                    "required": ["type", "a", "b"]}},
                "name": text("Its name"),
                "component": component()
            },
            "required": ["rows"],
            "examples": [{"cmd": "add_constraint_set", "rows": [{"type": "align", "a": {"occurrence": "Bracket:1", "geometry": "xy"}, "b": {"geometry": "xy"}, "offset": 5}]}]
        }),
        "add_motion_link" => json!({
            "description": "Adds a motion link: one joint's motion follows another's. Result: uid, name, parameters, state, solved.",
            "properties": {
                "a": link_side("The leading motion"),
                "b": link_side("The following motion"),
                "coupling": {"type": "object", "description": "How b follows a, by its type: ratio {ratio}, gear {ra, rb, internal}, rack_pinion {radius}, screw {lead} (a size is a value or round geometry {occurrence, geometry}), cam {profile, follower, tip}; model_reference {\"section\": \"Motion links\"}",
                             "properties": {"type": {"type": "string", "enum": ["ratio", "gear", "rack_pinion", "screw", "cam"]}},
                             "required": ["type"]},
                "reverse": flag(false, "Reverses the following motion"),
                "offset": value_input("b's value where a's is zero"),
                "as_built": {"type": "boolean", "description": "Without an offset: keep the joints where they are (the default for every coupling but a cam)"},
                "name": text("Its name"),
                "component": component()
            },
            "required": ["a", "b", "coupling"],
            "examples": [{"cmd": "add_motion_link", "a": {"joint": "Revolute1", "motion": "rz"}, "b": {"joint": "Slider1", "motion": "tz"}, "coupling": {"type": "ratio", "ratio": 2}}]
        }),
        "link_gears" => json!({
            "description": "Links a gear with the gear it meshes with: the motion link of their joints worked out from the gears. Result: as add_motion_link.",
            "properties": {
                "gear": feature_uid("The gear feature that meshes with another"),
                "name": text("The link's name")
            },
            "required": ["gear"],
            "examples": [{"cmd": "link_gears", "gear": "F3"}]
        }),
        "add_gear_pair" => json!({
            "description": "Adds two gears in one undo step, the second meshing with the first; when either is refused, neither. Result: uids, names.",
            "properties": {
                "first": pair_part("gear"),
                "second": pair_part("gear"),
                "component": component(),
                "names": names()
            },
            "required": ["first", "second"],
            "examples": [{"cmd": "add_gear_pair",
                          "first": {"kind": "spur", "module": 1, "teeth": 20, "face_width": 8, "operation": "new_body"},
                          "second": {"kind": "spur", "module": 1, "teeth": 40, "face_width": 8, "operation": "new_body"},
                          "names": ["Pinion", "Wheel"]}],
            "restrictions": ["The second gear must not have a mesh_with of its own: it gets the first's."]
        }),
        "add_belt_drive" => json!({
            "description": "Adds two pulleys in one undo step; when either is refused, neither. Result: uids, names.",
            "properties": {
                "first": pair_part("pulley"),
                "second": pair_part("pulley"),
                "component": component(),
                "names": names()
            },
            "required": ["first", "second"],
            "examples": [{"cmd": "add_belt_drive",
                          "first": {"kind": "toothed", "profile": "gt2_2mm", "teeth": 20, "belt_width": 6, "operation": "new_body"},
                          "second": {"kind": "toothed", "profile": "gt2_2mm", "teeth": 40, "belt_width": 6, "center": [60, 0], "operation": "new_body"}}]
        }),
        "add_motion_study" => json!({
            "description": "Adds a motion study: joints' motions keyed at steps. Result: name.",
            "properties": {
                "name": text("Its name, unique; default Study<n>"),
                "steps": {"type": "integer", "minimum": 1, "description": "Its number of steps; default the standard count"},
                "keys": {"type": "array", "default": [], "items": key(), "description": "Its keys"}
            },
            "required": [],
            "examples": [{"cmd": "add_motion_study", "name": "Swing", "steps": 100,
                          "keys": [{"joint": "Revolute1", "motion": "rz", "points": [[0, 0], [100, "90 deg"]]}]}]
        }),
        "edit_motion_study" => json!({
            "description": "Renames a motion study, changes its steps or replaces its keys. Result: name.",
            "properties": {
                "name": study(),
                "new_name": text("Its new name"),
                "steps": {"type": "integer", "minimum": 1, "description": "Its number of steps"},
                "keys": {"type": "array", "items": key(), "description": "Its keys, replacing all"}
            },
            "required": ["name"],
            "examples": [{"cmd": "edit_motion_study", "name": "Swing", "new_name": "Open", "steps": 50}]
        }),
        "delete_motion_study" => json!({
            "description": "Deletes a motion study.",
            "properties": {"name": study()},
            "required": ["name"],
            "examples": [{"cmd": "delete_motion_study", "name": "Swing"}]
        }),
        "set_study_key" => json!({
            "description": "Keys a joint's motion at a step of a study (Add Key). Result: joint, motion, step, value.",
            "properties": {
                "study": study(),
                "joint": joint(),
                "motion": motion("The joint's motion"),
                "step": {"type": "number", "description": "The step"},
                "value": value_input("The value; where the joint is when left out")
            },
            "required": ["study", "joint", "motion", "step"],
            "examples": [{"cmd": "set_study_key", "study": "Swing", "joint": "Revolute1", "motion": "rz", "step": 50, "value": "45 deg"}]
        }),
        "remove_study_key" => json!({
            "description": "Removes a joint motion's key at a step, or every key of the motion.",
            "properties": {
                "study": study(),
                "joint": joint(),
                "motion": motion("The joint's motion"),
                "step": {"type": "number", "description": "The step; every key of the motion when left out"}
            },
            "required": ["study", "joint", "motion"],
            "examples": [{"cmd": "remove_study_key", "study": "Swing", "joint": "Revolute1", "motion": "rz", "step": 50}]
        }),
        "play_study" => json!({
            "description": "Moves the joints to a study's values at a step. Result: the joints' values, study, step; contact when a contact set stopped it.",
            "properties": {
                "study": study(),
                "step": {"type": "number", "description": "The step"}
            },
            "required": ["study", "step"],
            "examples": [{"cmd": "play_study", "study": "Swing", "step": 25}]
        }),
        "add_contact_set" => json!({
            "description": "Adds a contact set: occurrences whose bodies stop each other's motion. Result: name.",
            "properties": {
                "name": text("Its name, unique; default the next Contact Set <n>"),
                "occurrences": ids("Its occurrences (paths of uids or names)"),
                "suppressed": flag(false, "Suppressed")
            },
            "required": ["occurrences"],
            "examples": [{"cmd": "add_contact_set", "name": "Stops", "occurrences": ["Arm:1", "Base:1"]}]
        }),
        "edit_contact_set" => json!({
            "description": "Renames a contact set, replaces its occurrences or suppresses it.",
            "properties": {
                "name": id("The contact set by its name"),
                "new_name": text("Its new name"),
                "occurrences": ids("Its occurrences, replacing all"),
                "suppressed": {"type": "boolean", "description": "Suppressed"}
            },
            "required": ["name"],
            "examples": [{"cmd": "edit_contact_set", "name": "Stops", "suppressed": true}]
        }),
        "delete_contact_set" => json!({
            "description": "Deletes a contact set.",
            "properties": {"name": id("The contact set by its name")},
            "required": ["name"],
            "examples": [{"cmd": "delete_contact_set", "name": "Stops"}]
        }),
        _ => return None,
    };
    Some(spec)
}
