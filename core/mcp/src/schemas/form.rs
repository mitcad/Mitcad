// SPDX-License-Identifier: MIT
//! The JSON Schemas of the Form commands (mitcad#1918): every permitted
//! `form.*` command, which makes a Form from a primitive cage or edits its
//! control cage, written from their fields in `core/model-api/src/form.rs`
//! (with `Cage` of `core/subdiv/src/cage.rs`) as [`super`] writes the
//! others. The drift test `core/model-tests/api/src/mcp_schema_tests.rs`
//! checks them against the API like the others.

use serde_json::{Value, json};

use super::{feature_uid, flag, number, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "form.create",
    "form.move",
    "form.rotate",
    "form.scale",
    "form.set_positions",
    "form.extrude",
    "form.insert_loop",
    "form.delete_loop",
    "form.delete_faces",
    "form.fill_hole",
    "form.bridge",
    "form.crease",
    "form.symmetry",
    "form.set_cage",
];

/// What every edit's ids are.
const IDS: &str = "ids in the whole cage (a negative id is the mirror image of a stored one on a symmetric cage; form_cage lists them)";

fn form() -> Value {
    feature_uid("The Form (its uid)")
}

fn vec3(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": description})
}

fn vertices(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "description": format!("{description}: {IDS}")})
}

fn faces(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "description": format!("{description}: {IDS}")})
}

fn edge(description: &str) -> Value {
    json!({"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "integer"},
           "description": format!("{description}: [a, b], its two vertices' {IDS}")})
}

fn edges(description: &str) -> Value {
    json!({"type": "array", "items": edge("An edge"), "description": description})
}

fn preview() -> Value {
    flag(
        false,
        "Nothing changes: the answer has the edited cage and its limit mesh (3 steps), for an editor's drag",
    )
}

/// A control cage as the Form's definition has it.
fn cage(description: &str) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}: vertices [{{id, at [x, y, z] mm}}], faces [{{id, vertices (counter-clockwise from outside, three or more)}}], creases [{{edge [a, b], sharpness}}], corners, symmetry {{origin, normal}}, next_vertex, next_face (model_reference {{\"feature\": \"form\"}})"),
        "properties": {
            "vertices": {"type": "array", "items": {"type": "object", "properties": {"id": {"type": "integer", "minimum": 1}, "at": vec3("mm")}, "required": ["id", "at"], "additionalProperties": false}},
            "faces": {"type": "array", "items": {"type": "object", "properties": {"id": {"type": "integer", "minimum": 1}, "vertices": {"type": "array", "minItems": 3, "items": {"type": "integer"}}}, "required": ["id", "vertices"], "additionalProperties": false}},
            "creases": {"type": "array", "items": {"type": "object"}},
            "corners": {"type": "array", "items": {"type": "integer"}},
            "symmetry": {"type": "object", "properties": {"origin": vec3("A point of the plane"), "normal": vec3("Towards the stored half")}, "required": ["origin", "normal"], "additionalProperties": false},
            "next_vertex": {"type": "integer", "minimum": 0},
            "next_face": {"type": "integer", "minimum": 0}
        },
        "required": ["vertices", "faces"],
        "additionalProperties": false
    })
}

/// A symmetry plane: an axis name, or a point and a normal.
fn plane(description: &str) -> Value {
    json!({
        "description": format!("{description}: \"x\", \"y\" or \"z\" (the plane square to that axis through center), or {{\"origin\": [x, y, z], \"normal\": [x, y, z]}} (the half on the side the normal points to is kept)"),
        "oneOf": [
            {"type": "string", "enum": ["x", "y", "z"]},
            {"type": "object", "properties": {"origin": vec3("A point of the plane"), "normal": vec3("Its normal")}, "required": ["origin", "normal"], "additionalProperties": false}
        ]
    })
}

/// A tetrahedron's cage, for the examples.
fn tetrahedron() -> Value {
    json!({
        "vertices": [{"id": 1, "at": [0, 0, 0]}, {"id": 2, "at": [10, 0, 0]}, {"id": 3, "at": [0, 10, 0]}, {"id": 4, "at": [0, 0, 10]}],
        "faces": [{"id": 1, "vertices": [1, 3, 2]}, {"id": 2, "vertices": [1, 2, 4]}, {"id": 3, "vertices": [1, 4, 3]}, {"id": 4, "vertices": [2, 3, 4]}]
    })
}

/// A Form command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "form.create" => json!({
            "description": "Adds a Form at the timeline marker: a body whose surface is the Catmull-Clark limit surface of a control cage made from a primitive (a solid when the cage is closed, a surface body when open). Result: uid, name, vertices, faces (the stored half's counts), closed, error when the Form failed.",
            "properties": {
                "primitive": {
                    "type": "object",
                    "description": "The cage it starts from, by its type: box {size [x, y, z], segments [nx, ny, nz] (default 1 each)}, cylinder about z {diameter, height, segments (round, default 8), rows (default 1)}, sphere {diameter, segments (default 2)}, plane facing +z {size [x, y], segments [nx, ny]}, face {body, face, thickness} (a planar face's outline; with thickness a slab), cage {cage} (a cage given whole), kept_cage {body} (the cage kept with an imported body); sizes in mm",
                    "properties": {
                        "type": {"type": "string", "enum": ["box", "cylinder", "sphere", "plane", "face", "cage", "kept_cage"]},
                        "size": {"type": "array", "items": {"type": "number"}},
                        "segments": {"type": ["integer", "array"]},
                        "diameter": {"type": "number"},
                        "height": {"type": "number"},
                        "rows": {"type": "integer"},
                        "body": {"type": "string"},
                        "face": {"type": "string"},
                        "thickness": {"type": "number"},
                        "cage": cage("The cage")
                    },
                    "required": ["type"],
                    "additionalProperties": false
                },
                "center": json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "default": [0, 0, 0],
                                 "description": "Where a box, cylinder, sphere or plane is centred, mm (a face's outline stays where it is)"}),
                "symmetry": plane("A symmetry plane; with an axis name the primitive's segments along it are made even so that vertices lie on it"),
                "operation": {"type": "string", "enum": ["new_body", "join", "cut", "intersect", "new_component"], "default": "new_body",
                              "description": "What it does with the bodies (an open cage: new_body only)"},
                "participants": {"type": "array", "items": {"type": "string"}, "description": "The bodies (F2.b0) a join, cut or intersect acts on"},
                "name": text("Its name; default the next Form<n>")
            },
            "required": ["primitive"],
            "examples": [
                {"cmd": "form.create", "primitive": {"type": "box", "size": [40, 30, 20], "segments": [2, 2, 1]}},
                {"cmd": "form.create", "primitive": {"type": "cylinder", "diameter": 30, "height": 50}, "symmetry": "x", "name": "Handle"}
            ]
        }),
        "form.move" => json!({
            "description": "Moves cage vertices, edges and faces by an offset. Result: moved (the stored vertices that moved); undo step Move in <Form>.",
            "properties": {
                "form": form(),
                "vertices": vertices("Vertices"),
                "edges": edges("Edges"),
                "faces": faces("Faces"),
                "by": vec3("The offset [x, y, z], mm"),
                "preview": preview()
            },
            "required": ["form", "by"],
            "examples": [{"cmd": "form.move", "form": "F1", "faces": [3], "by": [0, 0, 5]}]
        }),
        "form.rotate" => json!({
            "description": "Turns cage vertices, edges and faces about an axis. Result: moved.",
            "properties": {
                "form": form(),
                "vertices": vertices("Vertices"),
                "edges": edges("Edges"),
                "faces": faces("Faces"),
                "axis": vec3("The axis' direction"),
                "origin": vec3("A point of the axis; left out: the selection's centre"),
                "angle": number("Radians, right-handed about the axis"),
                "preview": preview()
            },
            "required": ["form", "axis", "angle"],
            "examples": [{"cmd": "form.rotate", "form": "F1", "faces": [3], "axis": [0, 0, 1], "angle": 0.5}]
        }),
        "form.scale" => json!({
            "description": "Scales cage vertices, edges and faces about a centre. Result: moved.",
            "properties": {
                "form": form(),
                "vertices": vertices("Vertices"),
                "edges": edges("Edges"),
                "faces": faces("Faces"),
                "factor": number("One factor for every axis (not zero)"),
                "factors": vec3("Instead of factor: factors along x, y and z (not zero)"),
                "center": vec3("The centre; left out: the selection's centre"),
                "preview": preview()
            },
            "required": ["form"],
            "examples": [{"cmd": "form.scale", "form": "F1", "faces": [3], "factor": 1.5}]
        }),
        "form.set_positions" => json!({
            "description": "Puts cage vertices where they were dragged; undo step Drag in <Form>.",
            "properties": {
                "form": form(),
                "positions": {"type": "array", "items": {"type": "object",
                    "properties": {"vertex": {"type": "integer", "description": format!("A vertex: {IDS}")}, "at": vec3("Where it goes, mm")},
                    "required": ["vertex", "at"], "additionalProperties": false}, "description": "Where each vertex goes"},
                "preview": preview()
            },
            "required": ["form", "positions"],
            "examples": [{"cmd": "form.set_positions", "form": "F1", "positions": [{"vertex": 2, "at": [12, 0, 0]}]}]
        }),
        "form.extrude" => json!({
            "description": "Extrudes cage faces along their normals. Result: vertices, faces (the side faces); the faces moved keep their ids.",
            "properties": {
                "form": form(),
                "faces": faces("The faces"),
                "distance": number("mm along the faces' normals; negative: inwards"),
                "preview": preview()
            },
            "required": ["form", "faces", "distance"],
            "examples": [{"cmd": "form.extrude", "form": "F1", "faces": [3], "distance": 10}]
        }),
        "form.insert_loop" => json!({
            "description": "Inserts an edge loop across the quads on either side of an edge. Result: vertices, faces (the halves with new ids), edges (the new edges).",
            "properties": {
                "form": form(),
                "edge": edge("An edge the loop crosses"),
                "at": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "description": "Where along the edge, from its first vertex"},
                "preview": preview()
            },
            "required": ["form", "edge"],
            "examples": [{"cmd": "form.insert_loop", "form": "F1", "edge": [1, 2], "at": 0.25}]
        }),
        "form.delete_loop" => json!({
            "description": "Deletes the edge loop through an edge. Result: removed_vertices, removed_faces.",
            "properties": {
                "form": form(),
                "edge": edge("An edge of the loop"),
                "preview": preview()
            },
            "required": ["form", "edge"],
            "examples": [{"cmd": "form.delete_loop", "form": "F1", "edge": [1, 2]}]
        }),
        "form.delete_faces" => json!({
            "description": "Deletes cage faces; the cage opens there. Result: removed_faces, removed_vertices.",
            "properties": {
                "form": form(),
                "faces": faces("The faces"),
                "preview": preview()
            },
            "required": ["form", "faces"],
            "examples": [{"cmd": "form.delete_faces", "form": "F1", "faces": [6]}]
        }),
        "form.fill_hole" => json!({
            "description": "Fills the opening a boundary edge of the cage lies on, so that the cage closes there: one face, or with quads and an even number of edges round it a grid of quads. On a symmetric cage the mirrored opening is filled too; an opening across the symmetry plane is refused. Result: faces, vertices (the new ones), closed; undo step Fill Hole in <Form>.",
            "properties": {
                "form": form(),
                "edge": edge("An edge of the opening (boundary true in form_cage)"),
                "quads": flag(false, "A grid of quads, as edge loops across one face would leave, when the opening has an even number of edges"),
                "preview": preview()
            },
            "required": ["form", "edge"],
            "examples": [{"cmd": "form.fill_hole", "form": "F1", "edge": [5, 6]}, {"cmd": "form.fill_hole", "form": "F1", "edge": [5, 6], "quads": true}]
        }),
        "form.bridge" => json!({
            "description": "Joins two openings, or two runs of open edges of as many edges each, with a strip of quads. On a symmetric cage the mirror image is bridged too. Result: faces, vertices (the new ones), closed; undo step Bridge in <Form>.",
            "properties": {
                "form": form(),
                "edges": edges("The boundary edges of both sides; the runs they make along the boundary are the two sides"),
                "loops": flag(false, "Each edge stands for the whole opening it lies on: one edge a side is enough"),
                "segments": {"type": "integer", "minimum": 1, "maximum": 100, "default": 1, "description": "Rows of quads across the strip"},
                "preview": preview()
            },
            "required": ["form", "edges"],
            "examples": [{"cmd": "form.bridge", "form": "F1", "edges": [[9, 10], [101, 102]], "loops": true, "segments": 2}]
        }),
        "form.crease" => json!({
            "description": "Creases cage edges and makes vertices corners, or smooths them again; undo step Crease in <Form>.",
            "properties": {
                "form": form(),
                "edges": edges("The edges creased"),
                "vertices": vertices("The vertices made corners"),
                "sharpness": {"type": ["number", "null"], "minimum": 0, "description": "Left out or null: sharp for good; a number: sharp for that many subdivision steps; 0: smooth again (the vertices no longer corners)"},
                "preview": preview()
            },
            "required": ["form"],
            "examples": [{"cmd": "form.crease", "form": "F1", "edges": [[1, 2], [2, 3]]}, {"cmd": "form.crease", "form": "F1", "vertices": [5], "sharpness": 0}]
        }),
        "form.symmetry" => json!({
            "description": "Makes the cage symmetric about a plane, or takes the symmetry away; refused when a face crosses the plane.",
            "properties": {
                "form": form(),
                "plane": {"description": "The plane; null (or left out): none, the mirrored half becomes the cage's own", "anyOf": [plane("The plane"), {"type": "null"}]},
                "center": vec3("Where an axis name puts the plane; left out: the middle of the cage's box"),
                "preview": preview()
            },
            "required": ["form"],
            "examples": [{"cmd": "form.symmetry", "form": "F1", "plane": "x"}, {"cmd": "form.symmetry", "form": "F1", "plane": null}]
        }),
        "form.set_cage" => json!({
            "description": "Replaces the cage whole (an editor's own edit, an import), keeping the earlier cage's next ids; undo step Edit Cage in <Form>.",
            "properties": {
                "form": form(),
                "cage": cage("The new cage"),
                "preview": preview()
            },
            "required": ["form", "cage"],
            "examples": [{"cmd": "form.set_cage", "form": "F1", "cage": tetrahedron()}]
        }),
        _ => return None,
    };
    Some(spec)
}
