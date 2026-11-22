// SPDX-License-Identifier: MIT
//! The `G92 E0` reset of relative extrusion in a Bambu-family project
//! (mitcad#1596) only where the layer G-code that counts is the file's
//! (mitcad#2211): a printer that names a system preset of the export's
//! own slicer keeps its layer G-code and relative E to that preset; a
//! borrowed preset name, no name, or a printer block that lists its layer
//! G-code as differing still gets the reset. The M600
//! `change_filament_gcode` of manual filament changes follows the same
//! rule (mitcad#2235).

use serde_json::{Map, Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;

use crate::print_files_tests::{command, design_of, export_to, project, small_block};

/// A block of PLA on a printer named only by its system presets
/// (`presets`, `printer_settings_id` per slicer), its model, nozzle, bed
/// and height.
fn design_on_preset(presets: Value) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    small_block(&mut doc);
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
             "values": {"printer_settings_id": presets,
                        "printer_model": {"bambu_studio": "Bench", "orcaslicer": "Bench"},
                        "nozzle_diameter": 0.4,
                        "bed_shape": [[0, 0], [180, 0], [180, 180], [0, 180]],
                        "max_print_height": 180}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
             "value": "bench"},
            {"cmd": "set_component_material", "component": "Root", "material": "pla"},
        ]),
    );
    doc
}

/// A Bambu-family project's `project_settings.config`.
fn project_settings(data: &[u8]) -> Map<String, Value> {
    let package = mitcad_3mf::read_package(data).unwrap();
    serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap()
}

/// The keys of the printer's entry of `different_settings_to_system`
/// (the last), sorted.
fn printer_listed(settings: &Map<String, Value>) -> Vec<String> {
    let entries = settings["different_settings_to_system"].as_array().unwrap();
    let mut keys: Vec<String> = entries
        .last()
        .unwrap()
        .as_str()
        .unwrap()
        .split(';')
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect();
    keys.sort();
    keys
}

/// The reset the export adds: `G92 E0` last in the before layer change
/// G-code, relative E on, both listed as the printer's.
fn assert_reset_added(settings: &Map<String, Value>, before: &str, what: &str) {
    assert_eq!(settings["before_layer_change_gcode"], before, "{what}");
    assert_eq!(settings["use_relative_e_distances"], "1", "{what}");
    let listed = printer_listed(settings);
    for key in ["before_layer_change_gcode", "use_relative_e_distances"] {
        assert!(listed.iter().any(|k| k == key), "{what}: {listed:?}");
    }
}

#[test]
fn a_printer_of_the_slicer_s_own_preset_keeps_its_layer_gcode() {
    let mut doc = design_on_preset(json!({"bambu_studio": "Bench 0.4 nozzle",
                                          "orcaslicer": "Bench 0.4 nozzle"}));
    for dialect in ["bambu_studio", "orcaslicer"] {
        let (_, data) = export_to(&mut doc, "own.3mf", dialect);
        let settings = project_settings(&data);
        assert_eq!(
            settings["printer_settings_id"], "Bench 0.4 nozzle",
            "{dialect}"
        );
        let inherits = settings["inherits_group"].as_array().unwrap();
        assert_eq!(inherits.last().unwrap(), "Bench 0.4 nozzle", "{dialect}");
        // The preset's layer G-code and relative E count: not written.
        assert!(
            !settings.contains_key("before_layer_change_gcode"),
            "{dialect}"
        );
        assert!(!settings.contains_key("layer_change_gcode"), "{dialect}");
        assert!(
            !settings.contains_key("use_relative_e_distances"),
            "{dialect}"
        );
        // The printer's entry lists the design's keys only.
        assert_eq!(
            printer_listed(&settings),
            [
                "nozzle_diameter",
                "printable_area",
                "printable_height",
                "printer_model"
            ],
            "{dialect}"
        );
    }
}

#[test]
fn a_preset_name_borrowed_from_another_family_member_still_gets_the_reset() {
    // Only Bambu Studio's name: QIDI Studio and OrcaSlicer get it lent
    // and may lack that preset, so the layer G-code is the file's.
    let mut doc = design_on_preset(json!({"bambu_studio": "Bench 0.4 nozzle"}));
    for dialect in ["qidi_studio", "orcaslicer"] {
        let (_, data) = export_to(&mut doc, "borrowed.3mf", dialect);
        let settings = project_settings(&data);
        assert_eq!(
            settings["printer_settings_id"], "Bench 0.4 nozzle",
            "{dialect}"
        );
        assert_reset_added(&settings, "G92 E0", dialect);
    }
    // Bambu Studio's own: left to the preset.
    let (_, data) = export_to(&mut doc, "own.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert!(!settings.contains_key("before_layer_change_gcode"));
    assert!(!settings.contains_key("use_relative_e_distances"));
}

/// A Bambu Studio project of one PLA filament on a printer inheriting
/// its system preset, with a before layer change G-code without a reset;
/// `listed`: the printer's keys that differ from the preset.
fn project_on_preset(listed: &str) -> Vec<u8> {
    let settings = json!({
        "printer_settings_id": "Bench X 0.4 nozzle",
        "printer_model": "Bench X",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "printable_height": "180",
        "nozzle_diameter": ["0.4"],
        "before_layer_change_gcode": ";BEFORE_LAYER_CHANGE",
        "print_settings_id": "Standard",
        "layer_height": "0.2",
        "filament_settings_id": ["Generic PLA"],
        "filament_type": ["PLA"],
        "filament_colour": ["#FF0000"],
        "inherits_group": ["", "", "Bench X 0.4 nozzle"],
        "different_settings_to_system": ["", "", listed],
    });
    project(
        "BambuStudio-02.00.00.00",
        vec![(
            "Metadata/project_settings.config",
            settings.to_string().into_bytes(),
        )],
        Vec::new(),
    )
}

#[test]
fn a_printer_block_listing_its_layer_gcode_still_gets_the_reset() {
    // The block of an imported project names its system preset and lists
    // its own before layer change G-code: that text counts, and gets the
    // reset appended.
    let data = project_on_preset("before_layer_change_gcode");
    let mut doc = design_of("listed.3mf", &data, &[0]);
    let (_, back) = export_to(&mut doc, "listed-back.3mf", "bambu_studio");
    let settings = project_settings(&back);
    let inherits = settings["inherits_group"].as_array().unwrap();
    assert_eq!(inherits.last().unwrap(), "Bench X 0.4 nozzle");
    assert_reset_added(&settings, ";BEFORE_LAYER_CHANGE\nG92 E0", "listed");

    // Not listed: the preset's own layer G-code counts, the kept text is
    // written back as it was and nothing is added.
    let data = project_on_preset("printable_height");
    let mut doc = design_of("unlisted.3mf", &data, &[0]);
    let (_, back) = export_to(&mut doc, "unlisted-back.3mf", "bambu_studio");
    let settings = project_settings(&back);
    assert_eq!(
        settings["before_layer_change_gcode"],
        ";BEFORE_LAYER_CHANGE"
    );
    assert!(!settings.contains_key("use_relative_e_distances"));
    let listed = printer_listed(&settings);
    assert!(
        !listed
            .iter()
            .any(|k| k == "before_layer_change_gcode" || k == "use_relative_e_distances"),
        "{listed:?}"
    );
}

/// The filament change G-code the export writes for manual changes
/// (mitcad#1596).
const M600: &str =
    "{if previous_extruder > -1}{if previous_extruder != next_extruder}M600{endif}{endif}";

/// The warning that goes with it.
const M600_WARNING: &str = "written with M600 as its change_filament_gcode";

/// `design_on_preset` in a red PLA with a change to blue at 2.5 mm, a
/// manual filament change on a plate of one filament.
fn design_with_change(presets: Value) -> Document<MockKernel> {
    let mut doc = design_on_preset(presets);
    command(
        &mut doc,
        json!([
            {"cmd": "create_appearance", "id": "red_pla", "name": "Red", "base_color": [1.0, 0.0, 0.0]},
            {"cmd": "create_appearance", "id": "blue_pla", "name": "Blue", "base_color": [0.0, 0.0, 1.0]},
            {"cmd": "create_material", "id": "pla_two", "name": "PLA Two", "density": 1.24,
             "class": "plastic", "appearances": ["red_pla", "blue_pla"],
             "filament": {"filament_type": "PLA", "temperature": 215}},
            {"cmd": "set_component_material", "component": "Root", "material": "pla_two"},
            {"cmd": "set_filament_changes", "changes": [
                {"height": 2.5, "material": "pla_two", "appearance": "blue_pla"}]},
        ]),
    );
    doc
}

/// The export's warnings.
fn warnings(result: &Value) -> Vec<String> {
    result["warnings"]
        .as_array()
        .map(|w| w.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

#[test]
fn a_printer_of_the_slicer_s_own_preset_keeps_its_filament_change_gcode() {
    // mitcad#2235: the preset's own change G-code counts (for a printer of
    // several extruders or a filament changer, its tool change sequence).
    let mut doc = design_with_change(json!({"bambu_studio": "Bench 0.4 nozzle",
                                            "orcaslicer": "Bench 0.4 nozzle"}));
    for dialect in ["bambu_studio", "orcaslicer"] {
        let (result, data) = export_to(&mut doc, "own-change.3mf", dialect);
        let settings = project_settings(&data);
        // The change is a manual one: two filaments, no prime tower.
        assert_eq!(
            settings["filament_colour"],
            json!(["#FF0000", "#0000FF"]),
            "{dialect}"
        );
        assert_eq!(settings["enable_prime_tower"], "0", "{dialect}");
        // The preset's change G-code: not written, not listed, no warning.
        assert!(!settings.contains_key("change_filament_gcode"), "{dialect}");
        assert_eq!(
            printer_listed(&settings),
            [
                "nozzle_diameter",
                "printable_area",
                "printable_height",
                "printer_model"
            ],
            "{dialect}"
        );
        assert!(
            warnings(&result).iter().all(|w| !w.contains(M600_WARNING)),
            "{result}"
        );
    }
}

#[test]
fn a_preset_name_borrowed_from_another_family_member_still_gets_the_m600() {
    // Only Bambu Studio's name: QIDI Studio and OrcaSlicer get it lent and
    // may lack that preset, so the change G-code is the file's.
    let mut doc = design_with_change(json!({"bambu_studio": "Bench 0.4 nozzle"}));
    for dialect in ["qidi_studio", "orcaslicer"] {
        let (result, data) = export_to(&mut doc, "borrowed-change.3mf", dialect);
        let settings = project_settings(&data);
        assert_eq!(
            settings["printer_settings_id"], "Bench 0.4 nozzle",
            "{dialect}"
        );
        assert_eq!(settings["change_filament_gcode"], M600, "{dialect}");
        let listed = printer_listed(&settings);
        assert!(
            listed.iter().any(|k| k == "change_filament_gcode"),
            "{dialect}: {listed:?}"
        );
        assert!(
            warnings(&result).iter().any(|w| w.contains(M600_WARNING)),
            "{result}"
        );
    }
    // Bambu Studio's own: left to the preset.
    let (result, data) = export_to(&mut doc, "own-change.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert!(!settings.contains_key("change_filament_gcode"));
    assert!(
        warnings(&result).iter().all(|w| !w.contains(M600_WARNING)),
        "{result}"
    );
}
