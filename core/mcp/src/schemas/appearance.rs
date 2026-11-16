// SPDX-License-Identifier: MIT
//! The JSON Schemas of the appearance, render and material commands
//! (mitcad#1918): the document's appearances, render settings and lights,
//! the bodies' and faces' looks (`core/model-api/src/lib.rs`, with
//! `AppearanceChange` and `RenderSettingsChange` of `core/model-base`) and
//! the materials (`core/model-api/src/materials.rs`, `MaterialChange` of
//! `core/model/src/material.rs`), written as [`super`] writes the others.

use serde_json::{Value, json};

use super::{choice, flag, id, ids, number, object, text};

/// The commands with a schema here, in the reference's order.
pub(super) const COMMANDS: &[&str] = &[
    "set_body_material",
    "set_body_appearance",
    "set_face_appearance",
    "clear_face_appearances",
    "create_appearance",
    "edit_appearance",
    "delete_appearance",
    "set_render_settings",
    "reset_render_settings",
    "add_render_light",
    "edit_render_light",
    "delete_render_light",
    "create_material",
    "edit_material",
    "delete_material",
    "set_component_material",
    "merge_materials",
    "import_materials",
    "create_dataset_materials",
];

/// A body, `F<n>.b<n>`.
fn body(description: &str) -> Value {
    json!({"type": "string", "pattern": "^F(0|[1-9][0-9]*)\\.b(0|[1-9][0-9]*)$", "description": description})
}

/// An sRGB colour, each component in [0, 1].
fn color(description: &str) -> Value {
    json!({"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number", "minimum": 0, "maximum": 1},
           "description": format!("{description}: sRGB [r, g, b], each in [0, 1]")})
}

/// A value in [0, 1].
fn unit(description: &str) -> Value {
    json!({"type": "number", "minimum": 0, "maximum": 1, "description": description})
}

/// A binding's expression (Bindings, mitcad#800).
fn expression(description: &str) -> Value {
    json!({"type": "string", "description": format!("Instead of {description}: an expression whose text names it, which it then follows (model_reference {{\"section\": \"Bindings: values from expressions\"}}); \"\" removes it")})
}

/// The libraries an item is taken from, and those a command writes in.
const READ: &[&str] = &["base", "user", "project"];
const WRITTEN: &[&str] = &["user", "project"];

fn project() -> Value {
    text("The project's folder, when the design is in one (the project library's place)")
}

/// What a material or profile starts from.
fn based_on(description: &str) -> Value {
    json!({
        "description": format!("{description}: an id of the design's (or a built-in one), or a library's item {{\"library\": \"base\" | \"user\" | \"project\", \"id\"}} (Duplicate)"),
        "oneOf": [
            {"type": "string"},
            {"type": "object", "properties": {"library": {"type": "string"}, "id": {"type": "string"}}, "required": ["library", "id"], "additionalProperties": false}
        ]
    })
}

/// A material's or an appearance's class.
fn class(description: &str) -> Value {
    json!({"type": "string", "enum": ["metal", "plastic", "fdm_plastic", "resin", "wood", "other"], "description": description})
}

/// A texture of an appearance.
fn texture() -> Value {
    json!({
        "type": ["object", "null"],
        "description": "The texture, or null for none: an image repeated over the body (path, or data: the image's bytes as base64 with path the file's name), size [width, height] in mm (default [100, 100]), rotation (radians), projection, and the set's other maps (normal, roughness, metalness, occlusion: each {path or data, channel, factor}) and source",
        "properties": {
            "path": text("The image file, relative to the design file's folder or absolute; with data, the name of the file the image came from"),
            "data": text("The image's bytes as base64 (embedded: no file is read)"),
            "size": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "number"}, "description": "One repeat of the image on the body, [width, height] in mm"},
            "rotation": number("The image's rotation, radians"),
            "projection": choice(&["box", "planar", "uv"], "How it is put onto a body: box (from the three axes, the default), planar (along Z), uv (the body's own texture coordinates)"),
            "normal": object("The normal map"),
            "roughness": object("The roughness map"),
            "metalness": object("The metalness map"),
            "occlusion": object("The occlusion map"),
            "source": object("Where the set came from: {id, asset, resolution, license, url}")
        },
        "required": ["path"]
    })
}

/// The fields of `create_appearance` and `edit_appearance`.
fn appearance_fields() -> Value {
    json!({
        "id": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$", "description": "create: its id (letters, digits, _ and -; the next free custom<n> when left out); edit: the appearance changed (needed: the API refuses an edit without it)"},
        "based_on": text("create_appearance only: the library or document appearance whose parameters it starts from (the defaults without it)"),
        "name": text("Its name (Appearance<n> when left out)"),
        "base_color": color("The base colour"),
        "metalness": unit("Metalness, 0 to 1"),
        "roughness": unit("Roughness, 0 to 1"),
        "specular": unit("Specular, 0 to 1"),
        "transmission": unit("Transmission (see-through like glass), 0 to 1"),
        "ior": {"type": "number", "minimum": 1, "maximum": 5, "description": "The index of refraction, 1 to 5"},
        "coat": unit("Clear coat, 0 to 1"),
        "coat_roughness": unit("The clear coat's roughness, 0 to 1"),
        "emission": {"type": "number", "minimum": 0, "maximum": 1000, "description": "Emitted light, 0 to 1000"},
        "emission_color": color("The emitted light's colour"),
        "opacity": unit("Opacity, 0 to 1"),
        "texture": texture(),
        "class": {"type": ["string", "null"], "enum": ["metal", "plastic", "fdm_plastic", "resin", "wood", "other", null], "description": "Its class; null or other: none"}
    })
}

/// A render settings section: the fields given change, a field's null
/// gives it its default.
fn section(description: &str, properties: Value) -> Value {
    json!({
        "type": "object",
        "description": format!("{description}; the fields given change, null gives a field its default"),
        "properties": properties,
        "additionalProperties": false
    })
}

/// A light's fields, all optional.
fn light_fields(id_description: &str) -> Value {
    json!({
        "id": text(id_description),
        "name": text("Its name (Light<n> when left out)"),
        "type": choice(&["point", "spot", "area", "sun"], "Its kind (point, the default): sets the default power"),
        "enabled": flag(true, "Off: kept but not lighting"),
        "space": choice(&["world", "camera"], "Where position and direction are given: in the design (world, the default) or relative to the camera, following the view"),
        "position": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": "Where the light is, mm (not for sun)"},
        "direction": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "number"}, "description": "Where it shines (spot, area, sun), a vector"},
        "color": color("Its colour"),
        "power": number("Point, spot, area: watts; sun: W/m² where it falls straight"),
        "size": number("Point, spot: the ball's diameter (0: hard shadows); area: the rectangle's width or the disc's diameter, mm"),
        "size_y": number("Area: the rectangle's height, mm"),
        "shape": choice(&["rectangle", "disc"], "Area: a rectangle or a disc"),
        "spot_angle": number("Spot: the cone's full angle, radians"),
        "spot_blend": unit("Spot: how softly the cone's edge fades, 0 (sharp) to 1"),
        "angle": number("Sun: the angular diameter of the sun's disc, radians (0: sharp shadows)")
    })
}

/// The fields of `create_material` and `edit_material`.
fn material_fields() -> Value {
    json!({
        "id": text("create: its id (the next free custom<n> when left out); edit: the material changed (needed: the API refuses an edit without it)"),
        "name": text("Its name (Material <n> when left out on create)"),
        "density": number("Density, g/cm³"),
        "appearance": {"type": ["string", "null"], "description": "Its default appearance's id (null: none)"},
        "appearances": ids("Every appearance it offers, in order, in the place of its list"),
        "filament": object("Print Filament values by key in the key map's JSON form (model_reference {\"query\": \"print_keys\"}); in edit_material a key's null removes it"),
        "based_on": based_on("create_material only: the material whose fields it starts from"),
        "class": class("Its class"),
        "version": {"type": ["integer", "null"], "minimum": 0, "description": "The library version it was copied at (null: none)"},
        "origin": {"type": ["object", "null"], "description": "The library item it was copied from, {library, id, version, declined} (null: none)"},
        "library": choice(WRITTEN, "user or project: the command works on that library's item instead of the design's material (base is read-only)"),
        "project": project(),
        "slicer": object("A slicer's own keys by slicer id ({\"<slicer>\": {\"<key>\": value}}); a key's null removes it"),
        "variants": {"type": "array", "items": {"type": "object", "properties": {"printer": {"type": "string"}, "nozzle": {"type": "number"}, "filament": {"type": "object"}, "slicer": {"type": "object"}}, "required": ["printer"], "additionalProperties": false}, "description": "Every variant (values for one printer and nozzle), in the place of those it has"},
        "variant": {"type": "object", "properties": {"printer": {"type": "string"}, "nozzle": {"type": "number"}, "filament": {"type": "object"}, "remove": {"type": "boolean"}}, "required": ["printer"], "additionalProperties": false, "description": "One variant set (made when missing; a filament key's null removed) or removed (remove: true), after variants"},
        "properties": {"type": "object", "properties": {"youngs_modulus": {"type": ["number", "null"]}, "poissons_ratio": {"type": ["number", "null"]}, "yield_strength": {"type": ["number", "null"]}, "max_service_temperature": {"type": ["number", "null"]}, "thermal_expansion": {"type": ["number", "null"]}}, "additionalProperties": false, "description": "Mechanical and thermal properties given (a null takes one away; model_reference {\"section\": \"Material properties\"})"},
        "reference": {"type": ["object", "null"], "properties": {"source": {"type": "string"}, "material": {"type": "string"}, "grade": {"type": "string"}, "revision": {"type": "string"}}, "required": ["source"], "description": "Where its values came from (null takes it away)"},
        "dataset": text("create_material only: the bundled material dataset's entry it starts from (aluminum/6061/t6)")
    })
}

/// A command's schema (fields without `cmd`).
pub(super) fn spec(name: &str) -> Option<Value> {
    let spec = match name {
        "set_body_material" => json!({
            "description": "Sets a body's material, from the design or a library, with its appearance or keeping the body's.",
            "properties": {
                "uid": body("The body"),
                "material": {"type": ["string", "null"], "description": "A built-in or design material's id (with library: that library's item); null: its component's default"},
                "appearance": choice(&["replace", "keep"], "replace: the material's appearance becomes the body's; keep: the body keeps its own; left out: replace unless set by hand"),
                "appearance_id": text("Instead of appearance: one of the material's appearances for the body (one it does not offer yet joins its list)"),
                "library": choice(READ, "The material is that library's item, copied into the design (or its copy reused) in the same undo step"),
                "project": project(),
                "expression": expression("material")
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_body_material", "uid": "F2.b0", "material": "pla"},
                         {"cmd": "set_body_material", "uid": "F2.b0", "material": null, "appearance": "keep"}]
        }),
        "set_body_appearance" => json!({
            "description": "Sets a body's appearance (an appearance outside its material's list joins it).",
            "properties": {
                "uid": body("The body"),
                "appearance": {"type": ["string", "null"], "description": "An appearance of the library or the document by id; null or left out: the default look"},
                "expression": expression("appearance")
            },
            "required": ["uid"],
            "examples": [{"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_black"}]
        }),
        "set_face_appearance" => json!({
            "description": "Sets the appearance of faces of a body.",
            "properties": {
                "uid": body("The body"),
                "faces": ids("The faces' topological names at the marker (F2:side(c1[c4,c2]))"),
                "appearance": {"type": ["string", "null"], "description": "An appearance's id; null or left out: the faces show the body's again"},
                "expression": expression("appearance")
            },
            "required": ["uid", "faces"],
            "examples": [{"cmd": "set_face_appearance", "uid": "F2.b0", "faces": ["F2:top"], "appearance": "paint_red"}]
        }),
        "clear_face_appearances" => json!({
            "description": "Takes every face appearance of a body away.",
            "properties": {"uid": body("The body")},
            "required": ["uid"],
            "examples": [{"cmd": "clear_face_appearances", "uid": "F2.b0"}]
        }),
        "create_appearance" => json!({
            "description": "Adds an appearance to the document; the fields given set its parameters, the others are those of based_on or the defaults. Result: id.",
            "properties": appearance_fields(),
            "required": [],
            "examples": [{"cmd": "create_appearance", "id": "red_gloss", "name": "Red gloss", "base_color": [0.8, 0.05, 0.05], "roughness": 0.2}],
            "restrictions": ["MCP refuses a texture (or texture map) read from a file (path without data): give the image as data (base64)."]
        }),
        "edit_appearance" => json!({
            "description": "Changes the given parameters of a document appearance (id).",
            "properties": appearance_fields(),
            "required": [],
            "examples": [{"cmd": "edit_appearance", "id": "red_gloss", "roughness": 0.4, "texture": null}],
            "restrictions": ["id is needed.", "MCP refuses a texture (or texture map) read from a file (path without data): give the image as data (base64)."]
        }),
        "delete_appearance" => json!({
            "description": "Deletes a document appearance; bodies and faces that used it get the default look. Result: bodies, faces.",
            "properties": {"id": id("The appearance")},
            "required": ["id"],
            "examples": [{"cmd": "delete_appearance", "id": "red_gloss"}]
        }),
        "set_render_settings" => json!({
            "description": "Changes the given fields of the render settings, per section. Result: changed.",
            "properties": {
                "environment": section("The light around the design", json!({
                    "preset": choice(&["studio", "studio_white", "studio_dark", "outdoor", "image"], "The lighting"),
                    "strength": number("Its strength (at most 100)"),
                    "rotation": number("Its rotation about Z, radians"),
                    "sun_elevation": number("outdoor: the sun's elevation, radians"),
                    "sun_azimuth": number("outdoor: the sun's azimuth, radians"),
                    "image": {"type": ["string", "null"], "description": "image: the HDR image file"}
                })),
                "background": section("What the render shows behind the bodies", json!({
                    "mode": choice(&["view", "color", "environment"], "The view's background, a colour, or the environment"),
                    "color": color("color: the colour")
                })),
                "ground": section("The ground under the design", json!({
                    "shadows": {"type": "boolean", "description": "Shadows on the ground"},
                    "reflections": {"type": "boolean", "description": "Reflections on the ground"},
                    "height": {"type": ["number", "null"], "description": "The ground's height, mm (null: under the design)"}
                })),
                "film": section("The exposure", json!({
                    "exposure": number("Exposure in stops (-10 to 10)"),
                    "view_transform": choice(&["standard", "filmic", "neutral"], "How light becomes colours")
                })),
                "output": section("The image", json!({
                    "width": {"type": "integer", "description": "Width in pixels"},
                    "height": {"type": "integer", "description": "fixed: height in pixels"},
                    "aspect": choice(&["view", "fixed"], "The view's aspect or width x height"),
                    "samples": {"type": "integer", "description": "Samples per pixel"},
                    "time_limit": number("A time limit in seconds"),
                    "denoise": {"type": "boolean", "description": "Denoise"},
                    "transparent": {"type": "boolean", "description": "A transparent background"},
                    "format": choice(&["png", "png16", "jpeg", "exr"], "The file's format"),
                    "quality": {"type": "integer", "description": "jpeg: its quality"}
                }))
            },
            "required": [],
            "examples": [{"cmd": "set_render_settings", "film": {"exposure": 0.5, "view_transform": "filmic"}, "background": {"mode": "color", "color": [1, 1, 1]}}],
            "restrictions": ["MCP refuses environment.image (an image file read)."]
        }),
        "reset_render_settings" => json!({
            "description": "Gives the render settings their defaults. Result: changed.",
            "properties": {},
            "required": [],
            "examples": [{"cmd": "reset_render_settings"}]
        }),
        "add_render_light" => json!({
            "description": "Adds a light of the render settings with the fields given (all optional). Result: id.",
            "properties": light_fields("Its id (the next free light<n> when left out)"),
            "required": [],
            "examples": [{"cmd": "add_render_light", "type": "spot", "position": [0, 0, 300], "direction": [0, 0, -1], "power": 10}]
        }),
        "edit_render_light" => json!({
            "description": "Changes the given fields of a light (null: the field's default). Result: changed.",
            "properties": light_fields("The light (light1); needed"),
            "required": ["id"],
            "examples": [{"cmd": "edit_render_light", "id": "light1", "power": 20, "enabled": false}]
        }),
        "delete_render_light" => json!({
            "description": "Deletes a light of the render settings.",
            "properties": {"id": id("The light (light1)")},
            "required": ["id"],
            "examples": [{"cmd": "delete_render_light", "id": "light1"}]
        }),
        "create_material" => json!({
            "description": "Adds a material to the design (or, with library, to that library). Result: id (with library also library, version, path, warnings).",
            "properties": material_fields(),
            "required": [],
            "examples": [{"cmd": "create_material", "name": "Walnut", "density": 0.65, "class": "wood"},
                         {"cmd": "create_material", "dataset": "aluminum/6061/t6"}]
        }),
        "edit_material" => json!({
            "description": "Changes the given fields of the design's material (or, with library, that library's item). Result: {} (with library: version, path, warnings).",
            "properties": material_fields(),
            "required": [],
            "examples": [{"cmd": "edit_material", "id": "custom1", "density": 1.24, "appearance": null}],
            "restrictions": ["id is needed; dataset and based_on are refused."]
        }),
        "delete_material" => json!({
            "description": "Deletes the design's material (or, with library, that library's item). Result: bodies and components that named it (with library: path, kept, removed).",
            "properties": {
                "id": id("The material"),
                "library": choice(WRITTEN, "user or project: that library's item is deleted"),
                "project": project()
            },
            "required": ["id"],
            "examples": [{"cmd": "delete_material", "id": "custom1"}]
        }),
        "set_component_material" => json!({
            "description": "Sets a component's default material for its bodies without one of their own.",
            "properties": {
                "component": id("The component by uid (C0 the root) or name"),
                "material": {"type": ["string", "null"], "description": "A material's id (with library: that library's item); null or left out: none of its own"},
                "appearance_id": text("One of the material's appearances for its bodies without one of their own; left out: the material's default"),
                "library": choice(READ, "The material is that library's item, copied into the design (or its copy reused) in the same undo step"),
                "project": project()
            },
            "required": ["component"],
            "examples": [{"cmd": "set_component_material", "component": "C0", "material": "pla"}]
        }),
        "merge_materials" => json!({
            "description": "Merges one filament's separate materials into one with variants: the first takes the others. Result: id, warnings.",
            "properties": {
                "ids": {"type": "array", "minItems": 2, "items": {"type": "string"}, "description": "The materials, two or more: the first takes the others"},
                "library": choice(WRITTEN, "user or project: that library's items are merged"),
                "project": project()
            },
            "required": ["ids"],
            "examples": [{"cmd": "merge_materials", "ids": ["custom1", "custom2"]}]
        }),
        "import_materials" => json!({
            "description": "Imports materials from a JSON or CSV file's text as one undo step. Result: added, replaced, kept, conflicts, errors.",
            "properties": {
                "text": text("The file's text (the application reads the file)"),
                "format": choice(&["json", "csv"], "The file's format"),
                "conflict": choice(&["keep", "replace", "keep_both", "ask"], "On a name that is there already: keep (the default), replace, keep_both, or ask (nothing imported, conflicts named)"),
                "library": choice(WRITTEN, "user or project: into that library; the design when left out"),
                "project": project()
            },
            "required": ["text", "format"],
            "examples": [{"cmd": "import_materials", "text": "name,density [g/cm3]\nWalnut,0.65\n", "format": "csv"}]
        }),
        "create_dataset_materials" => json!({
            "description": "Adds a material of each of several entries of the bundled material dataset, one undo step; a name there already is skipped. Result: added, skipped.",
            "properties": {
                "entries": {"type": "array", "minItems": 1, "items": {"type": "string"}, "description": "The entries' ids (wood/walnut, panel/plywood_pine)"},
                "library": choice(WRITTEN, "user or project: into that library; the design when left out"),
                "project": project()
            },
            "required": ["entries"],
            "examples": [{"cmd": "create_dataset_materials", "entries": ["wood/walnut"]}]
        }),
        _ => return None,
    };
    Some(spec)
}
