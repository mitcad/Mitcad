// SPDX-License-Identifier: MIT
//! The JSON Schemas of construction sets (mitcad#2514): the commands of
//! `core/model-api/src/construction_sets.rs`, written from their fields as
//! [`super`] writes the others. The drift test
//! `core/model-tests/api/src/mcp_schema_tests.rs` checks them against the
//! API.

use serde_json::{Value, json};

use super::{flag, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "add_construction_set",
    "activate_construction_set",
    "deactivate_construction_set",
    "set_construction_set_members",
    "rename_construction_set",
    "delete_construction_set",
];

/// A member: a sketch or construction feature, or a body.
fn members(description: &str) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": {
            "type": "object",
            "description": "A sketch (planar or 3D) or construction geometry of the set's component (feature: uid or name), or a body of it at the marker (body: uid or name)",
            "properties": {
                "feature": text("A feature's uid (F5) or name"),
                "body": text("A body's uid (F7.b0) or name")
            },
            "additionalProperties": false
        }
    })
}

/// The schema of a command of this family, None for another.
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "add_construction_set" => json!({
            "description": "Adds a named construction set to a component: a Browser group that collects the sketches, 3D sketches, construction geometry and surface bodies made in that component while the set is active. Organisation only: the items keep their component, timeline place and references. Activating it also activates its component. One undo step that recomputes nothing. Result: name, component, active.",
            "properties": {
                "name": text("Its name, unique in the design; the next free ConstructionSet<n> when left out"),
                "component": text("Its component (uid or name); the active component when left out"),
                "activate": flag(true, "Make it the active set (and its component the active component)")
            },
            "required": [],
            "examples": [{"cmd": "add_construction_set", "name": "Guides"}]
        }),
        "activate_construction_set" => json!({
            "description": "Makes a construction set the active one, and its component the active component: new sketches, construction geometry and surface bodies of that component join it. The set active before is not any more. Result: name, component.",
            "properties": {"name": text("The set")},
            "required": ["name"],
            "examples": [{"cmd": "activate_construction_set", "name": "Guides"}]
        }),
        "deactivate_construction_set" => json!({
            "description": "No construction set is active any more: new items go to their usual Browser folders. Result: name (the set that was active, or null).",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "deactivate_construction_set"}]
        }),
        "set_construction_set_members" => json!({
            "description": "Adds items to a construction set (each leaves the set it was in) and takes others out of it; the items stay in the design. Result: name, members (the count).",
            "properties": {
                "name": text("The set"),
                "add": members("Items to add"),
                "remove": members("Items to take out")
            },
            "required": ["name"],
            "examples": [{"cmd": "set_construction_set_members", "name": "Guides", "add": [{"feature": "F3"}]},
                         {"cmd": "set_construction_set_members", "name": "Guides", "remove": [{"body": "F7.b0"}]}]
        }),
        "rename_construction_set" => json!({
            "description": "Renames a construction set; a name another set has is refused. Result: name.",
            "properties": {"name": text("The set"), "new_name": text("Its new name")},
            "required": ["name", "new_name"],
            "examples": [{"cmd": "rename_construction_set", "name": "ConstructionSet1", "new_name": "Guides"}]
        }),
        "delete_construction_set" => json!({
            "description": "Deletes a construction set; its items stay and go back to their usual Browser folders.",
            "properties": {"name": text("The set")},
            "required": ["name"],
            "examples": [{"cmd": "delete_construction_set", "name": "Guides"}]
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
