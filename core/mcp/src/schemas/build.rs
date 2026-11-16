// SPDX-License-Identifier: MIT
//! The JSON Schemas of the BUILD tab's commands (mitcad#1918): the
//! `BuildCommand` variants of `core/model-api/src/build.rs` with their
//! inputs in `core/model-build/src/place.rs` (`PlaceInput`, `EditInput`),
//! `connect.rs` (`ConnectInput`, `PanelInput`) and `swap.rs`
//! (`SwapInput`), written as [`super`] writes the others. MCP permits
//! some of them only in their model form (`classes.rs`,
//! `command_class_with`): a part from a source the design has.

use serde_json::{Value, json};

use super::{flag, id, ids, object, text, value_input};

/// The commands with a schema here, in the enum's order.
pub(super) const COMMANDS: &[&str] = &[
    "build_place",
    "build_edit",
    "build_connect",
    "build_panel",
    "build_remove",
    "build_swap_fasteners",
    "build_upgrade",
];

/// Where BUILD's reference describes the fields.
const REFERENCE: &str =
    "model_reference {\"section\": \"BUILD: profile assemblies\"} (its Commands and Placement)";

/// MCP's form of a placing command: a source the design has, no library
/// read.
const SOURCE_ONLY: &str = "MCP runs it only with part.source, a source the design has (\"S1\": the library_parts query lists them); a part read from a library (part.library, url, rev) is refused.";

/// The part a command places (`PartInput`).
fn part(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}: {{\"source\": \"S<n>\", \"config\"}} reuses a source of the design (a library part is {{\"library\", \"url\", \"rev\", \"component\", \"config\"}}, which MCP refuses)"),
        "properties": {
            "source": {"type": "string", "pattern": "^S[0-9]+$", "description": "A source of the design"},
            "config": text("Its configuration row (a size); left out: the source's default"),
            "component": text("A library's component"),
            "library": text("A library"),
            "url": text("The library's address"),
            "rev": text("The library's version"),
            "parameters": object("Values of its options by name (expressions of the design's parameters); the command's own parameters win")
        },
        "additionalProperties": false
    })
}

/// Where a part goes (`PlaceAt`).
fn at(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}; the fields are in the reference's Placement"),
        "properties": {
            "occurrence": text("A path from the BUILD host (uids or names); \"\" for its own geometry"),
            "geometry": text("What was picked: a face or a frame of a placed part, or for the first part a plane, a point or a face; left out: grounded at point"),
            "point": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": "Where it was picked, in the host's coordinates, mm"},
            "place": {"type": "string", "enum": ["auto", "side", "end"], "default": "auto", "description": "A side or an end; auto: the pick decides"},
            "offset": value_input("On a side: the offset from `from`; on a turned end: the new part's end from the target's outer side"),
            "from": {"type": "string", "enum": ["a", "b", "middle", "gap"], "default": "a", "description": "What offset is measured from on a side"},
            "direction": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": "On an end: the new part's axis; for the first part the axis it runs along (+Z)"},
            "snap": {"type": "number", "description": "The app's least snap distance, mm"}
        },
        "additionalProperties": false
    })
}

/// A length: a value, or bound to what the part's ray meets.
fn length(description: &str) -> Value {
    json!({
        "description": format!("{description}: a value, or {{\"bind\": {{\"occurrence\", \"geometry\"}}, \"mode\": \"face\"/\"centre\"/\"through\", \"add\"}}, or to a picked face {{\"mode\": \"to_face\", \"face\": {{\"occurrence\", \"geometry\"}}, \"offset\"}}"),
        "type": ["number", "string", "object"]
    })
}

fn parameters() -> Value {
    object("Other source parameters by name: values or expressions")
}

/// A BUILD command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "build_place" => json!({
            "description": format!("Places a part: a profile on a side or an end of another, a mate on a frame, or the first part on a plane, point or face. Result: component, occurrence, name, features, joint, bound, adds, warnings, place. {REFERENCE}."),
            "properties": {
                "part": part("The part"),
                "at": at("Where it goes"),
                "rotate": value_input("Its turn about its axis: 0, 90, 180 or 270 deg"),
                "flip": flag(false, "The other end first"),
                "length": length("Its length; left out: the source's default"),
                "mitre": {"type": "object", "description": "A mitre at the corner: {\"angle\", \"cut\": \"new\"/\"existing\"/\"shared\"/\"none\"}", "properties": {"angle": value_input("The mitre's angle"), "cut": {"type": "string", "enum": ["new", "existing", "shared", "none"], "default": "new"}}, "additionalProperties": false},
                "tilt": value_input("On a side: the lean along the host, -80 to 80 deg, positive towards its end B"),
                "parameters": parameters(),
                "name": text("Its name"),
                "to": object("A member by two points: where it ends (at.point its start); the reference's Members by two points"),
                "lie": object("How a member by two points lies about its axis"),
                "spacing": object("The member repeated as a spacing set; the reference's Spacing sets"),
                "grade": text("The part's own grade (a timber member's)"),
                "along": object("A member along a straight edge, lying on a face beside it; the reference's Members along an edge")
            },
            "required": ["part"],
            "examples": [
                {"cmd": "build_place", "part": {"source": "S1", "config": "48x148"}, "at": {"point": [200, 0, 100]}},
                {"cmd": "build_place", "part": {"source": "S1"}, "at": {"occurrence": "C3", "place": "side", "offset": 100, "from": "middle"}, "rotate": "90 deg"}
            ],
            "restrictions": [SOURCE_ONLY]
        }),
        "build_edit" => json!({
            "description": format!("Edits a placed part. Result: changed (offset, from, rotate, flip, detached, length, bound, tilt, ends, config, parameters, machining). {REFERENCE}."),
            "properties": {
                "component": id("The part's component (uid or name)"),
                "at": {"type": "object", "description": "Its place again: offset, from (a, b, middle, gap), point (a member by two points: its start)", "properties": {"offset": value_input("The offset"), "from": {"type": "string", "enum": ["a", "b", "middle", "gap"]}, "point": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}}}, "additionalProperties": false},
                "rotate": value_input("Its turn about its axis"),
                "flip": {"type": "boolean", "description": "Which end meets"},
                "length": length("Its length: a value detaches, a bind binds"),
                "ends": {"type": "object", "description": "Its end cuts: {\"a\": {\"angle\", \"tilt\"}, \"b\": ...}; null squares an end, left out keeps it"},
                "tilt": value_input("A part on a side: its tilt again; 0 squares it"),
                "config": text("Its configuration row"),
                "parameters": parameters(),
                "machining": {"type": "array", "description": "The part's own end machining, replacing the whole list ([] takes it away): [{\"end\": \"a\"/\"b\", \"hole\": {\"diameter\", \"depth\", \"thread\"}}]", "items": {"type": "object"}},
                "spacing": object("The spacing set it is in: its fields changed"),
                "grade": {"type": ["string", "null"], "description": "Its own grade; null clears it"},
                "cuts": {"type": "string", "enum": ["refit"], "description": "refit: its joinery cut anew from where the members are"},
                "to": object("A member by two points: its end again"),
                "lie": object("A member by two points: its lie again"),
                "frame": object("The timber frame it is in, laid out again with these fields"),
                "along": object("A member along an edge: its edge, face and offsets")
            },
            "required": ["component"],
            "examples": [
                {"cmd": "build_edit", "component": "C2", "at": {"offset": 100, "from": "a"}},
                {"cmd": "build_edit", "component": "C2", "length": 600, "ends": {"a": null}}
            ],
            "restrictions": ["MCP refuses the forms that read a library: with frame (a timber frame laid out again) or with spacing.fastener."]
        }),
        "build_connect" => json!({
            "description": format!("Connects the members of a junction with a connector or fastener, or cuts joinery. Result: component, occurrence, adds, warnings (and cuts with joint). {REFERENCE}."),
            "properties": {
                "junction": id("A junction (an id from the build_junctions query); a post base names end instead"),
                "part": part("The connector or wood fastener; left out with joint"),
                "fasteners": flag(true, "Its fasteners too"),
                "machining": flag(true, "The holes it needs"),
                "name": text("Its name"),
                "count": {"type": "integer", "minimum": 0, "description": "How many screws or nails (default 2)"},
                "side": {"type": "string", "enum": ["x+", "x-", "y+", "y-"], "description": "An angle bracket's side of member a"},
                "end": object("A post base's member end"),
                "joint": {"type": "object", "description": "Joinery instead of a part", "properties": {"kind": {"type": "string", "enum": ["housing", "notch", "half_lap"]}, "depth": value_input("The cut's depth"), "on": {"type": "string", "enum": ["a", "b"], "description": "The member a notch is cut in"}}, "required": ["kind"], "additionalProperties": false}
            },
            "required": [],
            "examples": [
                {"cmd": "build_connect", "junction": "C5:end_a|C3:face_xp", "part": {"source": "S2"}},
                {"cmd": "build_connect", "junction": "C7:end_b|C6:side", "joint": {"kind": "half_lap"}}
            ],
            "restrictions": [format!("{SOURCE_ONLY} Joinery (joint) reads no library.")]
        }),
        "build_panel" => json!({
            "description": format!("Places a panel on a face or a slot frame of the frame's profiles. Result: component, occurrence, bound, fits, adds. {REFERENCE}."),
            "properties": {
                "at": at("Where it goes: a face or a slot frame of one of the frame's profiles"),
                "part": part("The panel"),
                "thickness": value_input("Its thickness, for a part whose thickness is a parameter"),
                "parameters": parameters(),
                "name": text("Its name"),
                "faces": {"type": "array", "description": "A sheet on a face: more faces it covers, each {\"occurrence\", \"geometry\"}", "items": {"type": "object"}}
            },
            "required": ["at", "part"],
            "examples": [
                {"cmd": "build_panel", "at": {"occurrence": "C1", "point": [0, 0, 300]}, "part": {"source": "S3"}, "thickness": 6}
            ],
            "restrictions": [SOURCE_ONLY]
        }),
        "build_remove" => json!({
            "description": format!("Removes a placed part and what it adds. Result: removed (components, features, detached, freed), notices. {REFERENCE}."),
            "properties": {
                "component": id("The part's component (uid or name)"),
                "set": flag(false, "The whole spacing set it is in")
            },
            "required": ["component"],
            "examples": [{"cmd": "build_remove", "component": "C2"}]
        }),
        "build_swap_fasteners" => json!({
            "description": "Swaps fasteners in place for another fastener. Result: the swapped components, their sizes, warnings and designation. model_reference {\"section\": \"BUILD: choosing and swapping fasteners\"}.",
            "properties": {
                "fasteners": ids("The fasteners to swap (components)"),
                "part": part("The fastener they become; its config the size (left out: a wood fastener sized from the members it joins, another its default row)")
            },
            "required": ["fasteners", "part"],
            "examples": [{"cmd": "build_swap_fasteners", "fasteners": ["C41", "C42"], "part": {"source": "S4", "config": "M6x16"}}],
            "restrictions": [SOURCE_ONLY]
        }),
        "build_upgrade" => json!({
            "description": format!("Places again by the current rule the slot connectors and fasteners the first placing rule placed. Result: upgraded, count. {REFERENCE}."),
            "properties": {},
            "required": [],
            "examples": [{"cmd": "build_upgrade"}]
        }),
        _ => return None,
    };
    Some(spec)
}
