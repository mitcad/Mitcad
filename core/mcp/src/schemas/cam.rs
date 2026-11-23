// SPDX-License-Identifier: MIT
//! The JSON Schemas of the CAM commands (mitcad#1918): setups, their
//! operations, and the machines with their tool libraries kept with the
//! design, written from their fields in `core/model-api/src/cam.rs` and the
//! inputs of `core/model/src/cam/mod.rs` as [`super`] writes the others.
//! A setup's, an operation's, a machine's and a tool's nested fields are
//! listed here in brief; the reference section "CAM: setups, operations and
//! machines" gives each one's values.

use serde_json::{Value, json};

use super::{choice, object, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "cam_setup_add",
    "cam_setup_edit",
    "cam_setup_delete",
    "cam_setup_move",
    "cam_operation_add",
    "cam_operation_edit",
    "cam_operation_delete",
    "cam_operation_move",
    "cam_machine_put",
    "cam_machine_delete",
    "cam_tool_put",
    "cam_tool_delete",
    "cam_generate",
];

/// Where the nested fields are described in full.
const SECTION: &str = "model_reference {\"section\": \"CAM: setups, operations and machines\"}";
/// Where a lathe, its tools and turning are described (mitcad#1869).
const TURNING: &str = "model_reference {\"section\": \"CAM: turning on a lathe\"}";

/// A CAM value: a number in the design's units or an expression with units.
fn cam_value(description: &str) -> Value {
    json!({
        "type": ["number", "string"],
        "description": format!("{description}: a number in mm, mm/min, rpm or degrees (the design's units), or an expression with units (\"0.25 in\", \"thickness + 1\")")
    })
}

/// A setup by its id or its name.
fn setup_ref(description: &str) -> Value {
    json!({
        "type": ["integer", "string"],
        "description": format!("{description}: its id (a number) or its name")
    })
}

/// An operation by its id.
fn operation_id(description: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "description": description})
}

/// A machine by its id.
fn machine_id(description: &str) -> Value {
    json!({"type": "string", "minLength": 1, "description": description})
}

/// A position in an order, from 0.
fn index(description: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "description": description})
}

/// A setup's fields (`cam_setup_add`'s `setup`, `cam_setup_edit`'s
/// `changes`): each optional.
fn setup_fields(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}; each field optional, those given are set ({SECTION})"),
        "properties": {
            "name": text("Its name; default Setup <n>"),
            "bodies": {"type": "array", "description": "The bodies it machines: a body id (\"F2.b0\") or {\"body\", \"occurrence\"}; empty: every body but the stock body"},
            "stock": object("{\"kind\": \"around_bodies\", \"side\", \"top\", \"bottom\"} (margins) or {\"kind\": \"body\", \"body\", \"occurrence\"}"),
            "wcs": object("The work coordinates: origin ({\"kind\": \"stock\", \"corner\"} or {\"kind\": \"point\", \"at\"}), z (a face, plane or axis reference), x (an axis reference), z_occurrence, x_occurrence"),
            "work_offset": {"type": "integer", "minimum": 1, "maximum": 6, "description": "The work offset, 1-6 (G54-G59); default 1"},
            "safe_height": cam_value("The safe height above the stock; default 15 mm"),
            "machine": {"type": ["string", "null"], "description": "One of the design's machines by its id; null clears it; default the design's only machine"},
            "dialect": {"type": ["string", "null"], "description": "The post-processor's dialect instead of the machine's; null clears it"},
            "post": object("The post settings (units: millimetres or inches, canned_cycles, comments, ...)"),
            "probe_work_zero": {"type": "boolean", "description": "Probe the work zero before cutting"},
            "placement": {"type": ["array", "null"], "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": "Where the work zero is in machine coordinates [x, y, z] (mm), for the machine check; null clears it"},
            "visible": {"type": "boolean", "description": "Its toolpaths shown (the Browser's eye); default true"}
        },
        "additionalProperties": false
    })
}

/// An operation's fields (`cam_operation_add`'s `operation`,
/// `cam_operation_edit`'s `changes`).
fn operation_fields(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}; the optional values null to clear ({SECTION})"),
        "properties": {
            "name": text("Its name; default the kind's next (Pocket 2)"),
            "kind": {"type": "object", "description": "{\"type\": \"facing\"}, {\"type\": \"contour\", \"side\": \"outside\" | \"inside\" | \"on\"}, {\"type\": \"pocket\"}, {\"type\": \"drill\", \"peck\", \"chip_break\", \"canned_cycle\"}, {\"type\": \"adaptive\"} or {\"type\": \"rest\", \"from_tool\"}", "properties": {"type": {"type": "string", "enum": ["facing", "contour", "pocket", "drill", "adaptive", "rest"]}}, "required": ["type"]},
            "tool": {"type": "integer", "minimum": 1, "description": "The tool's number in the setup's machine's library"},
            "inputs": {"type": "array", "description": "What it cuts: {\"kind\": \"stock\"}, {\"kind\": \"face\", \"body\", \"face\"}, {\"kind\": \"region\", \"sketch\", \"region\"}, {\"kind\": \"edges\", \"body\", \"edges\"}, {\"kind\": \"curves\", \"sketch\", \"curves\"} or {\"kind\": \"point\", \"at\"}, each but the stock with its occurrence"},
            "depths": object("{top, bottom, step_down}, values"),
            "heights": object("{clearance, retract}, values"),
            "cutting": object("{spindle_rpm, feed, plunge_feed}, values; the tool's where left out"),
            "stepover": cam_value("The stepover (null clears it)"),
            "stock_to_leave": cam_value("The stock to leave (null clears it)"),
            "direction": choice(&["climb", "conventional"], "The cutting direction"),
            "spindle": choice(&["cw", "ccw"], "The spindle's direction"),
            "coolant": choice(&["off", "flood", "mist", "air"], "The coolant"),
            "entry": {"type": ["object", "null"], "description": "{\"kind\": \"plunge\"}, {\"kind\": \"ramp\", \"angle\"} or {\"kind\": \"helix\", \"radius\", \"angle\"}; null clears it"},
            "tabs": {"type": ["object", "null"], "description": "Holding tabs of a contour, {count, width, height}; null clears them"},
            "lead": {"type": ["object", "null"], "description": "Arcs into and out of a wall, {radius}; null clears it"},
            "suppressed": {"type": "boolean", "description": "Suppressed"}
        },
        "additionalProperties": false
    })
}

/// A CAM command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "cam_setup_add" => json!({
            "description": "Adds a CAM setup: one clamping of the stock. Result: setup (its id).",
            "properties": {
                "setup": setup_fields("The setup's fields; left out: every default")
            },
            "required": [],
            "examples": [{"cmd": "cam_setup_add", "setup": {"name": "Top", "safe_height": 10}}, {"cmd": "cam_setup_add"}]
        }),
        "cam_setup_edit" => json!({
            "description": "Changes a CAM setup's fields; {\"visible\"} alone shows or hides its toolpaths.",
            "properties": {
                "setup": setup_ref("The setup"),
                "changes": setup_fields("The fields to set")
            },
            "required": ["setup", "changes"],
            "examples": [{"cmd": "cam_setup_edit", "setup": 1, "changes": {"work_offset": 2, "dialect": null}}, {"cmd": "cam_setup_edit", "setup": "Top", "changes": {"visible": false}}]
        }),
        "cam_setup_delete" => json!({
            "description": "Deletes a CAM setup with its operations.",
            "properties": {"setup": setup_ref("The setup")},
            "required": ["setup"],
            "examples": [{"cmd": "cam_setup_delete", "setup": 1}]
        }),
        "cam_setup_move" => json!({
            "description": "Moves a CAM setup in the order.",
            "properties": {
                "setup": setup_ref("The setup"),
                "index": index("Its place in the order, from 0")
            },
            "required": ["setup", "index"],
            "examples": [{"cmd": "cam_setup_move", "setup": 2, "index": 0}]
        }),
        "cam_operation_add" => json!({
            "description": "Adds an operation to a CAM setup. Result: operation (its id).",
            "properties": {
                "setup": setup_ref("The setup"),
                "operation": operation_fields("The operation's fields: kind and tool needed"),
                "index": index("Its place in the setup's order; last when left out")
            },
            "required": ["setup", "operation"],
            "examples": [{"cmd": "cam_operation_add", "setup": 1, "operation": {"kind": {"type": "pocket"}, "tool": 1, "inputs": [{"kind": "region", "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}], "stepover": 2.5}}]
        }),
        "cam_operation_edit" => json!({
            "description": "Changes a CAM operation's fields.",
            "properties": {
                "operation": operation_id("The operation's id"),
                "changes": operation_fields("The fields to set")
            },
            "required": ["operation", "changes"],
            "examples": [{"cmd": "cam_operation_edit", "operation": 4, "changes": {"tool": 2, "tabs": null}}]
        }),
        "cam_operation_delete" => json!({
            "description": "Deletes a CAM operation.",
            "properties": {"operation": operation_id("The operation's id")},
            "required": ["operation"],
            "examples": [{"cmd": "cam_operation_delete", "operation": 4}]
        }),
        "cam_operation_move" => json!({
            "description": "Moves a CAM operation in its setup's order, or into another setup.",
            "properties": {
                "operation": operation_id("The operation's id"),
                "setup": setup_ref("Another setup to move it into; its own when left out"),
                "index": index("Its place in the setup's order, from 0")
            },
            "required": ["operation", "index"],
            "examples": [{"cmd": "cam_operation_move", "operation": 4, "index": 0}, {"cmd": "cam_operation_move", "operation": 4, "setup": 2, "index": 1}]
        }),
        "cam_machine_put" => json!({
            "description": "Puts a machine with its tool library into the design, replacing the machine of its id. Result: machine (its id).",
            "properties": {
                "machine": {
                    "type": "object",
                    "description": format!("The machine: id (letters, digits, _ and -), machine {{name, controller (grbl, grblhal, reprapfirmware, millenniumos, mach3, mach4, linuxcnc), travel {{min, max}} (mm, machine coordinates), spindle {{min_rpm, max_rpm, spin_up_seconds, nose_diameter, nose_length}}, max_feed, rapid_feed (mm/min), zones, tool_changer, park, lathe}}, tools {{tools}} ({SECTION}). A lathe has lathe {{turret_stations (1 to 99), chuck_diameter, chuck_length (mm), axes}} and a turning controller (fanuc_turning; cam_dialects lists them with lathe true); its travel and zones are in the lathe's axes: X the distance from the spindle axis (a radius, Y unused), Z along the spindle ({TURNING})"),
                    "properties": {
                        "id": {"type": "string", "pattern": "^[A-Za-z0-9_-]{1,64}$"},
                        "machine": {"type": "object"},
                        "tools": {"type": "object"},
                        "origin": {"type": "object"}
                    },
                    "required": ["id", "machine"]
                }
            },
            "required": ["machine"],
            "examples": [{"cmd": "cam_machine_put", "machine": {"id": "router", "machine": {
                "name": "Router", "controller": "grbl",
                "travel": {"min": [0, 0, -80], "max": [800, 600, 0]},
                "spindle": {"min_rpm": 8000, "max_rpm": 24000, "nose_diameter": 52, "nose_length": 60},
                "max_feed": 5000, "rapid_feed": 8000}}},
                {"cmd": "cam_machine_put", "machine": {"id": "lathe", "machine": {
                "name": "Lathe", "controller": "fanuc_turning",
                "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
                "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
                "max_feed": 5000, "rapid_feed": 20000,
                "lathe": {"turret_stations": 12, "chuck_diameter": 160, "chuck_length": 50}}}}]
        }),
        "cam_machine_delete" => json!({
            "description": "Deletes one of the design's machines; refused while a setup uses it.",
            "properties": {"machine": machine_id("The machine's id")},
            "required": ["machine"],
            "examples": [{"cmd": "cam_machine_delete", "machine": "router"}]
        }),
        "cam_tool_put" => json!({
            "description": "Puts a tool into a machine's tool library, replacing the tool of its number. Result: tool (as kept, in mm and degrees).",
            "properties": {
                "machine": machine_id("The machine's id"),
                "tool": {
                    "type": "object",
                    "description": format!("The tool; lengths and angles as values with units ({SECTION}). Milling tools on a mill, lathe tools on a lathe: the number is then the turret station (1 to 99), diameter the holder's size (a boring bar's diameter), flute_length the deepest the insert cuts (4 mm, a grooving insert's 5, a parting blade's 25 when left out), length the overhang ({TURNING})"),
                    "properties": {
                        "number": {"type": "integer", "minimum": 1, "maximum": 999, "description": "Its number"},
                        "name": text("Its name"),
                        "shape": {"type": "object", "description": "{\"kind\": \"flat\"}, {\"kind\": \"ball\"}, {\"kind\": \"bull\", \"corner_radius\"}, {\"kind\": \"drill\", \"point_angle\"}, {\"kind\": \"chamfer\", \"angle\", \"tip_diameter\"} or {\"kind\": \"engraver\", \"angle\", \"tip_diameter\"} on a mill; {\"kind\": \"turning\", \"nose_radius\"}, {\"kind\": \"boring\", \"nose_radius\"}, {\"kind\": \"grooving\", \"width\", \"nose_radius\", \"inside\"}, {\"kind\": \"parting\", \"width\"} or {\"kind\": \"threading\", \"angle\" (60), \"inside\"} on a lathe", "properties": {"kind": {"type": "string", "enum": ["flat", "ball", "bull", "drill", "chamfer", "engraver", "turning", "boring", "grooving", "parting", "threading"]}}, "required": ["kind"]},
                        "diameter": cam_value("Its cutting diameter"),
                        "shank_diameter": cam_value("Its shank's diameter"),
                        "flute_length": cam_value("Its flute length; three diameters when left out"),
                        "length": cam_value("Its stick-out; the flute length and 5 mm when left out"),
                        "flutes": {"type": "integer", "minimum": 1, "description": "Its number of flutes"},
                        "holder": text("Its holder"),
                        "cutting": object("Its default {spindle_rpm, feed, plunge_feed}, values (a feed is a length per minute)")
                    },
                    "required": ["number", "shape", "diameter"],
                    "additionalProperties": false
                }
            },
            "required": ["machine", "tool"],
            "examples": [{"cmd": "cam_tool_put", "machine": "router", "tool": {"number": 1, "name": "6 mm flat", "shape": {"kind": "flat"}, "diameter": 6, "flutes": 2, "cutting": {"spindle_rpm": 18000, "feed": 1200, "plunge_feed": 300}}},
                {"cmd": "cam_tool_put", "machine": "lathe", "tool": {"number": 2, "shape": {"kind": "parting", "width": "3 mm"}, "diameter": "20 mm"}}]
        }),
        "cam_tool_delete" => json!({
            "description": "Deletes a tool from a machine's tool library.",
            "properties": {
                "machine": machine_id("The machine's id"),
                "number": {"type": "integer", "minimum": 0, "description": "The tool's number"}
            },
            "required": ["machine", "number"],
            "examples": [{"cmd": "cam_tool_delete", "machine": "router", "number": 1}]
        }),
        "cam_generate" => json!({
            "description": "Computes the toolpaths of a setup (every setup when left out) and records each operation's fingerprint. Result: setups (each with its operations' state, cut_length, duration, moves and error, and the machine check's findings).",
            "properties": {
                "setup": setup_ref("The setup; every setup when left out")
            },
            "required": [],
            "examples": [{"cmd": "cam_generate"}, {"cmd": "cam_generate", "setup": 1}],
            "restrictions": ["Toolpaths are not saved; the cam_toolpaths and cam_post queries compute them again."]
        }),
        _ => return None,
    };
    Some(spec)
}
