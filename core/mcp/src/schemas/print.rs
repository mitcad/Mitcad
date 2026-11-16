// SPDX-License-Identifier: MIT
//! The JSON Schemas of the print setting commands (mitcad#1918): print
//! profiles (`core/model-api/src/print_profiles.rs`, `ProfileChange` of
//! `core/model/src/print_profile.rs`), purge volumes
//! (`purge_volumes.rs`) and filament changes (`filament_changes.rs`),
//! written as [`super`] writes the others.

use serde_json::{Value, json};

use super::{choice, id, number, object, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "create_print_profile",
    "edit_print_profile",
    "delete_print_profile",
    "set_purge_volumes",
    "set_filament_changes",
];

/// The libraries a command writes in.
const WRITTEN: &[&str] = &["user", "project"];

fn project() -> Value {
    text("The project's folder, when the design is in one (the project library's place)")
}

/// The fields of `create_print_profile` and `edit_print_profile`.
fn profile_fields() -> Value {
    json!({
        "id": text("create: its id (from its name, else printer<n> or process<n>, when left out); edit: the profile changed (needed: the API refuses an edit without it)"),
        "name": text("Its name (Printer <n> or Process <n> when left out on create)"),
        "kind": choice(&["printer", "process"], "create_print_profile: what it sets (when not based on one); with library: which of the library's two items of the id"),
        "values": object("Print Printer or Print Process values by key in the key map's JSON form (layer_height; model_reference {\"query\": \"print_keys\"}); in edit_print_profile a key's null removes it"),
        "slicer": object("A slicer's own keys by slicer id ({\"<slicer>\": {\"<key>\": value}}); a key's null removes it"),
        "based_on": {
            "description": "create_print_profile only: the profile whose fields it starts from: a design profile's id, or a library's item {\"library\": \"base\" | \"user\" | \"project\", \"id\"} (Duplicate)",
            "oneOf": [
                {"type": "string"},
                {"type": "object", "properties": {"library": {"type": "string"}, "id": {"type": "string"}}, "required": ["library", "id"], "additionalProperties": false}
            ]
        },
        "version": {"type": ["integer", "null"], "minimum": 0, "description": "The library version it was copied at (null: none)"},
        "origin": {"type": ["object", "null"], "description": "The library item it was copied from, {library, id, version, declined} (null: none)"},
        "library": choice(WRITTEN, "user or project: the command works on that library's item instead of the design's profile"),
        "project": project(),
        "choose": {"type": "boolean", "description": "create_print_profile in the design: the design (the root) chooses it in the same undo step"}
    })
}

/// A command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "create_print_profile" => json!({
            "description": "Adds a printer or process profile to the design (or, with library, to that library). Result: id (with library also library, version, path).",
            "properties": profile_fields(),
            "required": [],
            "examples": [{"cmd": "create_print_profile", "kind": "process", "name": "Fine", "values": {"layer_height": 0.12}}]
        }),
        "edit_print_profile" => json!({
            "description": "Changes the given fields of the design's profile (or, with library, that library's item). Result: {} (with library: version, path).",
            "properties": profile_fields(),
            "required": [],
            "examples": [{"cmd": "edit_print_profile", "id": "process1", "values": {"layer_height": 0.16}}],
            "restrictions": ["id is needed; based_on and choose are create_print_profile's."]
        }),
        "delete_print_profile" => json!({
            "description": "Deletes the design's profile (or, with library, that library's item). Result: owners (the components, occurrences and bodies that chose it; with library: path, kept, removed).",
            "properties": {
                "id": id("The profile"),
                "library": choice(WRITTEN, "user or project: that library's item is deleted"),
                "kind": choice(&["printer", "process"], "With library: which, when the library has a printer and a process of the id"),
                "project": project()
            },
            "required": ["id"],
            "examples": [{"cmd": "delete_print_profile", "id": "process1"}]
        }),
        "set_purge_volumes" => json!({
            "description": "Sets the purge volumes of pairs of materials by hand, or resets pairs to their computed volumes.",
            "properties": {
                "volumes": {
                    "type": "array",
                    "minItems": 1,
                    "description": "One entry per pair",
                    "items": {
                        "type": "object",
                        "properties": {
                            "from": text("The material printed before (a built-in or design material's id)"),
                            "to": text("The material printed after"),
                            "volume": {"type": ["number", "null"], "minimum": 0, "description": "The volume, mm³; null or left out resets the pair to its computed volume"}
                        },
                        "required": ["from", "to"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["volumes"],
            "examples": [{"cmd": "set_purge_volumes", "volumes": [{"from": "pla", "to": "abs", "volume": 120}, {"from": "abs", "to": "pla"}]}]
        }),
        "set_filament_changes" => json!({
            "description": "Sets the whole schedule of filament changes and pauses of the design or of a print plate. Result: changes (the schedule as kept, sorted upwards).",
            "properties": {
                "plate": text("A print plate by id or name; the design's own schedule when left out"),
                "changes": {
                    "type": "array",
                    "description": "The whole schedule; [] takes it away",
                    "items": {
                        "type": "object",
                        "properties": {
                            "height": number("Where: mm above the print bed"),
                            "at": {"type": ["string", "object"], "description": "Instead of height: a plane, a construction plane (\"F5\"), an origin plane (xy, xz, yz) or a planar face {\"body\", \"face\"} (model_reference {\"section\": \"Filament changes at planes and faces\"})"},
                            "body": text("With at a plane: the body (F2.b0) the height is measured from"),
                            "kind": choice(&["change", "pause"], "A change to another filament (the default) or a pause"),
                            "material": text("change: the material printed from there"),
                            "appearance": text("change: its appearance (else the material's default)"),
                            "message": text("pause: the message shown")
                        },
                        "additionalProperties": false
                    }
                }
            },
            "required": ["changes"],
            "examples": [{"cmd": "set_filament_changes", "changes": [{"height": 4, "kind": "pause", "message": "Insert the magnets"}]},
                         {"cmd": "set_filament_changes", "plate": "Vase", "changes": []}]
        }),
        _ => return None,
    };
    Some(spec)
}
