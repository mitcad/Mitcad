// SPDX-License-Identifier: MIT
//! Filament changes at print heights (mitcad#1596) on the mock kernel's
//! box meshes, all through the JSON api that MCP's model_command and
//! model_query pass on: a 20 x 20 x 40 box of a material in two colours
//! (red, blue) with one change height, edited, undone, redone, saved and
//! reopened; each slicer's 3MF events read back from the file (the exact
//! XML the slicers' own readers take), a rotated print bed and two bodies
//! sharing a plate (and a conflict of two filaments), a print plate's own
//! schedule; and the opt-in print colours of `model_render` and the USDZ
//! export, both cut at the same layer boundary, the design unchanged.

use serde_json::{Map, Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::preview::{Preview, RenderArgs, png};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

use super::print_files_tests::command;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was taken: {result}"),
        Err(e) => e.to_string(),
    }
}

/// A `width` x 20 x `height` block on a rectangle on XY at `x` (its sketch
/// `sketch`, its extrude the next feature).
fn add_block(doc: &mut Document<MockKernel>, sketch: &str, x: f64, height: f64) {
    command(
        doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [x, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
              "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
        ]),
    );
}

/// A bench printer and a 0.2 mm process at the root, and the material
/// `pla_two` in red (its default) and blue.
fn print_setup(doc: &mut Document<MockKernel>) {
    command(
        doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
             "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]],
                        "max_print_height": 250, "nozzle_diameter": 0.4, "filament_slots": 4}},
            {"cmd": "create_print_profile", "kind": "process", "name": "Standard",
             "values": {"layer_height": 0.2}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
             "value": "bench"},
            {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile",
             "value": "standard"},
            {"cmd": "create_appearance", "id": "red_pla", "name": "Red", "base_color": [1.0, 0.0, 0.0]},
            {"cmd": "create_appearance", "id": "blue_pla", "name": "Blue", "base_color": [0.0, 0.0, 1.0]},
            {"cmd": "create_material", "id": "pla_two", "name": "PLA Two", "density": 1.24,
             "class": "plastic", "appearances": ["red_pla", "blue_pla"],
             "filament": {"filament_type": "PLA", "temperature": 215}},
        ]),
    );
}

/// Gives a body the material in a colour.
fn colour(doc: &mut Document<MockKernel>, body: &str, appearance: &str) {
    command(
        doc,
        json!({"cmd": "set_body_material", "uid": body, "material": "pla_two",
               "appearance_id": appearance}),
    );
}

/// The synthetic box: 20 x 20 x 40 (F2.b0, Body1), red.
fn red_box() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    add_block(&mut doc, "F1", 0.0, 40.0);
    print_setup(&mut doc);
    colour(&mut doc, "F2.b0", "red_pla");
    doc
}

/// The change to blue at `height` mm.
fn to_blue(height: f64) -> Value {
    json!({"height": height, "material": "pla_two", "appearance": "blue_pla"})
}

fn set(doc: &mut Document<MockKernel>, changes: Value) -> Value {
    command(
        doc,
        json!({"cmd": "set_filament_changes", "changes": changes}),
    )
}

fn folder(name: &str) -> std::path::PathBuf {
    mitcad_model::testing::unique_temp_dir(name)
}

/// Exports to `name` in a folder of its own with the fields `more`; the
/// result and the file.
fn export(doc: &mut Document<MockKernel>, name: &str, more: Value) -> (Value, Vec<u8>) {
    let path = folder("filament-changes").join(name);
    let mut export = json!({"cmd": "export", "path": path.to_str().unwrap()});
    for (key, value) in more.as_object().unwrap() {
        export[key] = value.clone();
    }
    let result = command(doc, export);
    (result, std::fs::read(&path).unwrap())
}

/// A file of a 3MF package as text, or none.
fn package_text(data: &[u8], name: &str) -> Option<String> {
    let package = mitcad_3mf::read_package(data).unwrap();
    package
        .file(name)
        .map(|bytes| std::str::from_utf8(bytes).unwrap().to_owned())
}

const BAMBU: &str = "Metadata/custom_gcode_per_layer.xml";
const PRUSA: &str = "Metadata/Prusa_Slicer_custom_gcode_per_print_z.xml";

fn warnings(result: &Value) -> Vec<String> {
    result["warnings"]
        .as_array()
        .map(|w| w.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

#[test]
fn a_two_colour_box_s_change_is_edited_undone_redone_saved_and_reopened() {
    let mut doc = red_box();
    let answer = set(&mut doc, json!([to_blue(10.0)]));
    assert_eq!(answer["changes"], json!([to_blue(10.0)]));
    assert_eq!(doc.undo_label(), Some("Set Filament Changes"));
    // The query: the change on its layer boundary (10 mm: the top of layer
    // 50 of 0.2 mm), written at the top of layer 51, in blue.
    let listed = query(&doc, json!({"query": "filament_changes"}));
    assert_eq!(listed["layer_height"], json!(0.2));
    assert_eq!(listed["first_layer_height"], json!(0.2));
    assert_eq!(listed["inherited"], false);
    let change = &listed["changes"][0];
    assert_eq!(change["kind"], "change");
    assert_eq!(change["material_name"], "PLA Two");
    assert_eq!(change["color"], "#0000FF");
    assert_eq!(change["boundary"], json!(10.0));
    assert_eq!(change["print_z"], json!(10.2));
    assert_eq!(change["layer"], 51);
    // A height between layers moves to the nearest boundary.
    set(&mut doc, json!([to_blue(10.07)]));
    let listed = query(&doc, json!({"query": "filament_changes"}));
    assert_eq!(listed["changes"][0]["boundary"], json!(10.0));
    assert_eq!(listed["changes"][0]["height"], json!(10.07));
    doc.undo();

    // Undone, redone.
    doc.undo();
    let listed = query(&doc, json!({"query": "filament_changes"}));
    assert_eq!(listed["changes"], json!([]));
    assert!(doc.filament_changes(None).is_empty());
    doc.redo();
    assert_eq!(doc.filament_changes(None).len(), 1);
    assert_eq!(doc.filament_changes(None)[0].height, 10.0);

    // Saved with the design and reopened intact.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["filament_changes"],
        json!([{"changes": [to_blue(10.0)]}])
    );
    let reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(reopened.filament_changes(None), doc.filament_changes(None));
    assert_eq!(reopened.to_json(), saved);

    // A pause beside it, sorted upwards, one undo step.
    set(
        &mut doc,
        json!([to_blue(10.0), {"height": 4, "kind": "pause", "message": "Insert the magnets"}]),
    );
    let listed = query(&doc, json!({"query": "filament_changes"}));
    assert_eq!(listed["changes"][0]["kind"], "pause");
    assert_eq!(listed["changes"][0]["message"], "Insert the magnets");
    assert_eq!(listed["changes"][1]["height"], json!(10.0));
    // Setting the same schedule again is no step.
    set(
        &mut doc,
        json!([{"height": 4, "kind": "pause", "message": "Insert the magnets"}, to_blue(10.0)]),
    );
    doc.undo();
    assert_eq!(doc.filament_changes(None).len(), 1);

    // Refused: what is not a schedule.
    for (changes, expected) in [
        (
            json!([{"height": 5, "material": "nothing"}]),
            "unknown material",
        ),
        (
            json!([{"height": 5, "material": "pla_two", "appearance": "nothing"}]),
            "unknown appearance",
        ),
        (json!([{"height": 5}]), "names the material"),
        (
            json!([{"height": 5, "kind": "pause", "material": "pla_two"}]),
            "keeps the filament",
        ),
        (json!([to_blue(5.0), to_blue(5.0)]), "one event per height"),
        (json!([to_blue(0.0)]), "above 0"),
        (json!([to_blue(-2.0)]), "above 0"),
        (json!([to_blue(5000.0)]), "at most 2000"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "set_filament_changes", "changes": changes}),
        );
        assert!(error.contains(expected), "{expected}: {error}");
    }
    let error = refused(
        &mut doc,
        json!({"cmd": "set_filament_changes", "plate": "nowhere", "changes": []}),
    );
    assert!(error.contains("nowhere"), "{error}");

    // Cleared: an empty schedule, which the file leaves out.
    set(&mut doc, json!([]));
    assert_eq!(doc.undo_label(), Some("Clear Filament Changes"));
    assert!(!doc.to_json().contains("\"filament_changes\""));
    doc.undo();
    assert_eq!(doc.filament_changes(None).len(), 1);
}

#[test]
fn each_slicer_s_project_gets_the_change_as_its_reader_takes_it() {
    let mut doc = red_box();
    let (plain, _) = export(&mut doc, "plain.3mf", json!({"dialect": "bambu_studio"}));
    set(
        &mut doc,
        json!([{"height": 4, "kind": "pause", "message": "Insert <magnets>"}, to_blue(10.0)]),
    );

    // The Bambu family: a filament change (ToolChange, type 2) to the
    // second filament slot, which the export adds in blue; a pause (type
    // 1) of the filament printing.
    let (result, data) = export(&mut doc, "bambu.3mf", json!({"dialect": "bambu_studio"}));
    assert_eq!(result["filaments"], json!([1]), "{result}");
    assert_eq!(
        result["filament_changes"],
        json!([
            {"plate": 0, "height": 4.0, "print_z": 4.2, "layer": 21, "kind": "pause",
             "material": null},
            {"plate": 0, "height": 10.0, "print_z": 10.2, "layer": 51, "kind": "change",
             "material": "pla_two"},
        ])
    );
    let settings: Map<String, Value> =
        serde_json::from_str(&package_text(&data, "Metadata/project_settings.config").unwrap())
            .unwrap();
    assert_eq!(settings["filament_colour"], json!(["#FF0000", "#0000FF"]));
    assert_eq!(settings["filament_type"], json!(["PLA", "PLA"]));
    assert_eq!(
        package_text(&data, BAMBU).unwrap(),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <custom_gcodes_per_layer>\n\
         <plate>\n\
         <plate_info id=\"1\"/>\n\
         <layer top_z=\"4.2\" type=\"1\" extruder=\"1\" color=\"\" extra=\"Insert &lt;magnets&gt;\"/>\n\
         <layer top_z=\"10.2\" type=\"2\" extruder=\"2\" color=\"#0000FF\" extra=\"\"/>\n\
         <mode value=\"MultiAsSingle\"/>\n\
         </plate>\n\
         </custom_gcodes_per_layer>\n"
    );
    // The geometry is the design's: the same mesh as without the changes.
    assert_eq!(result["meshes"], plain["meshes"]);
    assert_eq!(mitcad_3mf::read(&data).unwrap().parts.len(), 1);
    // OrcaSlicer and QIDI Studio read the same file.
    for dialect in ["orcaslicer", "qidi_studio"] {
        let (_, data) = export(&mut doc, "family.3mf", json!({"dialect": dialect}));
        assert!(
            package_text(&data, BAMBU)
                .unwrap()
                .contains("<layer top_z=\"10.2\" type=\"2\" extruder=\"2\""),
            "{dialect}"
        );
    }

    // PrusaSlicer: a colour change (type 0) of its one extruder, a pause
    // (type 1) with its message; one filament in the project.
    let (result, data) = export(&mut doc, "prusa.3mf", json!({"dialect": "prusaslicer"}));
    assert_eq!(result["filament_changes"].as_array().unwrap().len(), 2);
    assert_eq!(
        package_text(&data, PRUSA).unwrap(),
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <custom_gcodes_per_print_z bed_idx=\"0\">\n\
         <code print_z=\"4.2\" type=\"1\" extruder=\"1\" color=\"\" extra=\"Insert &lt;magnets&gt;\"/>\n\
         <code print_z=\"10.2\" type=\"0\" extruder=\"1\" color=\"#0000FF\" extra=\"\"/>\n\
         <mode value=\"SingleExtruder\"/>\n\
         </custom_gcodes_per_print_z>\n"
    );
    let config = package_text(&data, "Metadata/Slic3r_PE.config").unwrap();
    assert!(config.contains("; filament_colour = #FF0000\n"), "{config}");
    // The same material in another colour: no settings change to warn of.
    assert!(
        warnings(&result).iter().all(|w| !w.contains("colour only")),
        "{result}"
    );

    // Cura keeps no such events: a warning names each one's layer.
    let (result, data) = export(&mut doc, "cura.3mf", json!({"dialect": "cura"}));
    assert!(package_text(&data, BAMBU).is_none() && package_text(&data, PRUSA).is_none());
    assert!(result.get("filament_changes").is_none(), "{result}");
    let said = warnings(&result).join("\n");
    assert!(
        said.contains("a Cura project keeps no filament changes"),
        "{said}"
    );
    assert!(
        said.contains("layer 51 (10.2 mm) a change to PLA Two (#0000FF)"),
        "{said}"
    );
    assert!(
        said.contains("layer 21 (4.2 mm) a pause: Insert <magnets>"),
        "{said}"
    );

    // Without a slicer: none written, and said so.
    let (result, data) = export(&mut doc, "none.3mf", json!({}));
    assert!(package_text(&data, BAMBU).is_none() && package_text(&data, PRUSA).is_none());
    assert!(
        warnings(&result)
            .iter()
            .any(|w| w.contains("give the slicer (dialect)")),
        "{result}"
    );
}

/// A Bambu-family project's `project_settings.config`.
fn project_settings(data: &[u8]) -> Map<String, Value> {
    serde_json::from_str(&package_text(data, "Metadata/project_settings.config").unwrap()).unwrap()
}

/// The XY box of a 3MF's placed meshes, (min, max), to a nanometre.
fn xy_box(data: &[u8]) -> ([f64; 2], [f64; 2]) {
    let model = mitcad_3mf::read(data).unwrap();
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for part in &model.parts {
        for placement in &part.placements {
            for vertex in &part.mesh.vertices {
                let [x, y, _] = placement.apply(*vertex);
                min = [min[0].min(x), min[1].min(y)];
                max = [max[0].max(x), max[1].max(y)];
            }
        }
    }
    let nm = |v: [f64; 2]| v.map(|c| (c * 1e6).round() / 1e6 + 0.0);
    (nm(min), nm(max))
}

/// Whether an entry of `different_settings_to_system` (0 the process's,
/// the last the printer's) names `key`.
fn differs(settings: &Map<String, Value>, entry: usize, key: &str) -> bool {
    settings["different_settings_to_system"][entry]
        .as_str()
        .unwrap()
        .split(';')
        .any(|k| k == key)
}

/// The printer settings OrcaSlicer takes with relative extrusion: E reset
/// at each layer by a line `G92 E0` of the before or after layer change
/// G-code (its `Print::validate`), here the one the export writes for a
/// printer without layer G-code of its own (mitcad#1596).
fn assert_e_reset_each_layer(settings: &Map<String, Value>, what: &str) {
    assert_eq!(settings["use_relative_e_distances"], "1", "{what}");
    assert_eq!(settings["before_layer_change_gcode"], "G92 E0", "{what}");
    let last = settings["different_settings_to_system"]
        .as_array()
        .unwrap()
        .len()
        - 1;
    assert!(
        differs(settings, last, "before_layer_change_gcode"),
        "{what}"
    );
    assert!(
        differs(settings, last, "use_relative_e_distances"),
        "{what}"
    );
}

#[test]
fn the_bambu_family_s_projects_slice_on_a_printer_of_their_own() {
    // The real slicer run of mitcad#1596: the box on the 256 x 256 bench
    // printer, red, blue from 10 mm.
    let mut doc = red_box();
    // Without changes: the prime tower is the slicer's (one filament), E is
    // reset at each layer.
    for dialect in ["bambu_studio", "orcaslicer", "qidi_studio"] {
        let (_, data) = export(&mut doc, "plain.3mf", json!({"dialect": dialect}));
        let settings = project_settings(&data);
        assert!(!settings.contains_key("enable_prime_tower"), "{dialect}");
        assert!(!settings.contains_key("change_filament_gcode"), "{dialect}");
        assert_e_reset_each_layer(&settings, dialect);
        // The box from the design's origin lies in the middle of the bed,
        // not at its corner (where its skirt would be off the bed).
        assert_eq!(xy_box(&data), ([118.0, 118.0], [138.0, 138.0]), "{dialect}");
    }
    // Every slicer's project is centred; `center: false` keeps the design's
    // place, and a file for no slicer is the design's.
    let (_, data) = export(&mut doc, "prusa.3mf", json!({"dialect": "prusaslicer"}));
    assert_eq!(xy_box(&data), ([118.0, 118.0], [138.0, 138.0]));
    let (_, data) = export(
        &mut doc,
        "kept.3mf",
        json!({"dialect": "orcaslicer", "center": false}),
    );
    assert_eq!(xy_box(&data), ([0.0, 0.0], [20.0, 20.0]));
    let (_, data) = export(&mut doc, "design.3mf", json!({}));
    assert_eq!(xy_box(&data), ([0.0, 0.0], [20.0, 20.0]));

    // The manual change: the second filament slot is only the change's, so
    // no prime tower, which the slicers would place outside this bed; the
    // event as before.
    set(&mut doc, json!([to_blue(10.0)]));
    for dialect in ["bambu_studio", "orcaslicer", "qidi_studio"] {
        let (result, data) = export(&mut doc, "change.3mf", json!({"dialect": dialect}));
        let settings = project_settings(&data);
        assert_eq!(
            settings["filament_colour"],
            json!(["#FF0000", "#0000FF"]),
            "{dialect}"
        );
        assert_eq!(settings["enable_prime_tower"], "0", "{dialect}");
        assert!(differs(&settings, 0, "enable_prime_tower"), "{dialect}");
        assert_e_reset_each_layer(&settings, dialect);
        assert!(
            warnings(&result).iter().all(|w| !w.contains("prime tower")),
            "{result}"
        );
        // Two filaments by their diameters, as the slicers count them (one
        // entry: a colour change, which their G-code leaves out).
        assert_eq!(
            settings["filament_diameter"],
            json!(["1.75", "1.75"]),
            "{dialect}"
        );
        // The printer has no filament change G-code: M600 at each change
        // after the first filament, said so.
        assert_eq!(
            settings["change_filament_gcode"],
            "{if previous_extruder > -1}{if previous_extruder != next_extruder}M600{endif}{endif}",
            "{dialect}"
        );
        let last = settings["different_settings_to_system"]
            .as_array()
            .unwrap()
            .len()
            - 1;
        assert!(
            differs(&settings, last, "change_filament_gcode"),
            "{dialect}"
        );
        assert!(
            warnings(&result)
                .iter()
                .any(|w| w.contains("written with M600 as its change_filament_gcode")),
            "{result}"
        );
        assert_eq!(xy_box(&data), ([118.0, 118.0], [138.0, 138.0]), "{dialect}");
        assert!(
            package_text(&data, BAMBU)
                .unwrap()
                .contains("<layer top_z=\"10.2\" type=\"2\" extruder=\"2\""),
            "{dialect}"
        );
    }

    // A design that turned the tower on: off for the manual change, and
    // said so.
    command(
        &mut doc,
        json!([{"cmd": "set_property", "owner": "C0", "set": "Print Process",
                "key": "wipe_tower", "value": true}]),
    );
    let (result, data) = export(&mut doc, "tower.3mf", json!({"dialect": "orcaslicer"}));
    assert_eq!(project_settings(&data)["enable_prime_tower"], "0");
    assert!(
        warnings(&result)
            .iter()
            .any(|w| w.contains("the prime tower is written off")),
        "{result}"
    );

    // PrusaSlicer is unchanged: a colour change of its one extruder, no
    // wipe tower of a second filament.
    let (_, data) = export(&mut doc, "prusa.3mf", json!({"dialect": "prusaslicer"}));
    let xml = package_text(&data, PRUSA).unwrap();
    assert!(xml.contains("<code print_z=\"10.2\" type=\"0\" extruder=\"1\" color=\"#0000FF\""));
    let config = package_text(&data, "Metadata/Slic3r_PE.config").unwrap();
    assert!(config.contains("; filament_colour = #FF0000\n"), "{config}");
    assert!(!config.contains("before_layer_gcode"), "{config}");
}

#[test]
fn a_plate_of_several_filaments_keeps_its_prime_tower() {
    // The box (F2.b0) on the first plate, red, blue from 20 mm; two short
    // blocks (F4.b0 red, F6.b0 blue) on a plate of their own: a print of
    // two filaments, which needs the tower.
    let mut doc = Document::new(MockKernel::default());
    add_block(&mut doc, "F1", 0.0, 40.0);
    add_block(&mut doc, "F3", 50.0, 10.0);
    add_block(&mut doc, "F5", 80.0, 10.0);
    print_setup(&mut doc);
    colour(&mut doc, "F2.b0", "red_pla");
    colour(&mut doc, "F4.b0", "red_pla");
    colour(&mut doc, "F6.b0", "blue_pla");
    for body in ["F2.b0", "F4.b0", "F6.b0"] {
        let face = format!("{}:start({R})", &body[..2]);
        command(
            &mut doc,
            json!({"cmd": "set_print_bed_face", "uid": body, "face": face}),
        );
    }
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "Short"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "Short",
               "items": [{"body": "F4.b0"}, {"body": "F6.b0"}]}),
    );
    set(&mut doc, json!([to_blue(20.0)]));
    let (result, data) = export(
        &mut doc,
        "plates.3mf",
        json!({"dialect": "bambu_studio", "plates": "one_file"}),
    );
    // The first plate's change is written; the tower stays the slicer's.
    assert!(
        package_text(&data, BAMBU)
            .unwrap()
            .contains("<layer top_z=\"20.2\" type=\"2\" extruder=\"2\""),
        "{result}"
    );
    let settings = project_settings(&data);
    assert!(!settings.contains_key("enable_prime_tower"), "{result}");
    assert!(!settings.contains_key("change_filament_gcode"), "{result}");
    assert_eq!(settings["filament_diameter"], json!(["1.75", "1.75"]));
    assert_e_reset_each_layer(&settings, "plates");
}

#[test]
fn the_heights_are_measured_on_the_print_bed_after_the_turn() {
    let mut doc = red_box();
    // Blue from 10 mm, red again from 30 mm.
    set(
        &mut doc,
        json!([to_blue(10.0), {"height": 30, "material": "pla_two", "appearance": "red_pla"}]),
    );
    // Standing as designed, 40 mm high: both changes.
    let (result, data) = export(&mut doc, "upright.3mf", json!({"dialect": "prusaslicer"}));
    let xml = package_text(&data, PRUSA).unwrap();
    assert!(xml.contains("<code print_z=\"10.2\" type=\"0\" extruder=\"1\" color=\"#0000FF\""));
    assert!(xml.contains("<code print_z=\"30.2\" type=\"0\" extruder=\"1\" color=\"#FF0000\""));
    assert!(
        warnings(&result)
            .iter()
            .all(|w| !w.contains("top of the print"))
    );

    // On its side on the print bed (its print bed face a side), 20 mm
    // high: the change at 30 mm is above the print, left out and said so.
    // The mock kernel knows the side y = 0 as a plane facing -Y when told,
    // as the print bed tests tell it.
    doc.kernel().surfaces.borrow_mut().insert(
        "F2:side(c1[c4,c2])".to_owned(),
        SurfaceGeometry::Plane {
            origin: [10.0, 0.0, 20.0],
            normal: [0.0, -1.0, 0.0],
        },
    );
    command(
        &mut doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": "F2:side(c1[c4,c2])"}),
    );
    let (result, data) = export(
        &mut doc,
        "turned.3mf",
        json!({"dialect": "prusaslicer", "on_bed": true}),
    );
    let model = mitcad_3mf::read(&data).unwrap();
    let placement = model.parts[0].placements[0];
    let (low, high) = model.parts[0]
        .mesh
        .vertices
        .iter()
        .map(|v| placement.apply(*v)[2])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| {
            (lo.min(z), hi.max(z))
        });
    assert!(
        low.abs() < 1e-6 && (high - 20.0).abs() < 1e-6,
        "{low} {high}"
    );
    let xml = package_text(&data, PRUSA).unwrap();
    assert!(xml.contains("<code print_z=\"10.2\" type=\"0\""), "{xml}");
    assert!(!xml.contains("30.2"), "{xml}");
    assert!(
        warnings(&result).iter().any(|w| w
            .contains("the change at 30 mm is at or above the top of the print (20 mm): left out")),
        "{result}"
    );
    assert_eq!(result["filament_changes"].as_array().unwrap().len(), 1);
}

#[test]
fn bodies_sharing_a_plate_change_together_and_a_plate_may_have_its_own() {
    // The box (F2.b0) and a second block beside it, 10 mm high (F4.b0),
    // both of the red filament.
    let mut doc = Document::new(MockKernel::default());
    add_block(&mut doc, "F1", 0.0, 40.0);
    add_block(&mut doc, "F3", 50.0, 10.0);
    print_setup(&mut doc);
    colour(&mut doc, "F2.b0", "red_pla");
    colour(&mut doc, "F4.b0", "red_pla");
    set(&mut doc, json!([to_blue(20.0)]));
    // One plate of both: the change is the whole layer's, once; the plate's
    // top is the taller body's.
    let (result, data) = export(&mut doc, "shared.3mf", json!({"dialect": "bambu_studio"}));
    assert_eq!(result["filaments"], json!([1, 1]), "{result}");
    let xml = package_text(&data, BAMBU).unwrap();
    assert_eq!(xml.matches("<layer ").count(), 1, "{xml}");
    assert!(xml.contains("<layer top_z=\"20.2\" type=\"2\" extruder=\"2\" color=\"#0000FF\""));
    assert_eq!(result["filament_changes"].as_array().unwrap().len(), 1);

    // The short block in blue: two filaments below the change, which a
    // single extruder's manual change cannot print: a conflict, nothing
    // written.
    colour(&mut doc, "F4.b0", "blue_pla");
    let (result, data) = export(&mut doc, "conflict.3mf", json!({"dialect": "prusaslicer"}));
    assert!(package_text(&data, PRUSA).is_none());
    assert!(result.get("filament_changes").is_none(), "{result}");
    assert!(
        warnings(&result).iter().any(|w| w.contains(
            "its parts print with 2 filaments (PLA Two (Red), PLA Two (Blue)): none is written"
        )),
        "{result}"
    );
    colour(&mut doc, "F4.b0", "red_pla");

    // Print plates: the short block on a plate of its own with a pause of
    // its own; the first plate takes the design's schedule.
    for body in ["F2.b0", "F4.b0"] {
        let face = format!("{}:start({R})", &body[..2]);
        command(
            &mut doc,
            json!({"cmd": "set_print_bed_face", "uid": body, "face": face}),
        );
    }
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "Short"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "Short", "items": [{"body": "F4.b0"}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_filament_changes", "plate": "Short",
               "changes": [{"height": 5, "kind": "pause", "message": "Nuts"}]}),
    );
    assert_eq!(doc.undo_label(), Some("Set Filament Changes of Short"));
    let short = query(&doc, json!({"query": "filament_changes", "plate": "Short"}));
    assert_eq!(short["inherited"], false);
    assert_eq!(short["plate"]["name"], "Short");
    assert_eq!(short["changes"][0]["kind"], "pause");
    let first = query(
        &doc,
        json!({"query": "filament_changes", "plate": "Plate 1"}),
    );
    assert_eq!(first["inherited"], true);
    assert_eq!(first["changes"][0]["height"], json!(20.0));
    let (result, data) = export(
        &mut doc,
        "plates.3mf",
        json!({"dialect": "bambu_studio", "plates": "one_file"}),
    );
    let xml = package_text(&data, BAMBU).unwrap();
    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <custom_gcodes_per_layer>\n\
         <plate>\n\
         <plate_info id=\"1\"/>\n\
         <layer top_z=\"20.2\" type=\"2\" extruder=\"2\" color=\"#0000FF\" extra=\"\"/>\n\
         <mode value=\"MultiAsSingle\"/>\n\
         </plate>\n\
         <plate>\n\
         <plate_info id=\"2\"/>\n\
         <layer top_z=\"5.2\" type=\"1\" extruder=\"1\" color=\"\" extra=\"Nuts\"/>\n\
         <mode value=\"SingleExtruder\"/>\n\
         </plate>\n\
         </custom_gcodes_per_layer>\n",
        "{result}"
    );
    // PrusaSlicer 2.9's beds: a root per bed.
    let (_, data) = export(
        &mut doc,
        "beds.3mf",
        json!({"dialect": "prusaslicer", "plates": "one_file"}),
    );
    let xml = package_text(&data, PRUSA).unwrap();
    assert!(xml.contains("<custom_gcodes_per_print_z bed_idx=\"0\">\n<code print_z=\"20.2\""));
    assert!(
        xml.contains("<custom_gcodes_per_print_z bed_idx=\"1\">\n<code print_z=\"5.2\" type=\"1\"")
    );

    // The plate's schedule is saved, and goes with the plate.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["filament_changes"].as_array().unwrap().len(), 2);
    let reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(reopened.filament_changes(Some("plate2")).len(), 1);
    command(
        &mut doc,
        json!({"cmd": "delete_print_plate", "plate": "Short"}),
    );
    let all = query(&doc, json!({"query": "filament_changes"}));
    assert_eq!(
        all["schedules"],
        json!([{"plate": null, "name": null, "events": 1}])
    );
    doc.undo();
    let all = query(&doc, json!({"query": "filament_changes"}));
    assert_eq!(all["schedules"].as_array().unwrap().len(), 2);
}

/// The picture's pixel at `(right, up)` mm from the camera's target in an
/// orthographic view.
fn pixel(preview: &Preview, right: f64, up: f64) -> [u8; 3] {
    let (width, height, channels, pixels) = png::decode(&preview.full.png).unwrap();
    let scale = f64::from(height) / preview.camera.height;
    let x = (f64::from(width) / 2.0 + right * scale).floor() as usize;
    let y = (f64::from(height) / 2.0 - up * scale).floor() as usize;
    let i = (y * width as usize + x) * channels;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

fn render(doc: &Document<MockKernel>, arguments: Value) -> Preview {
    let args = RenderArgs::from_json(&arguments).unwrap_or_else(|e| panic!("{arguments}: {e}"));
    doc.render_preview(&args, None)
        .unwrap_or_else(|e| panic!("{arguments}: {e}"))
}

/// Reddish: more red than blue, by a margin.
fn reddish(c: [u8; 3]) -> bool {
    u16::from(c[0]) > u16::from(c[2]) + 40
}

fn bluish(c: [u8; 3]) -> bool {
    u16::from(c[2]) > u16::from(c[0]) + 40
}

#[test]
fn render_and_usdz_show_the_same_two_height_regions_without_changing_the_design() {
    let mut doc = red_box();
    set(&mut doc, json!([to_blue(20.0)]));
    let design = doc.to_json();
    let front = |print_colors: bool| {
        json!({"view": "front", "projection": "orthographic", "size": [160, 240],
               "edges": false, "print_colors": print_colors})
    };
    // Drawn as designed: red all over.
    let plain = render(&doc, front(false));
    assert!(reddish(pixel(&plain, 0.0, -10.0)) && reddish(pixel(&plain, 0.0, 10.0)));
    // In print colours: red below 20 mm, blue above (the camera's target
    // is the box's middle, 20 mm up).
    let colours = render(&doc, front(true));
    let below = pixel(&colours, 0.0, -10.0);
    let above = pixel(&colours, 0.0, 10.0);
    assert!(reddish(below), "{below:?}");
    assert!(bluish(above), "{above:?}");
    assert!(reddish(pixel(&colours, 0.0, -1.0)) && bluish(pixel(&colours, 0.0, 1.0)));
    // The ray tracer does not draw them.
    assert!(
        RenderArgs::from_json(&front(true))
            .unwrap()
            .inspect
            .given()
            .contains(&"print_colors")
    );
    assert_eq!(doc.to_json(), design);

    // USDZ: the same cut at 20 mm, the blue band a subset of its own.
    let path = folder("filament-usdz").join("colours.usdz");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "print_colors": true}),
    );
    let materials = result["materials"].as_array().unwrap();
    assert!(materials.contains(&json!("print color ff0000")), "{result}");
    assert!(materials.contains(&json!("print color 0000ff")), "{result}");
    let package = mitcad_usdz::read_package(&std::fs::read(&path).unwrap()).unwrap();
    let layer = &package.layer;
    let Some(mitcad_usdz::sdf::Value::Vec3fArray(points)) =
        layer.field("/colours/Body1.points", "default")
    else {
        panic!("no points");
    };
    let Some(mitcad_usdz::sdf::Value::IntArray(corners)) =
        layer.field("/colours/Body1.faceVertexIndices", "default")
    else {
        panic!("no triangles");
    };
    let Some(mitcad_usdz::sdf::Value::IntArray(blue)) =
        layer.field("/colours/Body1/PrintColor_1.indices", "default")
    else {
        panic!("no blue band");
    };
    let triangles = corners.len() / 3;
    assert!(triangles > 12, "the box's sides cut: {triangles}");
    for t in 0..triangles {
        let z: Vec<f32> = (0..3)
            .map(|k| points[corners[3 * t + k] as usize][2])
            .collect();
        if blue.contains(&(t as i32)) {
            assert!(z.iter().all(|&z| z >= 20.0 - 1e-4), "{t}: {z:?}");
        } else {
            assert!(z.iter().all(|&z| z <= 20.0 + 1e-4), "{t}: {z:?}");
        }
    }
    assert!(!blue.is_empty());
    // Without print colours: the body as designed, uncut.
    let path = folder("filament-usdz").join("plain.usdz");
    command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap()}),
    );
    let package = mitcad_usdz::read_package(&std::fs::read(&path).unwrap()).unwrap();
    assert!(
        package
            .layer
            .field("/plain/Body1/PrintColor_1.indices", "default")
            .is_none()
    );
    // Only USDZ files take them.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": "x.3mf", "print_colors": true}),
    );
    assert!(error.contains("USDZ"), "{error}");
    assert_eq!(doc.to_json(), design);
}
