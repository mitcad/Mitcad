// SPDX-License-Identifier: MIT
use serde_json::{Value, json};

use crate::reference::echoed;

// The longest strings the tools take (mitcad#935), in characters: a
// handle or a command id, an owner, occurrence or other name, an entity's
// topological name (it nests the features it came through), a path.
const ID_CHARS: u64 = 256;
const NAME_CHARS: u64 = 1024;
const ENTITY_CHARS: u64 = 4096;
const PATH_CHARS: u64 = 4096;
// ui_screenshot's max_bytes (mitcad#1188), the encoded image's bytes: the
// smallest a caller may ask for, and the default and largest for an image
// returned inline (well within the 8 MiB frame once in base64). The
// application's mcp/ScreenshotBudget.hpp has the same numbers.
pub(crate) const SCREENSHOT_MIN_BYTES: u64 = 4096;
pub(crate) const SCREENSHOT_INLINE_BYTES: u64 = 5 * 1024 * 1024;
/// The longest tool description, in characters (mitcad#1506): the rules
/// of one tool are in its description, which clients keep whole.
#[cfg(test)]
pub(crate) const DESCRIPTION_CHARS: usize = 2048;

/// How a sketch's ids are given out (mitcad#1269), in `model_command`'s
/// description since the server instructions became too long to arrive
/// whole (mitcad#1506); the reference's "Sketch commands" says it too.
pub(crate) const SKETCH_IDS: &str = "In a sketch, points, curves and texts share one numbering (p3, c4, t5 name different entities) and constraints and dimensions another (k6): take ids from a result (entities, made, constraints, dimensions) or the sketch query, and do not guess the ids a command earlier in the same model_command array will make. A point input [x, y] is always a new point, also where a curve ends; to join a new curve to an end, give the end's id (the drawing tools' point inputs take an existing point id where the reference says so), or hold the two ends together afterwards with a coincident constraint (two curve ends become one point).";

/// How the application's command ids are found (mitcad#1269), in
/// `ui_command`'s description (mitcad#1506).
pub(crate) const UI_COMMAND_IDS: &str = "ui_command with search (\"\" for all, or words such as \"fillet\") lists the command registry's ids with whether an agent may run each; an unknown id is refused with the nearest ones.";

pub(crate) fn tools() -> Vec<Value> {
    let document = json!({"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "Explicit document handle returned by document_create or document_open"});
    let path = json!({"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "Path within the configured workspace"});
    vec![
        tool(
            "document_create",
            "Create an empty metric CAD document and return its handle. Documents remain open until closed or the server exits.",
            json!({}),
            &[],
            false,
            false,
        ),
        tool(
            "document_open",
            "Open a MitCad project within the configured workspace and return its document handle. The source file is not changed. A SolidCraft design (.sc, .solidcraft) is imported with its history into a new, unsaved document (save it with document_save and a path); a .sc file whose format key is not SolidCraft's is refused.",
            json!({"path": path}),
            &["path"],
            false,
            false,
        ),
        tool(
            "document_list",
            "List open document handles and their saved paths and modification state.",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "document_close",
            "Close a document. Unsaved changes prevent closing unless discard is explicitly true.",
            json!({
                "document": document,
                "discard": {"type": "boolean", "default": false, "description": "Explicitly discard unsaved changes"}
            }),
            &["document"],
            false,
            true,
        ),
        tool(
            "model_query",
            "Read a document using one existing JSON model query. model_reference lists the query names and gives each one's arguments and result (identities and units too). print_thickness answers its summary only (thinnest wall, where, regions, thin_triangles) unless the query sets triangles (true, with max_triangles to cap them). What numbers miss (a slab past a gable, a cut under a step) shows in a picture: model_render.",
            json!({"document": document, "query": query_schema()}),
            &["document", "query"],
            true,
            false,
        ),
        tool(
            "model_command",
            &format!(
                "Apply a JSON model command, or an array of them in order, to a document. Look up the command, or the feature type of add_feature, with model_reference first. An array stops at the first refused entry (the entries before it are undone, nothing of the array stays; the result names the entry) or at the first entry whose added or edited feature fails (applied; stopped_at names it). Headless print imports import_print_profile and import_slicer_presets read only workspace sources into the design (into omitted or design; no project); inherited JSON requires a confined slicer folder. Other file imports and external links are refused. isError is true only when a command is refused or a feature it added or edited fails; failed_features lists every failed feature in the timeline (edited, new). A failed feature stays in the timeline: edit it, or use undo or delete_feature. The result holds the document's state and what the command changed (changes: features added, changed, recomputed or failed, bodies added or changed, what was removed); timeline: full gives the whole timeline and bodies instead, none neither. hints gives up to 3 modeling tips for added/edited features; hints:false omits them. {SKETCH_IDS}"
            ),
            json!({
                "document": document,
                "command": command_schema(),
                "timeline": timeline_schema(),
                "hints": {"type": "boolean", "default": true,
                          "description": "Short modeling guidance for features added or edited by this call; false omits it"}
            }),
            &["document", "command"],
            false,
            true,
        ),
        tool(
            "document_save",
            "Save a document as a MitCad project inside the workspace. An existing file requires overwrite:true.",
            json!({
                "document": document,
                "path": path,
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file"}
            }),
            &["document", "path"],
            false,
            true,
        ),
        tool(
            "model_export",
            "Export geometry to a new file inside the workspace. An existing file is replaced only with overwrite: true (each file the export writes, checked before any is written). Optionally select bodies; refinement sets the mesh of STL, OBJ, 3MF and USDZ (low, medium by default, high, or {\"deviation\": mm, \"angle\": radians}; model_reference {\"section\": \"Mesh refinement\"}); DXF requires a sketch handle, or a sheet-metal body whose Flat Pattern is written, instead of bodies, unless drawing or cut_sheet is given; cut_sheet writes a cut sheet kept in the design (DXF or SVG at 1:1, model_reference {\"section\": \"Cut sheets\"}). PDF and SVG (and DXF with drawing) write a drawing of the bodies: views with hidden lines removed, overall dimensions, hole diameters, a hole table and a title block; drawing holds the export_drawing options (sheet, orientation, scale, projection, views, section, hidden_lines, ...; model_reference {\"section\": \"Drawings\"}). 3MF for a slicer: plates writes the design's print plates (one_file: every plate in one file, for Bambu Studio, OrcaSlicer, QIDI Studio and PrusaSlicer 2.9+; file_per_plate: a file per plate beside path, for Cura and older PrusaSlicer, each checked before any is written; model_reference {\"section\": \"Print plates\"}), plate one of them, dialect the slicer whose settings go with the bodies (bambu_studio, orcaslicer, qidi_studio, prusaslicer, cura); on_bed, center, drop, auto_orient, bed_face and spacing lay the pieces on the print bed (also STL and OBJ; model_reference {\"section\": \"Print bed faces\"}), occurrences and whole pick placements. USDZ: animation writes a mechanism's motion as time-sampled placements (poses per occurrence or body, or a motion study; model_reference {\"section\": \"Animated USDZ\"}). The result is the export command's (plates with each plate's path, origin, bed and warnings, and files). Defaults are metric.",
            json!({
                "document": document,
                "path": path,
                "format": {"type": "string", "enum": ["step", "iges", "brep", "stl", "obj", "3mf", "usdz", "dxf", "pdf", "svg"]},
                "bodies": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS}, "description": "Optional body identities, for example F7.b0"},
                "sketch": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "Sketch feature identity for DXF, for example F2"},
                "construction": {"type": "boolean", "default": false, "description": "With sketch: also write its construction curves and center lines, on the layer construction (left out by default)"},
                "flat_pattern": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "Sheet-metal body identity for a Flat Pattern DXF, for example F3.b0"},
                "drawing": {"type": "object", "description": "Drawing options (export_drawing without cmd, path and bodies), for example {\"sheet\": \"A3\", \"views\": [\"front\", \"top\", \"right\", \"iso\"], \"hidden_lines\": true}"},
                "cut_sheet": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A cut sheet kept in the design (its id like D2, or its name) written as DXF or SVG: its parts' outlines at 1:1 on the layers CUT_OUTER and CUT_INNER"},
                "labels": {"type": "boolean", "default": false, "description": "With cut_sheet: each part's name as text on the layer LABEL"},
                "plates": {"type": "string", "enum": ["one_file", "file_per_plate"], "description": "3MF: the design's print plates instead of bodies, on the print bed: one_file every plate in path (Bambu Studio, OrcaSlicer, QIDI Studio, PrusaSlicer 2.9+), file_per_plate a file per plate beside path, '<name> - <plate name>.3mf' (Cura, older PrusaSlicer); no bodies, occurrence, occurrences, whole or bed_face with it (model_reference {\"section\": \"Print plates\"})"},
                "plate": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "3MF: one print plate (its id like plate2, or its name) alone, in path, on the print bed"},
                "dialect": {"type": "string", "enum": ["bambu_studio", "orcaslicer", "qidi_studio", "prusaslicer", "cura"], "description": "3MF: the slicer whose print settings go with the bodies (model_reference {\"section\": \"Print profiles\"})"},
                "on_bed": {"type": "boolean", "default": false, "description": "3MF, STL, OBJ: the bodies on the print bed by their print bed faces, centred on the plate (implied by plates and plate)"},
                "center": {"type": "boolean", "default": true, "description": "On the print bed: each piece's middle over the plate's centre; a 3MF with a dialect without on_bed: the whole build's middle over it (false keeps the design's X and Y)"},
                "drop": {"type": "boolean", "default": true, "description": "On the print bed: each piece's lowest point on the bed"},
                "auto_orient": {"type": "boolean", "default": true, "description": "On the print bed: a piece without a usable print bed face turned onto the face the automatic orientation finds"},
                "spacing": {"type": "number", "minimum": 0, "maximum": 1000, "default": 5, "description": "On the print bed: mm between neighbouring pieces beyond their brims (model_reference {\"section\": \"Spacing beyond the brims\"})"},
                "bed_face": {"type": ["object", "array"], "description": "On the print bed: the print bed face that turns the piece it is in, {\"body\": \"F2.b0\", \"occurrence\": \"Arm:1\"} (occurrence \"\" or left out for the design itself), or a list of them, one per piece"},
                "occurrence": {"type": "string", "maxLength": NAME_CHARS, "description": "Only the placements by this occurrence (\"O1/O4\" or \"Arm:1/Pin:2\")"},
                "occurrences": {"type": "object", "description": "3MF: per body (uid or name), the occurrences whose placements are written, e.g. {\"F2.b0\": [\"Nuts:1\"]} (\"\" the design itself; [] all of them)"},
                "whole": {"type": "array", "items": {"type": "string", "maxLength": NAME_CHARS}, "description": "3MF on the print bed: occurrences sent whole, each one object as assembled"},
                // Print colours by height (mitcad#1596).
                "print_colors": {"type": "boolean", "default": false, "description": "USDZ: each placement in the colours of the filaments it prints with by height (the filament changes, set_filament_changes), as model_render's print_colors draws it, in place of the appearances; a 3MF for a slicer gets the changes themselves with dialect"},
                // The mesh's tolerances and replacing a file (mitcad#1569).
                "refinement": refinement_schema(),
                // A USDZ's time-sampled placements (mitcad#1597).
                "animation": animation_schema(),
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of existing files: path, and the other files the export writes (an OBJ's .mtl, a drawing's further sheets, file_per_plate's plate files)"}
            }),
            &["document", "path", "format"],
            false,
            false,
        ),
        {
            // The headless server draws pictures (mitcad#1486).
            let mut reference = reference_tool();
            reference["description"] = json!(format!(
                "{} To see what a document looks like, model_render draws it (PNG).",
                reference["description"].as_str().unwrap_or_default()
            ));
            reference
        },
        with_raytrace(with_overlay(render_tool())),
    ]
}

/// model_render's overlay in its description (mitcad#1488).
const RENDER_OVERLAY: &str = "overlay draws what Print Analysis' functions find over the bodies, with a summary per body in the result's overlay: overhang (angle; up, else the print bed face, else +Z), thickness (walls under minimum), draft (angle, pull) or failed (the bodies failed features read); a name or {type, ...}; fast quality only.";

/// model_render's overlay (mitcad#1488): what Print Analysis' functions
/// find, drawn over the bodies, with its summary in the result.
fn with_overlay(mut tool: Value) -> Value {
    tool["description"] = json!(format!(
        "{} {RENDER_OVERLAY}",
        tool["description"].as_str().unwrap_or_default()
    ));
    let direction = |description: &str| json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": description});
    if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
        properties.insert(
            "overlay".to_owned(),
            json!({
                "type": ["string", "object"],
                "minLength": 1,
                "maxLength": 16,
                "description": "overhang, thickness, draft or failed, or {\"type\": one of them, with its settings and color \"#rrggbb\"}",
                "properties": {
                    "type": {"type": "string", "enum": ["overhang", "thickness", "draft", "failed"]},
                    "angle": {"type": "number", "minimum": 0, "maximum": 90, "description": "overhang: degrees from the vertical (default 45); draft: the least draft in degrees (default 1)"},
                    "up": direction("overhang: printed with this direction up (default: the body's print bed face, else +Z)"),
                    "minimum": {"type": "number", "minimum": 0, "description": "thickness: the thinnest wall in mm (default 0.8)"},
                    "pull": direction("draft: the mold's pull direction (default +Z)"),
                    "color": {"type": "string", "minLength": 7, "maxLength": 7, "description": "#rrggbb"}
                },
                "required": ["type"],
                "additionalProperties": false
            }),
        );
    }
    tool
}

/// model_render's ray-traced picture (mitcad#1489): its quality and the
/// render worker's samples, time limit and denoising.
fn with_raytrace(mut tool: Value) -> Value {
    tool["description"] = json!(format!(
        "{} quality raytrace ray traces the picture with the Cycles render worker as mitcad-cli render does (the design's render settings; samples, time_limit_s, denoise), with the same camera as the fast mode for the same arguments; slower, and refused with error_code render_worker_missing or render_worker_failed (suggesting quality fast) when the worker is not installed or fails.",
        tool["description"].as_str().unwrap_or_default()
    ));
    if let (Some(properties), Value::Object(quality)) = (
        tool["inputSchema"]["properties"].as_object_mut(),
        json!({
            "quality": {"type": "string", "enum": ["fast", "raytrace"], "default": "fast", "description": "fast: shaded and edged as the 3D view shows it; raytrace: the Cycles render worker as mitcad-cli render runs it, with the same camera"},
            "samples": {"type": "integer", "minimum": 1, "maximum": 4096, "default": 64, "description": "raytrace: samples per pixel"},
            "time_limit_s": {"type": "number", "minimum": 1, "maximum": 600, "default": 30, "description": "raytrace: the most seconds the render may take; it stops with the samples it has"},
            "denoise": {"type": "boolean", "default": true, "description": "raytrace: denoise the picture"}
        }),
    ) {
        properties.extend(quality);
    }
    tool
}

/// model_render's description (mitcad#1486), both servers': what it
/// draws, its camera and its budget.
const RENDER_ABOUT: &str = "A picture of the document's bodies, shaded with their appearances and edged as the 3D view shows them, returned as an MCP image (PNG) after the text, to see what a design looks like: drawn without a GPU or a display, the same arguments giving the same bytes. view: home (default), iso, front, back, left, right, top, bottom, a named view of the design, {azimuth, elevation} in degrees, or a camera {eye, target, up}; the result's camera (eye, target, up, projection, height, fov) passed back as view gives the same picture, to turn or zoom from it. fit frames all bodies drawn, some by id (F2.b0, F7, C1, O1/O4, or a body's name) or a box {min, max} to zoom on a detail; bodies and hidden choose what is drawn. max_bytes counts the PNG (default 5 MiB): a larger picture is drawn smaller, downscaled_for_bytes says so.";
/// Its inspection options and views (mitcad#1487).
const RENDER_OPTIONS: &str = "colors {id: \"#rrggbb\"} recolors bodies; highlight draws ids, faces (F2:side(...), Body1/F2:end(...)) and edges (E{...}) in the selection blue; section {origin, normal} cuts away the normal's side, solids capped; explode spreads components apart by a factor; views (up to 8) answers an image per view, each with its camera in structuredContent.views, sharing max_bytes. Unknown references are warnings.";

/// The live server's model_render (mitcad#1487).
const RENDER_LIVE: &str = "The live server draws the design at the timeline marker as the headless server does for the same file and arguments, whatever the window's view shows (ui_screenshot shows that). path also writes the PNG at full size to a new .png file in a folder the user allows agents to use.";

/// A preview image of a document (mitcad#1486), headless.
fn render_tool() -> Value {
    let document = json!({"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "Explicit document handle returned by document_create or document_open"});
    let ids = json!({"type": "array", "minItems": 1, "maxItems": 10000, "items": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS}});
    tool(
        "model_render",
        &format!(
            "{RENDER_ABOUT} {RENDER_OPTIONS} path also writes the PNG at full size to a new .png file in the workspace. Read-only: allowed with --read-only (without path)."
        ),
        json!({
            "document": document,
            "view": {"type": ["string", "object"], "description": "A view name (home, iso, front, back, left, right, top, bottom, or a named view of the design), {\"azimuth\": deg, \"elevation\": deg} (azimuth about Z from the front, 90 from the right), or a camera {\"eye\": [x,y,z], \"target\": [x,y,z], \"up\": [x,y,z]} in mm with optional height, fov and projection, as the result's camera gives it. Default home"},
            "projection": {"type": "string", "enum": ["perspective", "orthographic"], "description": "Default: the view's own, else perspective"},
            "fit": {"type": ["string", "array", "object"], "description": "\"all\": the bodies drawn (the default for a view name or angles; a camera stays as given unless fit is given); a list of body, feature, component or occurrence ids or body names; or a box {\"min\": [x,y,z], \"max\": [x,y,z]} in mm", "items": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS}, "minItems": 1, "maxItems": 10000},
            "bodies": {"type": ["string", "array"], "description": "\"visible\" (default: what the design shows), \"all\" (hidden bodies too), or a list of ids as for fit: only these", "items": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS}, "minItems": 1, "maxItems": 10000},
            "hidden": ids,
            "size": {"type": "array", "items": {"type": "integer", "minimum": 16, "maximum": 4096}, "minItems": 2, "maxItems": 2, "default": [1024, 768], "description": "[width, height] in pixels"},
            "edges": {"type": "boolean", "default": true, "description": "Face boundaries and silhouettes"},
            "background": {"type": "string", "minLength": 1, "maxLength": 16, "default": "theme", "description": "theme (the view's light gradient), transparent (a PNG with alpha), white, black or #rrggbb"},
            "path": {"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "Also write the PNG at full size to this .png file in the workspace (refused when the server is read-only)"},
            "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file at path"},
            "max_bytes": {"type": "integer", "minimum": SCREENSHOT_MIN_BYTES, "maximum": SCREENSHOT_INLINE_BYTES, "description": "The returned PNG's largest size in bytes (not its base64): the picture is drawn smaller until it fits. Default 5 MiB"},
            // The inspection options and views (mitcad#1487).
            "colors": {"type": "object", "description": "Per-body colors: {\"F2.b0\": \"#a52a2a\", \"C1\": \"#3060c0\"}, ids as for bodies; a body several name takes the most specific's. An id naming no body drawn is a warning"},
            "highlight": {"type": "array", "minItems": 1, "maxItems": 1000, "items": {"type": ["string", "object"]}, "description": "References drawn in the selection blue: body, feature, component or occurrence ids or body names (the whole body), face names (F2:side(c1[c4,c2])), edge names (E{...|...}, a line over everything), body/face text (Body1/F2:end(...)), {\"body\", \"face\" or \"edge\", \"occurrence\"} or an entity as ui_state lists it {\"kind\", \"owner\", \"name\"}. One naming nothing drawn is a warning"},
            "section": {"type": "object", "description": "A clip plane {\"origin\": [x,y,z], \"normal\": [x,y,z]} in mm: the side the normal points to is cut away, solid bodies capped (their color, hatched), to look inside fits and cavities"},
            "explode": {"type": "number", "minimum": 0, "maximum": 10, "default": 0, "description": "Moves top-level components (in a design of one, its bodies) apart from the drawn bodies' centre: each by this factor times its centre's distance from it; 0 draws them where they are"},
            // Print colours by height (mitcad#1596).
            "print_colors": {"type": "boolean", "default": false, "description": "Each body in the colours of the filaments it prints with by height, as the filament changes of the plate it prints on say (set_filament_changes; model_reference {\"section\": \"Filament changes at print heights\"}), in place of its appearances: its triangles cut at the layer boundaries, measured up from its lowest point away from its print bed face; the design is not changed"},
            "views": {"type": "array", "minItems": 1, "maxItems": 8, "items": {"type": ["string", "object"]}, "description": "Several pictures in one call: each a view (as view takes it) or {\"view\", \"projection\", \"fit\", \"size\", \"section\"} over this call's arguments; structuredContent.views holds each one's camera and size in order, and an image block follows the text for each. Each picture gets max_bytes / count bytes. Not with path"}
        }),
        &["document"],
        true,
        false,
    )
}

/// The command reference in parts (mitcad#714), in both servers; the
/// protocol answers it without the backend.
fn reference_tool() -> Value {
    tool(
        "model_reference",
        "Look up the model API reference: commands, queries, feature definitions, values and units, identities. Without arguments: the contents (the sections' headings) and the names of every query, command and feature type this server permits. With one of: section (a heading or a name it lists, e.g. \"Values\", \"split_body\", \"Sketch commands\"), query or command (its reference row: fields and result), feature (a feature type's section with an example definition; a section and a feature come in brief, the fields and tables without nested details, and extended: true gives the whole text), search (the sections and excerpts that contain a text), guide (how to model well in MitCad rather than what exists: \"index\" lists the topics with when to read each, as the contents' guides do; read the topic that fits the task). Each answer holds at most 64 kB of text; a longer one says truncated and gives next_offset, to pass as offset for the rest. The resource mitcad://reference/commands is the whole reference (about 1 MB), and mitcad://guide/<id> each guide.",
        json!({
            "section": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A section's heading, or a name its heading lists, e.g. \"Values\", \"split_body\" or \"Sketch commands\""},
            "query": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A query's name, e.g. \"bodies\" or \"faces\""},
            "command": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A command's name (its cmd), e.g. \"add_feature\" or \"sketch.add_line\""},
            "feature": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A feature type (a definition's type), e.g. \"extrude\" or \"construction_plane\""},
            "search": {"type": "string", "minLength": 2, "maxLength": NAME_CHARS, "description": "A text to find in any case, e.g. \"bbox\""},
            "guide": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A guide on how to model well here: \"index\" (the topics, with when to read each) or a topic's id, e.g. \"principles\""},
            "extended": {"type": "boolean", "default": false, "description": "With section or feature: the whole text instead of the brief one (nested details, every example, the subsections' texts)"},
            "offset": {"type": "integer", "minimum": 0, "description": "Where a truncated answer continues: its next_offset"}
        }),
        &[],
        true,
        false,
    )
}

/// `model_command`'s command: an object, or an array of them run in order
/// (mitcad#689).
fn command_schema() -> Value {
    json!({
        "type": ["object", "array"],
        "items": {"type": "object"},
        "minItems": 1,
        "maxItems": 1000,
        "description": "One JSON command object {\"cmd\": \"<name>\", ...its fields}, for example {\"cmd\": \"add_feature\", \"def\": {...}}, or an array of them run in order. JSON numbers are millimeters and radians; write \"90 deg\" for degrees. They are millimeters in a design in any length unit (the document query's units.length): convert a length the user gives in the design's unit, or write it as text, which is read in that unit (\"0.875\", \"0.875 in\"). model_reference gives each command's fields."
    })
}

/// `model_export`'s `refinement` (mitcad#1569): the `export` command's,
/// which checks its values (commands.md, "Mesh refinement").
fn refinement_schema() -> Value {
    json!({
        "type": ["string", "object"],
        "minLength": 1,
        "maxLength": 16,
        "description": "STL, OBJ, 3MF, USDZ: the mesh's tolerances, a preset (low, medium the default, high) or {\"deviation\": mm, \"angle\": radians} with optional face_triangles (a face's most triangles, 0 none) and max_edge (mm, a triangle's longest edge, 0 none), e.g. {\"deviation\": 0.03, \"angle\": 0.052} for a small printed part",
        "properties": {
            "deviation": {"type": "number", "description": "The surface's largest distance from the mesh in mm"},
            "angle": {"type": "number", "description": "The largest angle between neighbouring facets' normals in radians"},
            "face_triangles": {"type": "integer", "minimum": 0, "description": "A face's most triangles (about); 0, the default: no bound"},
            "max_edge": {"type": "number", "description": "STL, OBJ, 3MF: a triangle's longest edge in mm; 0, the default: none"}
        },
        "additionalProperties": false
    })
}

/// `model_export`'s `animation` (mitcad#1597): the `export` command's,
/// which checks its values (commands.md, "Animated USDZ").
fn animation_schema() -> Value {
    let pose = json!({
        "type": "array",
        "minItems": 3,
        "maxItems": 16,
        "items": {"type": ["array", "number"]},
        "description": "Rows of the rotation with the translation last, [[r00, r01, r02, tx], [r10, r11, r12, ty], [r20, r21, r22, tz]] (a fourth row [0, 0, 0, 1] optional; p' = M p), or the same 12 or 16 numbers row by row; a rotation and a translation only"
    });
    json!({
        "type": "object",
        "description": "USDZ: a mechanism's motion as time-sampled placements of the occurrences and bodies; the meshes and materials are written once and the design does not change. Either tracks (poses given, one per frame) or study (a saved motion study's placements). model_reference {\"section\": \"Animated USDZ\"}",
        "properties": {
            "fps": {"type": "number", "minimum": 0, "description": "Frames per second (> 0): the file's timeCodesPerSecond"},
            "start": {"type": "number", "description": "The first frame's time code (default 0); the others follow one apart"},
            "times": {"type": "array", "minItems": 1, "maxItems": 100000, "items": {"type": "number"}, "description": "Each frame's time code, increasing (instead of start)"},
            "space": {"type": "string", "enum": ["parent", "design"], "default": "parent", "description": "tracks: parent (an occurrence's placement in its parent occurrence, a body's motion in its component) or design (where it is in the design)"},
            "tracks": {
                "type": "array",
                "minItems": 1,
                "maxItems": 10000,
                "items": {
                    "type": "object",
                    "properties": {
                        "occurrence": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "An occurrence path (\"O1/O4\" or \"Arm:1/Pin:2\"): each placement of a repeated or nested component moves on its own"},
                        "body": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A body (uid or name) written by one placement; one written by several is refused: name the occurrence"},
                        "transforms": {"type": "array", "minItems": 1, "maxItems": 100000, "items": pose, "description": "One pose per frame; every track has as many"}
                    },
                    "required": ["transforms"],
                    "additionalProperties": false
                },
                "description": "Poses given: each an occurrence or a body and its transforms"
            },
            "study": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "A saved motion study's name: its keyed joints solved at each frame's step, as play_study solves them"},
            "from": {"type": "number", "description": "study: the first frame's step (default 0)"},
            "to": {"type": "number", "description": "study: the last frame's step (default the study's steps)"},
            "frames": {"type": "integer", "minimum": 1, "maximum": 100000, "description": "study: the number of frames from from to to (default one per step)"}
        },
        "required": ["fps"],
        "additionalProperties": false
    })
}

/// `model_query`'s query (mitcad#714).
fn query_schema() -> Value {
    json!({
        "type": "object",
        "description": "One JSON query object {\"query\": \"<name>\", ...its fields}, for example {\"query\": \"bodies\", \"properties\": true}; model_reference lists the queries and gives each one's fields"
    })
}

/// How much of the document `model_command` answers with (mitcad#689).
fn timeline_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["changed", "full", "none"],
        "default": "changed",
        "description": "changed: what the command changed (default); full: the whole timeline and bodies; none: only the document's state"
    })
}

pub(crate) fn live_tools() -> Vec<Value> {
    let mut tools: Vec<Value> = tools()
        .into_iter()
        .filter(|tool| {
            matches!(
                tool["name"].as_str(),
                Some(
                    "document_list"
                        | "model_query"
                        | "model_command"
                        | "model_export"
                        | "model_reference"
                        | "model_render"
                )
            )
        })
        .collect();
    for tool in &mut tools {
        match tool["name"].as_str() {
            Some("document_list") => {
                tool["description"] = json!(
                    "List every open document (its handle, path, name, active flag and state) and the folders the user allows agents to save to and open from (folders; empty: none allowed). Handles become invalid when their document is closed or replaced. File imports and closing are not available through this connection. Disconnecting leaves the application's document open, including unsaved changes."
                );
            }
            Some("model_reference") => {
                tool["description"] = reference_tool()["description"].clone();
            }
            Some("model_query") => {
                tool["description"] = json!(
                    "Read the open document with one JSON model query, also while a sketch or command is open. Refused only while the model computes: the result then says busy and retry_after_ms. model_reference lists the query names and gives each one's arguments and result (identities and units too). measure's selections a and b also take an entity exactly as ui_state lists it (kind, owner, name, occurrence), origin datums by name (\"xy\", \"x\", \"origin\") and occurrence paths by ids or names (\"O1/O4\" or \"Arm:1/Pin:2\")."
                );
            }
            Some("model_command") => {
                tool["description"] = json!(format!(
                    "Apply a JSON model command, or an array of them in order, to the open document through its undo workflow as one undo step labeled (agent) in the undo text, which the user undoes with Ctrl+Z. While a sketch is open, sketch.* commands on it (and edit_feature of it, undo, redo) go through the sketch session as the user's would; other commands wait until the sketch or command panel is finished. Editing is refused when the document is read-only. Look up the command, or the feature type of add_feature, with model_reference first. File, remote and internal commands are refused. An array stops at the first refused entry (the entries before it are undone; the result names the entry) or at the first entry whose added or edited feature fails (applied; stopped_at names it); undo and redo run alone. isError is true only when a command is refused or a feature it added or edited fails; failed_features lists every failed feature in the timeline (edited, new). A failed feature stays in the timeline: edit it, or use undo or delete_feature. The result holds the document's state and what the command changed (changes: features added, changed, recomputed or failed, bodies added or changed, what was removed); timeline: full gives the whole timeline and bodies instead, none neither. hints gives up to 3 modeling tips for added/edited features; hints:false omits them. {SKETCH_IDS}"
                ));
            }
            // Into the folders for designs (mitcad#689), as
            // document_save's path.
            Some("model_export") => {
                tool["description"] = json!(
                    "Export the open document's geometry to a new file in a folder the user allows agents to use (document_list lists them); the document does not change. An existing file is replaced only with overwrite: true (each file the export writes, checked before any is written). bodies selects what is written (for example leaving out a helper body); refinement sets the mesh of STL, OBJ, 3MF and USDZ (low, medium by default, high, or {\"deviation\": mm, \"angle\": radians}; model_reference {\"section\": \"Mesh refinement\"}). DXF requires a sketch handle, or a sheet-metal body whose Flat Pattern is written, instead of bodies, unless drawing or cut_sheet is given; cut_sheet writes a cut sheet kept in the design (DXF or SVG at 1:1, model_reference {\"section\": \"Cut sheets\"}). PDF and SVG (and DXF with drawing) write a drawing of the bodies: views with hidden lines removed, overall dimensions, hole diameters, a hole table and a title block; drawing holds the export_drawing options (model_reference {\"section\": \"Drawings\"}). 3MF for a slicer: plates writes the design's print plates (one_file: every plate in one file, for Bambu Studio, OrcaSlicer, QIDI Studio and PrusaSlicer 2.9+; file_per_plate: a file per plate beside path, for Cura and older PrusaSlicer, each checked in an allowed folder before any is written; model_reference {\"section\": \"Print plates\"}), plate one of them, dialect the slicer whose settings go with the bodies (bambu_studio, orcaslicer, qidi_studio, prusaslicer, cura); on_bed, center, drop, auto_orient, bed_face and spacing lay the pieces on the print bed (also STL and OBJ; model_reference {\"section\": \"Print bed faces\"}), occurrences and whole pick placements. The result is the export command's (plates with each plate's path, origin, bed and warnings, and files). Refused while a command panel is open, while a sketch in the middle of the timeline is edited (its later features are rolled back until it is finished), or when the document has failed features. Defaults are metric."
                );
                tool["inputSchema"]["properties"]["path"]["description"] = json!(
                    "A new file (an existing one only with overwrite: true) in a folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder), with the format's extension; a relative path is relative to the first allowed folder; '..' is refused"
                );
            }
            // The headless server's fast picture (mitcad#1487).
            Some("model_render") => {
                tool["description"] = json!(format!(
                    "{RENDER_ABOUT} {RENDER_OPTIONS} {RENDER_OVERLAY} {RENDER_LIVE}"
                ));
                if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
                    for raytrace in ["quality", "samples", "time_limit_s", "denoise"] {
                        properties.remove(raytrace);
                    }
                }
                tool["inputSchema"]["properties"]["path"]["description"] = json!(
                    "Also write the PNG at full size to this new .png file in a folder the user allows agents to use (document_list lists them); a relative path is relative to the first allowed folder"
                );
            }
            _ => {}
        }
        if let Some(document) = tool["inputSchema"]["properties"].get_mut("document") {
            document["description"] = json!(LIVE_DOCUMENT);
        }
    }
    tools.extend(ui_tools());
    for tool in &mut tools {
        if matches!(
            tool["name"].as_str(),
            Some("document_list" | "model_reference")
        ) {
            continue;
        }
        tool["inputSchema"]["properties"]["document"] = json!({
            "type": "string", "minLength": 1, "maxLength": ID_CHARS,
            "description": LIVE_DOCUMENT
        });
        if let Some(required) = tool["inputSchema"]["required"].as_array_mut() {
            required.retain(|field| field != "document");
        }
    }
    tools
}

const LIVE_DOCUMENT: &str = "Optional live document handle from document_list; omitted uses the active document (chat agents remain bound to their own document, or to the last one they made with document_new)";

/// The application's own state and actions on the open document
/// (mitcad#265); the application executes them.
fn ui_tools() -> Vec<Value> {
    let document = json!({"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": LIVE_DOCUMENT});
    let kinds = json!([
        "face",
        "edge",
        "vertex",
        "body",
        "profile",
        "sketch_curve",
        "sketch_point",
        "sketch_constraint",
        "sketch_dimension",
        "plane",
        "axis",
        "point",
        "component",
        "sketch",
        "feature"
    ]);
    let item = json!({
        "type": "object",
        "description": "An entity as ui_state lists the selection: kind, owner (a body such as F2.b0, a sketch such as F4, a datum, a component), name (a face, edge or vertex name, a region key, a curve, point, constraint or dimension id; empty for bodies, datums and sketches), occurrence (\"O1/O4\"; empty in the root)",
        "properties": {
            "kind": {"type": "string", "enum": kinds},
            "owner": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS},
            "name": {"type": "string", "maxLength": ENTITY_CHARS},
            "occurrence": {"type": "string", "maxLength": NAME_CHARS}
        },
        "required": ["kind", "owner"],
        "additionalProperties": false
    });
    let folder_path = json!({"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "A .mitcad file in a folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder); a relative path is relative to the first allowed folder; '..' is refused"});
    let point = json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": "x, y, z in millimeters"});
    // A comment of the design by its id (mitcad#1119).
    let comment = json!({"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "A comment's id as the comments resource lists it, e.g. C3: its target and place"});
    vec![
        tool(
            "document_save",
            "Save the open document. Without path: to its own path as File > Save does, with the project's version and commit message when it is in a project. With path: as Save As, to that file in a folder the user allows agents to use (document_list lists them); the document then is that file. An existing file requires overwrite: true. Refused for a read-only document, while a command panel is open and while a dialog would have to ask the user.",
            json!({
                "document": document,
                "path": folder_path,
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file"}
            }),
            &["document"],
            false,
            true,
        ),
        tool(
            "document_open",
            "Open a MitCad design (.mitcad) in the selected application window in place of its document, from a folder the user allows agents to use (document_list lists them). The previous document handle becomes invalid; the result carries the new one. Refused while the open document has unsaved changes unless discard is true, while a sketch or command panel is open, and while a dialog would have to ask the user.",
            json!({
                "path": folder_path,
                "discard": {"type": "boolean", "default": false, "description": "Explicitly discard the open document's unsaved changes (they and their recovery data are lost)"}
            }),
            &["path"],
            false,
            true,
        ),
        tool(
            "ui_state",
            "The application's state: mode (idle, sketch, command, pick_plane), the active component, the sketch being edited with its Sketch Palette status and warnings (each with the entities it is about), the running command with its inputs and panel messages, the selection, the hint or error line, read-only state, unsaved changes, undo and redo labels, connected agents, the number of open comments (comments.open: notes the user pinned for an agent; read them first, from the resource mitcad://comments or model_query {\"query\": \"comments\", \"status\": \"open\"}), and the application's version and build.",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "ui_log",
            "The application's latest log lines (its own messages, command refusals), each with a sequence number. Pass since to get only lines after that number.",
            json!({
                "since": {"type": "integer", "minimum": 0, "description": "Only lines with a larger sequence number"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 2000, "default": 200, "description": "At most this many of the newest lines"}
            }),
            &[],
            true,
            false,
        ),
        tool(
            "ui_screenshot",
            "An image of the 3D view as the user sees it, including the sketch overlay, or of the whole window, scaled to fit max_size pixels and then down until its encoded image takes at most max_bytes bytes, returned as an MCP image (PNG, or JPEG with format; the format asked for is kept, and downscaled_for_bytes tells when the budget made it smaller). max_bytes counts the PNG or JPEG bytes (the result's bytes); the image block's base64 is 4/3 of that, and the rest of the response adds under 2 KiB. Its default, 5 MiB, keeps the response within Mitcad's own 8 MiB transport limit; a client with a lower limit of its own passes a smaller max_bytes (e.g. 500000). An image that cannot fit is refused with error_code screenshot_too_large. With path it is written to that file in a folder the user allows agents to use, and only the path, the size and the bytes are returned, at full size unless max_bytes is given. width and height, or aspect, render the 3D view offscreen at that size, without the widgets over it, so the picture does not take the window's shape; frame it first with ui_camera's aspect.",
            json!({
                "window": {"type": "boolean", "default": false, "description": "The whole window instead of the 3D view"},
                "max_size": {"type": "integer", "minimum": 64, "maximum": 4096, "default": 1280, "description": "The longer side's largest size in pixels"},
                "max_bytes": {"type": "integer", "minimum": SCREENSHOT_MIN_BYTES, "maximum": SCREENSHOT_INLINE_BYTES, "description": "The encoded PNG or JPEG image's largest size in bytes (not its base64): the picture is scaled down until it fits. Default 5 MiB for an image returned inline; a file written to path has no limit unless this is given"},
                "format": {"type": "string", "enum": ["png", "jpeg"], "description": "png (the default, or by the path's extension) or jpeg"},
                "quality": {"type": "integer", "minimum": 1, "maximum": 100, "default": 90, "description": "A JPEG's quality"},
                "width": {"type": "integer", "minimum": 16, "maximum": 4096, "description": "Pixels across an offscreen render of the 3D view (with height, or with aspect)"},
                "height": {"type": "integer", "minimum": 16, "maximum": 4096, "description": "Pixels down an offscreen render of the 3D view (with width, or with aspect)"},
                "aspect": {"type": "number", "minimum": 0.1, "maximum": 10, "description": "Width / height of an offscreen render of the 3D view; max_size on its longer side unless width or height is given"},
                "path": {"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "A .png or .jpg file in a folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder) to write the image to instead of returning it; a relative path is relative to the first allowed folder; '..' is refused"},
                "overwrite": {"type": "boolean", "default": false, "description": "Explicitly allow replacement of an existing file at path"}
            }),
            &[],
            false,
            false,
        ),
        tool(
            "ui_select",
            "Set the application's selection to the given entities, as clicks would, to show the user something; the user can then press Delete or run a command on them. Optionally pan or zoom the view to them. With comment instead of items: select that comment's target (a free point, a parameter or an orphaned comment's target selects nothing) and bring its place into the view.",
            json!({
                "document": document,
                "items": {"type": "array", "items": item, "description": "The entities; empty clears the selection"},
                "comment": comment,
                "reveal": {"type": "string", "enum": ["none", "pan", "fit"], "default": "pan", "description": "pan: move the view to them when none is in sight, keeping the zoom unless they do not fit; fit: zoom to them"}
            }),
            &["document"],
            false,
            false,
        ),
        tool(
            "ui_command",
            &format!(
                "Run a command of the application's command registry by its id (as app/COMMANDS.md lists them: sketch.clean_up_stale_projections, sketch.project, sketch.finish, ...) with input values, then OK or Cancel; or with edit, open a feature for editing (a sketch in sketch mode) as a double-click in the timeline does; or with only finish, end the open command panel, or with cancel a sketch being edited (its changes taken back). The result carries the panel's validation, preview and warnings. Commands that open a file dialog or another modal dialog are refused, and so is file.new_design (it waits for the user to name the design): document_new makes a new design. {UI_COMMAND_IDS}"
            ),
            json!({
                "document": document,
                "id": {"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "A command id of the registry; search lists them (an unknown id is refused with the nearest ids)"},
                "edit": {"type": "string", "minLength": 1, "maxLength": ID_CHARS, "description": "A feature uid to edit instead of running a command"},
                "inputs": {"type": "object", "description": "Input values by the panel's input keys: text for values and texts, a choice's value, true or false for checks, an array of entities (as ui_select's items) for selections"},
                "finish": {"type": "string", "enum": ["ok", "cancel", "keep_open"], "default": "ok", "description": "What ends a command panel"},
                // The commands an agent may run (mitcad#1269).
                "search": {"type": "string", "maxLength": NAME_CHARS, "description": "List the registry's commands instead of running one: each id, name, kind, whether an agent may run it (agents, with why not) and whether it is available now; the words given (all of them, any case) filter by id, name and keywords, \"\" lists all. Without id, edit or search and with nothing open to finish, the call lists them too"}
            }),
            &["document"],
            false,
            true,
        ),
        tool(
            "ui_camera",
            "Read or set the 3D view's camera, for inspection screenshots; the view changes at once (no animation) and the result is the camera as it then is. Without arguments it only reads. Given values replace the current camera's: eye and target (points), up, height (mm of the model the view shows at the target, top to bottom), fov (the perspective's vertical field of view in degrees), projection. Then box frames that box, or frame frames the shown bodies (only the isolated ones when the design isolates some) or the selection, seen along the resulting view direction with a small margin, in the part of the view no panel covers; or comment selects that comment's target and frames it (a free point or an orphaned comment's place is centred at the current zoom).",
            json!({
                "eye": point,
                "target": point,
                "up": point,
                "height": {"type": "number", "minimum": 0.000001, "description": "Millimeters of the model the view shows at the target, top to bottom"},
                "fov": {"type": "number", "minimum": 1, "maximum": 170, "description": "Vertical field of view of the perspective camera, degrees"},
                "projection": {"type": "string", "enum": ["orthographic", "perspective"]},
                "box": {
                    "type": "object",
                    "description": "A box in model coordinates to frame",
                    "properties": {"min": point, "max": point},
                    "required": ["min", "max"],
                    "additionalProperties": false
                },
                "frame": {"type": "string", "enum": ["none", "shown", "selection"], "default": "none", "description": "shown: the shown bodies (the isolated ones when some are isolated); selection: the selected entities"},
                "comment": comment,
                "aspect": {"type": "number", "minimum": 0.1, "maximum": 10, "description": "Frame box or frame for a picture of this width / height (ui_screenshot's aspect, or its width and height) instead of the view's own shape"}
            }),
            &[],
            false,
            false,
        ),
        // A new design beside the open ones (mitcad#1266), as File > New
        // Design makes it.
        tool(
            "document_new",
            "Make an empty design and open it in a window (a tab) of its own beside the open designs, as File > New Design does; it never replaces a design, and every open design keeps its handle. Returns the new design's handle; your calls without a document handle go to it from now on (a chat agent too), and the design you came from stays readable by its handle (previous in the result). Where: in folder (a folder the user allows agents to use, document_list lists them); without folder in the current project's top folder when agents may use it, else with a name in the first allowed folder, else an untitled design (save it later with document_save and a path). name is the file's name (.mitcad added), by default the first free DesignN. In a project with versions the design is recorded as New Design records it. A name that exists is refused. Wait for editable in document_list (or retry a busy result) before the first command.",
            json!({
                "document": document,
                "name": {"type": "string", "minLength": 1, "maxLength": NAME_CHARS, "description": "The design's file name, e.g. \"nipple\" (.mitcad is added); default the first free DesignN"},
                "folder": {"type": "string", "minLength": 1, "maxLength": PATH_CHARS, "description": "A folder the user allows agents to use (Preferences > AI Assistants, or the application's --mcp-folder), or one below it; a relative path is relative to the first allowed folder; '..' is refused. Default: the current project's top folder"}
            }),
            &[],
            false,
            false,
        ),
    ]
}

fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
    destructive: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        },
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "openWorldHint": false
        }
    })
}

/// What a tool's schema cannot say (mitcad#1119): `ui_select` takes
/// `items` or `comment`, `ui_camera`'s `comment` frames instead of `box` or
/// `frame`.
pub(crate) fn check(name: &str, arguments: &Value) -> Result<(), String> {
    let has = |key: &str| arguments.get(key).is_some();
    match name {
        "ui_select" if has("items") == has("comment") => {
            Err("ui_select takes either items or comment".to_owned())
        }
        "ui_camera"
            if has("comment")
                && (has("box") || arguments.get("frame").is_some_and(|f| f != "none")) =>
        {
            Err("ui_camera's comment frames that comment: give no box or frame with it".to_owned())
        }
        // A listing runs nothing (mitcad#1269).
        "ui_command" if has("search") && (has("id") || has("edit") || has("inputs")) => Err(
            "ui_command's search lists the commands: give no id, edit or inputs with it".to_owned(),
        ),
        _ => Ok(()),
    }
}

/// Validate the small, fixed schema vocabulary used by our tool catalog.
/// CAD command and query contents remain the document backend's responsibility.
pub(crate) fn validate(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let of_type = |name: &str| match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        _ => true,
    };
    // One type, or a list of them (mitcad#689: a command or an array).
    let valid_type = match schema.get("type") {
        Some(Value::String(name)) => of_type(name.as_str()),
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).any(of_type),
        _ => true,
    };
    if !valid_type {
        return Err(format!("{path} must be {}", schema["type"]));
    }
    if let Some(number) = value.as_f64() {
        if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
            && number < minimum
        {
            return Err(format!("{path} must be at least {minimum}"));
        }
        if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64)
            && number > maximum
        {
            return Err(format!("{path} must be at most {maximum}"));
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        return Err(format!("{path} must be one of {}", schema["enum"]));
    }
    if let Some(string) = value.as_str()
        && let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
        && (string.chars().count() as u64) < minimum
    {
        return Err(format!("{path} must not be empty"));
    }
    // A string's length in characters (mitcad#935): counted only up to the
    // limit, a megabyte string is refused without being walked.
    if let Some(string) = value.as_str()
        && let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64)
        && string
            .chars()
            .nth(usize::try_from(maximum).unwrap_or(usize::MAX))
            .is_some()
    {
        return Err(format!("{path} must have at most {maximum} characters"));
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}.{key} is required"));
                }
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, value) in object {
            if let Some(property) = properties.and_then(|properties| properties.get(key)) {
                validate(value, property, &format!("{path}.{key}"))?;
            } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                return Err(format!("Unknown argument {path}.{}", echoed(key)));
            }
        }
    }
    if let Some(array) = value.as_array() {
        if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64)
            && (array.len() as u64) < minimum
        {
            return Err(format!("{path} must have at least {minimum} items"));
        }
        if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64)
            && (array.len() as u64) > maximum
        {
            return Err(format!("{path} must have at most {maximum} items"));
        }
    }
    if let Some(array) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for (index, value) in array.iter().enumerate() {
            validate(value, items, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}
