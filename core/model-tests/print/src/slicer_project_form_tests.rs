// SPDX-License-Identifier: MIT
//! A Bambu-family project as its loader reads it (mitcad#2008), with a
//! synthetic slicer data folder the tests write (never a real slicer's
//! presets): a preset's metadata in the project's own form (a filament
//! preset's `filament_id` as the project's `filament_ids`, none of a
//! preset's bookkeeping, every value a text or a list of texts), templates
//! a preset includes resolved and those not found reported, and a printer
//! of two extruders with nozzle variants whose lists keep their shape, with
//! two materials whose values per nozzle variant give one entry per slot.
//! The checks are of the file the receiver gets; what the slicer then makes
//! of it is a manual check of the receiver.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::print_files_tests::{command, export_to, small_block};
use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::print_files::read_file;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value.to_string()).unwrap();
}

/// A preset's bookkeeping, which the family's loader reads as one text of a
/// preset and a project never holds.
const PRESET_ONLY: &[&str] = &[
    "name",
    "version",
    "from",
    "type",
    "instantiation",
    "setting_id",
    "base_id",
    "user_id",
    "updated_time",
    "is_custom_defined",
    "url",
    "description",
    "renamed_from",
    "inherits",
    "include",
    "filament_id",
];

/// A synthetic Bambu Studio data folder: a vendor Acme with a printer of
/// two extruders (two nozzle variants each) that includes its machine
/// G-code templates, and two filaments with values per nozzle variant that
/// include a template of their own. `filament_template`: whether the
/// filaments' template is there.
fn slicer_folder(name: &str, filament_template: bool) -> PathBuf {
    let dir = mitcad_model::testing::unique_temp_path(&format!("project-form-{name}"));
    let slicer = dir.join("BambuStudio");
    let machine = slicer.join("system/Acme/machine");
    let filament = slicer.join("system/Acme/filament");
    write(
        &machine.join("acme_common.json"),
        &json!({"type": "machine", "name": "acme_common", "instantiation": "false",
                "nozzle_diameter": ["0.4"], "printable_height": "250",
                "machine_start_gcode": "G28 ; generic start",
                "machine_end_gcode": "M84 ; generic end"}),
    );
    write(
        &machine.join("Acme Duo template start.json"),
        &json!({"name": "Acme Duo template start", "instantiation": "false",
                "machine_start_gcode": ";===== machine: Acme Duo\nG28"}),
    );
    write(
        &machine.join("Acme Duo template end.json"),
        &json!({"name": "Acme Duo template end", "instantiation": "false",
                "machine_end_gcode": ";===== Acme Duo end\nM84"}),
    );
    write(
        &machine.join("Acme Duo 0.4 nozzle.json"),
        &json!({"type": "machine", "name": "Acme Duo 0.4 nozzle", "inherits": "acme_common",
                "setting_id": "AM01", "from": "system", "instantiation": "true",
                "include": ["Acme Duo template start", "Acme Duo template end"],
                "printable_area": ["0x0", "300x0", "300x250", "0x250"],
                "nozzle_diameter": ["0.4", "0.6"],
                "extruder_type": ["Direct Drive", "Direct Drive"],
                "default_nozzle_volume_type": ["Standard", "Standard"],
                "extruder_printable_area": ["0x0,280x0,280x250,0x250",
                                            "20x0,300x0,300x250,20x250"],
                "printer_extruder_id": ["1", "1", "2", "2"],
                "printer_extruder_variant": ["Direct Drive Standard", "Direct Drive High Flow",
                                             "Direct Drive Standard", "Direct Drive High Flow"],
                "nozzle_volume": ["100", "110", "120", "130"],
                "retraction_length": ["0.8", "1.0", "0.8", "1.0"]}),
    );
    if filament_template {
        write(
            &filament.join("acme_filament_template.json"),
            &json!({"name": "acme_filament_template", "instantiation": "false",
                    "type": "filament", "filament_acme_template_value": ["t1"]}),
        );
    }
    write(
        &filament.join("Acme PLA.json"),
        &json!({"type": "filament", "name": "Acme PLA", "from": "system",
                "filament_id": "GFX01", "setting_id": "GFSX01",
                "description": "A synthetic filament", "instantiation": "true",
                "include": ["acme_filament_template"],
                "compatible_printers": ["Acme Duo 0.4 nozzle"],
                "filament_type": ["PLA"],
                "filament_extruder_variant": ["Direct Drive Standard", "Direct Drive High Flow"],
                "filament_acme_variant_value": ["0", "5"],
                "filament_acme_note": ["only the PLA keeps it"]}),
    );
    write(
        &filament.join("Acme PETG.json"),
        &json!({"type": "filament", "name": "Acme PETG", "from": "system",
                "filament_id": "GFX02", "setting_id": "GFSX02", "instantiation": "true",
                "include": ["acme_filament_template"],
                "filament_type": ["PETG"],
                "filament_extruder_variant": ["Direct Drive High Flow", "Direct Drive Standard"],
                "filament_acme_variant_value": ["7", "9"]}),
    );
    slicer
}

/// The design: the folder's printer and filaments imported, the printer
/// chosen, a body of each filament in a component of its own; and the
/// import's answer.
fn design_of_presets(slicer: &Path) -> (Document<MockKernel>, Value) {
    let presets: Vec<Value> = [
        "machine/Acme Duo 0.4 nozzle.json",
        "filament/Acme PLA.json",
        "filament/Acme PETG.json",
    ]
    .iter()
    .map(|p| json!({"path": slicer.join("system/Acme").join(p), "same_name": "copy"}))
    .collect();
    let mut doc = Document::new(MockKernel::default());
    let imported = command(
        &mut doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "into": "design", "presets": presets}),
    );
    let printer = imported["printers"][0]["id"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
               "value": printer}),
    );
    let materials: Vec<String> = imported["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(materials.len(), 2, "{imported}");
    bodies_of(&mut doc, &materials);
    (doc, imported)
}

/// A body of each material, each in a component of its own.
fn bodies_of(doc: &mut Document<MockKernel>, materials: &[String]) {
    for (i, material) in materials.iter().enumerate() {
        let name = format!("Slot {}", i + 1);
        doc.create_component(
            Some(name.as_str()),
            Transform::translation([20.0 * i as f64, 0.0, 0.0]),
            true,
        )
        .unwrap();
        small_block(doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        command(
            doc,
            json!({"cmd": "set_component_material", "component": name, "material": material}),
        );
    }
}

/// The project settings of an exported 3MF.
fn project_settings(data: &[u8]) -> Map<String, Value> {
    let package = mitcad_3mf::read_package(data).unwrap();
    serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap()
}

fn warnings_of(result: &Value) -> Vec<String> {
    result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect()
}

/// Every value of a project is a text or a list of texts, and no preset's
/// bookkeeping is among its keys: what the family's loader reads.
fn assert_loader_types(settings: &Map<String, Value>) {
    for key in PRESET_ONLY {
        assert!(!settings.contains_key(*key), "{key}: {:?}", settings[*key]);
    }
    for (key, value) in settings {
        let ok = match value {
            Value::String(_) => true,
            Value::Array(items) => items.iter().all(Value::is_string),
            _ => false,
        };
        assert!(ok, "{key}: {value}");
    }
}

#[test]
fn installed_presets_reach_the_project_in_its_own_form() {
    let slicer = slicer_folder("whole", true);
    let (mut doc, imported) = design_of_presets(&slicer);
    assert_eq!(imported["report"]["problems"], json!([]), "{imported}");
    let left_out = imported["report"]["left_out"].to_string();
    assert!(!left_out.contains("template"), "{left_out}");

    // Twice: an export changes nothing it writes the next time.
    for file in ["bambu.3mf", "bambu-again.3mf"] {
        let (result, data) = export_to(&mut doc, file, "bambu_studio");
        let settings = project_settings(&data);
        assert_loader_types(&settings);
        // The filaments' ids as the project's list, an entry each.
        assert_eq!(
            settings["filament_ids"],
            json!(["GFX01", "GFX02"]),
            "{file}"
        );
        // The printers a filament preset is for are not a slot's setting.
        assert!(!settings.contains_key("compatible_printers"), "{file}");
        // The printer's templates, not its base's.
        assert_eq!(
            settings["machine_start_gcode"],
            ";===== machine: Acme Duo\nG28"
        );
        assert_eq!(settings["machine_end_gcode"], ";===== Acme Duo end\nM84");
        // The filaments' template, for both slots.
        assert_eq!(
            settings["filament_acme_template_value"],
            json!(["t1", "t1"])
        );
        // Two extruders, two nozzle variants each, as the printer has them.
        assert_eq!(settings["nozzle_diameter"], json!(["0.4", "0.6"]));
        assert_eq!(
            settings["extruder_type"],
            json!(["Direct Drive", "Direct Drive"])
        );
        assert_eq!(
            settings["nozzle_volume_type"],
            json!(["Standard", "Standard"])
        );
        assert_eq!(settings["printer_extruder_id"], json!(["1", "1", "2", "2"]));
        assert_eq!(
            settings["nozzle_volume"],
            json!(["100", "110", "120", "130"])
        );
        assert_eq!(
            settings["retraction_length"],
            json!(["0.8", "1.0", "0.8", "1.0"])
        );
        // Each slot's value of the printer's nozzle variant.
        assert_eq!(
            settings["filament_extruder_variant"],
            json!(["Direct Drive Standard", "Direct Drive Standard"])
        );
        assert_eq!(settings["filament_acme_variant_value"], json!(["0", "9"]));
        // Two materials, two slots.
        assert_eq!(settings["filament_type"], json!(["PLA", "PETG"]));
        assert_eq!(result["filaments"], json!([1, 2]));
        let warnings = warnings_of(&result);
        let said = |part: &str| warnings.iter().any(|w| w.contains(part));
        // A key only one slot's material keeps: left to the slicer, said.
        assert!(!settings.contains_key("filament_acme_note"));
        assert!(said("filament_acme_note"), "{warnings:?}");
        // The values per nozzle variant reduced to one per slot: said.
        assert!(said("filament_acme_variant_value"), "{warnings:?}");
        assert!(said("Direct Drive Standard"), "{warnings:?}");
        // The printer's structure is whole: no warning of it.
        assert!(!said("different lengths"), "{warnings:?}");
        assert!(!said("does not have"), "{warnings:?}");
        assert!(!said("bookkeeping"), "{warnings:?}");
    }

    // Read back: the printer keeps its lists, the filaments their ids.
    let (_, data) = export_to(&mut doc, "back.3mf", "bambu_studio");
    let back = read_file("back.3mf", &data, None).unwrap();
    let raw = &back.printers[0].raw["bambu_studio"];
    assert_eq!(raw["nozzle_diameter"], json!(["0.4", "0.6"]));
    assert_eq!(raw["printer_extruder_id"], json!(["1", "1", "2", "2"]));
    let ids: Vec<&str> = back
        .materials
        .iter()
        .map(|(m, _)| m.slicer["bambu_studio"]["filament_ids"].as_str())
        .collect();
    assert_eq!(ids, ["GFX01", "GFX02"]);

    // A nozzle diameter changed in MitCad is written for every extruder,
    // and said.
    let printer = imported["printers"][0]["id"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": printer,
               "values": {"nozzle_diameter": 0.6}}),
    );
    let (result, data) = export_to(&mut doc, "changed.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert_eq!(settings["nozzle_diameter"], json!(["0.6", "0.6"]));
    assert!(
        warnings_of(&result)
            .iter()
            .any(|w| w.contains("nozzle_diameter") && w.contains("every one")),
        "{result}"
    );
}

/// mitcad#2008: templates a preset includes that are not there are
/// reported: the import is not complete, and says so.
#[test]
fn templates_not_found_are_reported() {
    let slicer = slicer_folder("missing", false);
    let (mut doc, imported) = design_of_presets(&slicer);
    let left_out: Vec<&str> = imported["report"]["left_out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert!(
        left_out
            .iter()
            .any(|l| l.contains("templates acme_filament_template")),
        "{left_out:?}"
    );
    // The project names no template.
    let (_, data) = export_to(&mut doc, "missing.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert_loader_types(&settings);
    assert!(!settings.contains_key("filament_acme_template_value"));
}

/// mitcad#2008: a design imported before keeps a preset's `filament_id`,
/// its bookkeeping and the printer's `include`, and its printer one nozzle
/// diameter beside lists of two extruders: the project gets the ids as
/// `filament_ids`, none of the bookkeeping, a diameter for each extruder,
/// and warnings that say so; lists of the printer that disagree are
/// warnings, never a silent fix.
#[test]
fn a_design_imported_before_writes_a_project_the_loader_reads() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "id": "duo", "name": "Duo",
             "choose": true,
             "values": {"nozzle_diameter": 0.4,
                        "printer_settings_id": {"bambu_studio": "Acme Duo 0.4 nozzle"}},
             "slicer": {"bambu_studio": {
                 "extruder_type": "[\"Direct Drive\",\"Direct Drive\"]",
                 "include": "[\"Acme Duo template start\"]",
                 "printer_extruder_id": "[\"1\",\"3\"]",
                 "printer_extruder_variant": "[\"Direct Drive Standard\"]"}}},
            {"cmd": "create_material", "id": "old-pla", "name": "Old PLA",
             "filament": {"filament_type": "PLA"},
             "slicer": {"bambu_studio": {"filament_id": "GFA00", "setting_id": "GFSA00",
                                         "description": "kept before"}}},
            {"cmd": "create_material", "id": "old-petg", "name": "Old PETG",
             "filament": {"filament_type": "PETG"},
             "slicer": {"bambu_studio": {"filament_id": "GFG00"}}},
        ]),
    );
    bodies_of(&mut doc, &["old-pla".to_owned(), "old-petg".to_owned()]);
    let (result, data) = export_to(&mut doc, "before.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert_loader_types(&settings);
    assert_eq!(settings["filament_ids"], json!(["GFA00", "GFG00"]));
    assert_eq!(settings["nozzle_diameter"], json!(["0.4", "0.4"]));
    let warnings = warnings_of(&result);
    let said = |parts: &[&str]| {
        warnings
            .iter()
            .any(|w| parts.iter().all(|part| w.contains(part)))
    };
    assert!(said(&["nozzle_diameter", "2 extruders"]), "{warnings:?}");
    assert!(
        said(&["bookkeeping", "include", "setting_id", "description"]),
        "{warnings:?}"
    );
    assert!(
        said(&["per nozzle variant", "different lengths"]),
        "{warnings:?}"
    );
    assert!(
        said(&["printer_extruder_id", "does not have"]),
        "{warnings:?}"
    );
}
