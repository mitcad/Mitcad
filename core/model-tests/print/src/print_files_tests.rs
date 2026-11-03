// SPDX-License-Identifier: MIT
//! Slicers' settings files (mitcad#640, part 5) with synthetic files the
//! tests write: each dialect's project read into profiles and materials
//! with its report, preset files and bundles, and a design's settings
//! written for each dialect (shared values, per-part values, filament
//! slots) and read back.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::internal::print_files::per_filament_keys;
use mitcad_model::print_files::*;
use mitcad_model::print_keys::{self, Dialect, KeyDef};
use mitcad_model::print_profile::PrintProfile;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

/// A closed cube for a 3MF part.
fn cube(name: &str) -> mitcad_3mf::Part {
    let vertices = (0..8)
        .map(|i| {
            [
                f64::from(i & 1),
                f64::from((i >> 1) & 1),
                f64::from((i >> 2) & 1),
            ]
        })
        .collect();
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let triangles = quads
        .iter()
        .flat_map(|q: &[u32; 4]| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    mitcad_3mf::Part {
        name: name.to_owned(),
        color: None,
        mesh: mitcad_3mf::Mesh {
            vertices,
            triangles,
        },
        placements: vec![mitcad_3mf::Placement::IDENTITY],
        metadata: Vec::new(),
    }
}

/// A 3MF project of one cube with a slicer's files.
pub(super) fn project(
    application: &str,
    files: Vec<(&str, Vec<u8>)>,
    object_metadata: Vec<(&str, &str)>,
) -> Vec<u8> {
    let mut part = cube("Cube");
    part.metadata = object_metadata
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    mitcad_3mf::write(&mitcad_3mf::Model {
        name: "Plate".to_owned(),
        parts: vec![part],
        application: application.to_owned(),
        namespaces: vec![(
            "cura".to_owned(),
            "http://software.ultimaker.com/xml/cura/3mf/2015/10".to_owned(),
        )],
        files: files.into_iter().map(|(p, d)| (p.to_owned(), d)).collect(),
        ..mitcad_3mf::Model::default()
    })
    .unwrap()
}

fn values(profile: &PrintProfile) -> &BTreeMap<String, Value> {
    &profile.values
}

#[test]
fn bambu_family_projects_import_with_their_report() {
    let settings = json!({
        "printer_settings_id": "Bench X 0.4 nozzle",
        "printer_model": "Bench X",
        "printable_area": ["0x0", "256x0", "256x256", "0x256"],
        "printable_height": "250",
        "nozzle_diameter": ["0.4"],
        "machine_start_gcode": "G28",
        "print_settings_id": "0.20mm Standard",
        "layer_height": "0.2",
        "wall_loops": "3",
        "sparse_infill_density": "15%",
        "a_process_setting": "1",
        "filament_settings_id": ["Generic PLA", "Generic PETG"],
        "filament_type": ["PLA", "PETG"],
        "filament_colour": ["#FF0000", "#00FF00"],
        "nozzle_temperature": ["220", "240"],
        "filament_density": ["1.24", "1.27"],
        "filament_custom_tag": ["a", "b"],
        "inherits_group": ["", "", "", "Bench X 0.4 nozzle"],
        "different_settings_to_system": ["layer_height", "", "", "printable_height"],
    });
    let model_settings = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n \
        <plate><metadata key=\"plater_id\" value=\"1\"/></plate>\n \
        <object id=\"3\"><metadata key=\"name\" value=\"Cube\"/>\
        <metadata key=\"layer_height\" value=\"0.1\"/></object>\n</config>\n";
    let data = project(
        "OrcaSlicer-2.3.0",
        vec![
            (
                "Metadata/project_settings.config",
                settings.to_string().into_bytes(),
            ),
            (
                "Metadata/model_settings.config",
                model_settings.as_bytes().to_vec(),
            ),
        ],
        Vec::new(),
    );
    let imported = read_file("plate.3mf", &data, None).unwrap();
    assert_eq!(imported.dialect, Some(Dialect::OrcaSlicer));
    // The printer: Mitcad's values, its preset's name per slicer, the
    // file's whole printer block for this slicer.
    let printer = &imported.printers[0];
    assert_eq!(printer.name, "Bench X 0.4 nozzle");
    assert_eq!(printer.id, "bench-x-0-4-nozzle");
    assert_eq!(
        values(printer)["bed_shape"],
        json!([[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]])
    );
    assert_eq!(values(printer)["max_print_height"], 250.0);
    assert_eq!(values(printer)["nozzle_diameter"], 0.4);
    assert_eq!(
        values(printer)["printer_settings_id"],
        json!({"orcaslicer": "Bench X 0.4 nozzle"})
    );
    let raw = &printer.raw["orcaslicer"];
    assert_eq!(raw["machine_start_gcode"], "G28");
    assert_eq!(raw["printable_height"], "250");
    assert_eq!(raw["@inherits"], "Bench X 0.4 nozzle");
    assert_eq!(printer.slicer["orcaslicer"]["machine_start_gcode"], "G28");
    // The process.
    let process = &imported.processes[0];
    assert_eq!(process.name, "0.20mm Standard");
    assert_eq!(values(process)["layer_height"], 0.2);
    assert_eq!(values(process)["perimeters"], 3);
    assert_eq!(values(process)["fill_density"], 15.0);
    assert_eq!(process.slicer["orcaslicer"]["a_process_setting"], "1");
    // A material per filament, with its colour and its own keys.
    assert_eq!(imported.materials.len(), 2);
    let (pla, pla_look) = &imported.materials[0];
    assert_eq!(pla.name, "Generic PLA");
    assert_eq!(pla.based_on.as_deref(), Some("pla"));
    assert_eq!(pla.density, 1.24);
    assert_eq!(pla.filament["temperature"], 220.0);
    assert_eq!(pla.slicer["orcaslicer"]["filament_custom_tag"], "a");
    assert_eq!(pla_look.as_ref().unwrap().base_color, [1.0, 0.0, 0.0]);
    let (petg, _) = &imported.materials[1];
    assert_eq!(petg.density, 1.27);
    assert_eq!(petg.filament["temperature"], 240.0);
    assert_eq!(petg.slicer["orcaslicer"]["filament_custom_tag"], "b");
    for (material, _) in &imported.materials {
        material.check().unwrap();
    }
    // The report: what was mapped, kept and left out.
    let report = &imported.report;
    assert!(report.mapped.contains("wall_loops"));
    assert!(report.kept.contains("a_process_setting"));
    assert!(
        report
            .left_out
            .iter()
            .any(|l| l.contains("per-object settings of 1 objects")),
        "{:?}",
        report.left_out
    );
    assert!(report.left_out.iter().any(|l| l.contains("1 plates")));
    assert!(report.problems.is_empty(), "{:?}", report.problems);
}

#[test]
fn bambu_presets_and_bundles_import() {
    let preset = json!({
        "type": "filament",
        "name": "Acme PETG HF",
        "inherits": "Generic PETG",
        "filament_type": ["PETG"],
        "nozzle_temperature": ["245"],
        "version": "2.0.0.0",
    });
    let imported = read_file("acme.json", preset.to_string().as_bytes(), None).unwrap();
    assert!(imported.printers.is_empty() && imported.processes.is_empty());
    assert_eq!(imported.materials[0].0.name, "Acme PETG HF");
    assert_eq!(imported.materials[0].0.filament["temperature"], 245.0);
    assert!(imported.report.left_out[0].contains("Generic PETG"));
    // A bundle: a zip of presets.
    let process = json!({"type": "process", "name": "Fast", "layer_height": "0.28"});
    let mut zip = mitcad_zip::Writer::new();
    zip.add("bundle_structure.json", b"{}", mitcad_zip::Method::Deflate)
        .unwrap();
    zip.add(
        "filament/acme.json",
        preset.to_string().as_bytes(),
        mitcad_zip::Method::Deflate,
    )
    .unwrap();
    zip.add(
        "process/fast.json",
        process.to_string().as_bytes(),
        mitcad_zip::Method::Deflate,
    )
    .unwrap();
    let bundle = zip.finish().unwrap();
    let imported = read_file("presets.bbscfg", &bundle, Some(Dialect::QidiStudio)).unwrap();
    assert_eq!(imported.dialect, Some(Dialect::QidiStudio));
    assert_eq!(imported.processes[0].name, "Fast");
    assert_eq!(values(&imported.processes[0])["layer_height"], 0.28);
    assert_eq!(imported.materials.len(), 1);
}

#[test]
fn prusaslicer_projects_configs_and_bundles_import() {
    let config = "; generated by PrusaSlicer 2.9.0\n\n\
        ; bed_shape = 0x0,250x0,250x210,0x210\n\
        ; max_print_height = 210\n\
        ; nozzle_diameter = 0.4,0.4\n\
        ; printer_settings_id = Original Prusa MK4\n\
        ; print_settings_id = 0.15mm QUALITY\n\
        ; layer_height = 0.15\n\
        ; perimeters = 2\n\
        ; fill_density = 20%\n\
        ; filament_settings_id = \"Prusament PLA\";\"Prusament PETG\"\n\
        ; filament_type = PLA;PETG\n\
        ; filament_colour = #FF8000;#0000FF\n\
        ; temperature = 215,240\n\
        ; gcode_comments = 0\n";
    let model_settings = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n \
        <object id=\"2\" instances_count=\"1\">\
        <metadata type=\"object\" key=\"perimeters\" value=\"4\"/></object>\n</config>\n";
    let data = project(
        "PrusaSlicer-2.9.0",
        vec![
            ("Metadata/Slic3r_PE.config", config.as_bytes().to_vec()),
            (
                "Metadata/Slic3r_PE_model.config",
                model_settings.as_bytes().to_vec(),
            ),
        ],
        Vec::new(),
    );
    let imported = read_file("mk4.3mf", &data, None).unwrap();
    assert_eq!(imported.dialect, Some(Dialect::PrusaSlicer));
    let printer = &imported.printers[0];
    assert_eq!(printer.name, "Original Prusa MK4");
    assert_eq!(values(printer)["filament_slots"], 2);
    assert_eq!(values(printer)["max_print_height"], 210.0);
    let process = &imported.processes[0];
    assert_eq!(values(process)["perimeters"], 2);
    assert_eq!(values(process)["fill_density"], 20.0);
    assert_eq!(process.slicer["prusaslicer"]["gcode_comments"], "0");
    let names: Vec<&str> = imported
        .materials
        .iter()
        .map(|(m, _)| m.name.as_str())
        .collect();
    assert_eq!(names, ["Prusament PLA", "Prusament PETG"]);
    assert_eq!(imported.materials[1].0.filament["filament_type"], "PETG");
    assert_eq!(imported.materials[1].0.filament["temperature"], 240.0);
    assert!(
        imported
            .report
            .left_out
            .iter()
            .any(|l| l.contains("per-object settings of 1 objects"))
    );

    // A config bundle, with inheritance inside it.
    let bundle = "[print:*common*]\nperimeters = 3\nlayer_height = 0.2\n\n\
        [print:0.20mm SPEED]\ninherits = *common*\nlayer_height = 0.2\nfill_density = 10%\n\n\
        [filament:Acme PLA]\nfilament_type = PLA\ntemperature = 210\n\n\
        [printer:Bench MK]\nbed_shape = 0x0,200x0,200x200,0x200\nmax_print_height = 180\n\n\
        [presets]\nprint = 0.20mm SPEED\n";
    let imported = read_file("bundle.ini", bundle.as_bytes(), None).unwrap();
    assert_eq!(imported.processes.len(), 1);
    assert_eq!(imported.processes[0].name, "0.20mm SPEED");
    assert_eq!(values(&imported.processes[0])["perimeters"], 3);
    assert_eq!(values(&imported.processes[0])["fill_density"], 10.0);
    assert_eq!(imported.materials[0].0.name, "Acme PLA");
    assert_eq!(values(&imported.printers[0])["max_print_height"], 180.0);
    assert!(imported.report.left_out[0].contains("only others inherit"));
}

#[test]
fn cura_projects_import_their_stacks() {
    let global = "[general]\nversion = 4\nname = My Ender\nid = My Ender\n\n\
        [metadata]\ntype = machine\n\n\
        [containers]\n0 = My Ender_user\n1 = empty_quality_changes\n2 = My Ender_settings\n\
        3 = creality_ender3\n";
    let user = "[general]\nversion = 4\nname = My Ender_user\n\n[metadata]\ntype = user\n\n\
        [values]\nlayer_height = 0.16\nwall_line_count = 3\ninfill_sparse_density = =10 * 2\n";
    let machine = "[general]\nversion = 4\nname = My Ender_settings\n\n\
        [metadata]\ntype = definition_changes\n\n\
        [values]\nmachine_width = 220\nmachine_depth = 220\nmachine_height = 250\n\
        machine_center_is_zero = False\nmachine_start_gcode = G28\n    G1 Z5\n";
    let extruder = "[general]\nversion = 4\nname = Extruder 1\nid = My Ender_extruder_0\n\n\
        [metadata]\ntype = extruder_train\nposition = 0\n\n\
        [containers]\n0 = My Ender_extruder_0_user\n1 = generic_pla_175\n";
    let extruder_user = "[general]\nversion = 4\nname = x\n\n[metadata]\ntype = user\n\n\
        [values]\nmaterial_print_temperature = 205\n";
    let material = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <fdmmaterial xmlns=\"http://www.ultimaker.com/material\"><metadata><name>\
        <brand>Generic</brand><material>PLA</material><color>Yellow</color></name>\
        <color_code>#ffc924</color_code></metadata><properties><density>1.24</density>\
        </properties></fdmmaterial>\n";
    let data = project(
        "Cura 5.9.0",
        vec![
            ("Cura/My Ender.global.cfg", global.as_bytes().to_vec()),
            ("Cura/My Ender_user.inst.cfg", user.as_bytes().to_vec()),
            (
                "Cura/My Ender_settings.inst.cfg",
                machine.as_bytes().to_vec(),
            ),
            (
                "Cura/My Ender_extruder_0.extruder.cfg",
                extruder.as_bytes().to_vec(),
            ),
            (
                "Cura/My Ender_extruder_0_user.inst.cfg",
                extruder_user.as_bytes().to_vec(),
            ),
            (
                "Cura/generic_pla_175.xml.fdm_material",
                material.as_bytes().to_vec(),
            ),
        ],
        vec![("cura:infill_sparse_density", "30")],
    );
    let imported = read_file("ender.3mf", &data, None).unwrap();
    assert_eq!(imported.dialect, Some(Dialect::Cura));
    let printer = &imported.printers[0];
    assert_eq!(printer.name, "My Ender");
    assert_eq!(
        values(printer)["bed_shape"],
        json!([[0.0, 0.0], [220.0, 0.0], [220.0, 220.0], [0.0, 220.0]])
    );
    assert_eq!(values(printer)["max_print_height"], 250.0);
    assert_eq!(
        values(printer)["printer_settings_id"],
        json!({"cura": "creality_ender3"})
    );
    assert_eq!(printer.slicer["cura"]["machine_start_gcode"], "G28\nG1 Z5");
    let process = &imported.processes[0];
    assert_eq!(values(process)["layer_height"], 0.16);
    assert_eq!(values(process)["perimeters"], 3);
    assert!(!values(process).contains_key("fill_density"));
    let (pla, look) = &imported.materials[0];
    assert_eq!(pla.filament["temperature"], 205.0);
    assert_eq!(pla.filament["filament_type"], "PLA");
    assert_eq!(pla.density, 1.24);
    assert!(look.is_some());
    let left_out = &imported.report.left_out;
    assert!(
        left_out.iter().any(|l| l.contains("formulas")),
        "{left_out:?}"
    );
    assert!(
        left_out
            .iter()
            .any(|l| l.contains("per-model settings of 1"))
    );
}

pub(super) fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// A 10 x 10 x 5 block in the active component.
pub(super) fn small_block(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// Two bodies: one of the root (PLA), one of an arm (ABS) with five walls
/// of its own; a bench printer and a standard process at the root.
pub(super) fn design() -> Document<MockKernel> {
    design_and_body().0
}

/// [`design`] and its root body (PLA, no print values of its own).
fn design_and_body() -> (Document<MockKernel>, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let body = small_block(&mut doc);
    let (arm, _) = doc
        .create_component(Some("Arm"), Transform::translation([40.0, 0.0, 0.0]), true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
             "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]],
                        "max_print_height": 250, "nozzle_diameter": 0.4, "filament_slots": 4,
                        "printer_settings_id": {"bambu_studio": "Bench 0.4 nozzle",
                                                "prusaslicer": "Bench MK"}}},
            {"cmd": "create_print_profile", "kind": "process", "name": "Standard",
             "values": {"layer_height": 0.2, "perimeters": 2, "fill_density": 15}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
             "value": "bench"},
            {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile",
             "value": "standard"},
            {"cmd": "set_property", "owner": arm.to_string(), "set": "Print Process",
             "key": "perimeters", "value": 5},
            {"cmd": "set_component_material", "component": "Root", "material": "pla"},
            {"cmd": "set_component_material", "component": "Arm", "material": "abs"},
        ]),
    );
    (doc, body)
}

pub(super) fn export_to(
    doc: &mut Document<MockKernel>,
    name: &str,
    dialect: &str,
) -> (Value, Vec<u8>) {
    let dir = mitcad_model::testing::unique_temp_dir("print-export");
    let path = dir.join(name).to_str().unwrap().to_owned();
    let result = command(
        doc,
        json!({"cmd": "export", "path": path, "dialect": dialect}),
    );
    (result, std::fs::read(&path).unwrap())
}

#[test]
fn bambu_family_exports_carry_the_loader_s_files() {
    let mut doc = design();
    let (result, data) = export_to(&mut doc, "bambu.3mf", "bambu_studio");
    assert_eq!(result["dialect"], "bambu_studio");
    assert_eq!(result["filaments"], json!([1, 2]));
    // The geometry is a core 3MF still.
    assert_eq!(mitcad_3mf::read(&data).unwrap().parts.len(), 2);
    let package = mitcad_3mf::read_package(&data).unwrap();
    assert_eq!(
        package.metadata("Application"),
        Some(format!("BambuStudio-{}", bambu_version()).as_str())
    );
    assert_eq!(package.metadata("BambuStudio:3mfVersion"), Some("1"));
    let settings: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(settings["printer_settings_id"], "Bench 0.4 nozzle");
    assert_eq!(settings["print_settings_id"], "Standard");
    assert_eq!(
        settings["printable_area"],
        json!(["0x0", "256x0", "256x256", "0x256"])
    );
    assert_eq!(settings["printable_height"], "250");
    assert_eq!(settings["nozzle_diameter"], json!(["0.4"]));
    assert_eq!(settings["layer_height"], "0.2");
    assert_eq!(settings["sparse_infill_density"], "15%");
    // The walls differ: they are each part's.
    assert!(!settings.contains_key("wall_loops"));
    // A filament per material, in the order of first use.
    assert_eq!(settings["filament_type"], json!(["PLA", "ABS"]));
    assert_eq!(settings["filament_colour"].as_array().unwrap().len(), 2);
    assert_eq!(settings["filament_density"], json!(["1.24", "1.06"]));
    assert_eq!(settings["inherits_group"].as_array().unwrap().len(), 4);
    assert_eq!(settings["inherits_group"][3], "Bench 0.4 nozzle");
    assert_eq!(
        settings["different_settings_to_system"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(model.contains("<object id=\"4\">"), "{model}");
    assert!(model.contains("<part id=\"2\" subtype=\"normal_part\">"));
    assert!(model.contains("<metadata key=\"extruder\" value=\"2\"/>"));
    assert!(model.contains("<metadata key=\"wall_loops\" value=\"5\"/>"));
    assert!(model.contains("<metadata key=\"wall_loops\" value=\"2\"/>"));

    // Read back: the same printer and process, two materials.
    let imported = read_file("bambu.3mf", &data, None).unwrap();
    assert_eq!(imported.dialect, Some(Dialect::BambuStudio));
    assert_eq!(values(&imported.printers[0])["max_print_height"], 250.0);
    assert_eq!(values(&imported.processes[0])["layer_height"], 0.2);
    assert_eq!(imported.materials.len(), 2);

    // QIDI Studio's application.
    let (_, qidi) = export_to(&mut doc, "qidi.3mf", "qidi_studio");
    let package = mitcad_3mf::read_package(&qidi).unwrap();
    assert!(
        package
            .metadata("Application")
            .unwrap()
            .starts_with("QIDIStudio-")
    );
    // Bambu Studio's preset name stands in for its family.
    let settings: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(settings["printer_settings_id"], "Bench 0.4 nozzle");
}

#[test]
fn prusaslicer_and_cura_exports_give_each_body_its_settings() {
    let mut doc = design();
    let (result, data) = export_to(&mut doc, "prusa.3mf", "prusaslicer");
    assert_eq!(result["filaments"], json!([1, 2]));
    let package = mitcad_3mf::read_package(&data).unwrap();
    assert_eq!(package.metadata("slic3rpe:Version3mf"), Some("1"));
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(config.contains("; layer_height = 0.2\n"), "{config}");
    assert!(config.contains("; nozzle_diameter = 0.4,0.4\n"));
    assert!(config.contains("; printer_settings_id = Bench MK\n"));
    // Strings are quoted only where an entry needs it, as PrusaSlicer
    // writes them.
    assert!(config.contains("; filament_type = PLA;ABS\n"));
    assert!(!config.contains("; perimeters ="));
    let model = std::str::from_utf8(package.file("Metadata/Slic3r_PE_model.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        model.contains("<object id=\"3\" instances_count=\"1\">"),
        "{model}"
    );
    assert!(model.contains("key=\"extruder\" value=\"2\""));
    assert!(model.contains("key=\"perimeters\" value=\"5\""));
    assert!(model.contains("<volume firstid=\"0\" lastid=\"11\">"));
    let imported = read_file("prusa.3mf", &data, None).unwrap();
    assert_eq!(values(&imported.printers[0])["filament_slots"], 2);
    // Two filaments with the wipe tower on: PrusaSlicer makes it only with
    // relative extruder addressing (mitcad#640).
    assert!(!config.contains("use_relative_e_distances"));
    command(
        &mut doc,
        json!([{"cmd": "set_property", "owner": "C0", "set": "Print Process",
                "key": "wipe_tower", "value": true}]),
    );
    let (_, data) = export_to(&mut doc, "prusa-tower.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(config.contains("; wipe_tower = 1\n"), "{config}");
    assert!(config.contains("; use_relative_e_distances = 1\n"));

    let (_, cura) = export_to(&mut doc, "cura.3mf", "cura");
    let package = mitcad_3mf::read_package(&cura).unwrap();
    let parts: Vec<&mitcad_3mf::PackageObject> = package
        .objects
        .iter()
        .filter(|o| !o.metadata.is_empty())
        .collect();
    assert_eq!(parts.len(), 2);
    let setting = |object: &mitcad_3mf::PackageObject, key: &str| {
        object
            .metadata
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(setting(parts[0], "cura:extruder_nr").as_deref(), Some("0"));
    assert_eq!(setting(parts[1], "cura:extruder_nr").as_deref(), Some("1"));
    assert_eq!(
        setting(parts[0], "cura:wall_line_count").as_deref(),
        Some("2")
    );
    assert_eq!(
        setting(parts[1], "cura:wall_line_count").as_deref(),
        Some("5")
    );
    assert_eq!(
        setting(parts[0], "cura:layer_height").as_deref(),
        Some("0.2")
    );
}

fn read(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A print value added to a body as its own (the Make tab's Add Print
/// Value…, mitcad#1178): `set_property` on the body at the value it prints
/// with, then typed over, each one undo step; the 3MF exports give that
/// body its own Walls and the Make tab's markers name it; `remove_property`
/// clears it again in one undo step, and the export goes back to the
/// process's Walls.
#[test]
fn a_print_value_added_to_a_body_reaches_the_3mf_export() {
    let (mut doc, body) = design_and_body();
    let owner = body.to_string();
    // What it prints with: the process's two walls; none of its own.
    let settings = read(
        &doc,
        json!({"query": "print_settings", "body": owner.clone()}),
    );
    let walls = settings["process"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "perimeters")
        .unwrap()
        .clone();
    assert_eq!(walls["from"], "profile", "{walls}");
    assert_eq!(read(&doc, json!({"query": "make_differences"})), json!([]));

    // Added at that value, then typed over: one undo step each.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": owner.clone(), "set": "Print Process",
               "key": "perimeters", "value": walls["value"].clone()}),
    );
    assert_eq!(
        read(&doc, json!({"query": "document"}))["undo"],
        "Set perimeters"
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": owner.clone(), "set": "Print Process",
               "key": "perimeters", "value": 4}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    let settings = read(
        &doc,
        json!({"query": "print_settings", "body": owner.clone()}),
    );
    let own = settings["process"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "perimeters")
        .unwrap()
        .clone();
    assert_eq!(own["from"], "property", "{own}");
    assert_eq!(own["level"], "body", "{own}");
    assert_eq!(
        own["value"].as_f64(),
        Some(2.0),
        "one undo step back to the value added"
    );
    command(&mut doc, json!({"cmd": "redo"}));
    let differences = read(&doc, json!({"query": "make_differences"}));
    assert_eq!(differences[0]["body"], owner.as_str(), "{differences}");
    assert_eq!(differences[0]["print"], json!(["Walls"]), "{differences}");

    // The exports carry it: the body's four walls beside the arm's five.
    let (_, data) = export_to(&mut doc, "own-walls.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/Slic3r_PE_model.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(model.contains("key=\"perimeters\" value=\"4\""), "{model}");
    assert!(model.contains("key=\"perimeters\" value=\"5\""), "{model}");
    let (_, data) = export_to(&mut doc, "own-walls-bambu.3mf", "bambu_studio");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        model.contains("<metadata key=\"wall_loops\" value=\"4\"/>"),
        "{model}"
    );

    // Removed: one undo step; the process's two walls again.
    command(
        &mut doc,
        json!({"cmd": "remove_property", "owner": owner.clone(), "set": "Print Process",
               "key": "perimeters"}),
    );
    assert_eq!(
        read(&doc, json!({"query": "document"}))["undo"],
        "Remove perimeters"
    );
    assert_eq!(read(&doc, json!({"query": "make_differences"})), json!([]));
    let (_, data) = export_to(&mut doc, "no-own-walls.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/Slic3r_PE_model.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(!model.contains("key=\"perimeters\" value=\"4\""), "{model}");
    assert!(model.contains("key=\"perimeters\" value=\"2\""), "{model}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        read(&doc, json!({"query": "make_differences"}))[0]["print"],
        json!(["Walls"])
    );
}

#[test]
fn more_materials_than_slots_is_a_warning() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": "bench", "values": {"filament_slots": 1}}),
    );
    let (result, _) = export_to(&mut doc, "warning.3mf", "orcaslicer");
    assert!(
        result["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("1 filament slots")),
        "{result}"
    );
    // Only 3MF files take a slicer's settings.
    let error = doc
        .command(&json!({"cmd": "export", "path": "x.stl", "dialect": "cura"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("3MF files only"), "{error}");
}

/// mitcad#654: Bambu Studio writes 95% (bridges) for the overhangs slowed
/// for cooling. Each of four filaments' 95% is read without a problem, and
/// a design that imports the file writes it back as 95%.
#[test]
fn overhangs_slowed_for_cooling_at_95_percent_read_and_write_back() {
    let settings = json!({
        "printer_settings_id": "Bench Mini 0.4 nozzle",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "printable_height": "180",
        "nozzle_diameter": ["0.4"],
        "print_settings_id": "0.08mm Extra Fine",
        "layer_height": "0.08",
        "filament_settings_id": ["Generic PETG", "Generic PLA", "Generic PETG HF", "Generic TPU"],
        "filament_type": ["PETG", "PLA", "PETG", "TPU"],
        "filament_colour": ["#FF0000", "#00FF00", "#0000FF", "#FFFFFF"],
        "overhang_threshold_participating_cooling": ["95%", "95%", "95%", "95%"],
        "overhang_fan_threshold": ["95%", "50%", "95%", "25%"],
    });
    let data = project(
        "BambuStudio-02.00.00.00",
        vec![(
            "Metadata/project_settings.config",
            settings.to_string().into_bytes(),
        )],
        Vec::new(),
    );
    let imported = read_file("mount.3mf", &data, None).unwrap();
    assert_eq!(imported.dialect, Some(Dialect::BambuStudio));
    assert!(
        imported.report.problems.is_empty(),
        "{:?}",
        imported.report.problems
    );
    assert_eq!(imported.materials.len(), 4);
    for (material, _) in &imported.materials {
        assert_eq!(
            material.filament["overhang_threshold_participating_cooling"],
            "95"
        );
        material.check().unwrap();
    }
    assert_eq!(
        imported.materials[3].0.filament["overhang_fan_threshold"],
        "25"
    );

    // Imported into a design whose root uses the first material: the
    // export writes 95% again.
    let dir = mitcad_model::testing::unique_temp_dir("print-import");
    let path = dir.join("mount.3mf").to_str().unwrap().to_owned();
    std::fs::write(&path, &data).unwrap();
    let mut doc = Document::new(MockKernel::default());
    small_block(&mut doc);
    let result = command(
        &mut doc,
        json!({"cmd": "import_print_profile", "path": path}),
    );
    assert_eq!(result["report"]["problems"], json!([]), "{result}");
    assert_eq!(result["materials"].as_array().unwrap().len(), 4);
    let material = result["materials"][0]["id"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": material}),
    );
    let (_, exported) = export_to(&mut doc, "mount-back.3mf", "bambu_studio");
    let package = mitcad_3mf::read_package(&exported).unwrap();
    let written: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(
        written["overhang_threshold_participating_cooling"],
        json!(["95%"])
    );
    assert_eq!(written["overhang_fan_threshold"], json!(["95%"]));
}

/// mitcad#640: a retraction length only the third filament overrides
/// (`nil` for the others, the printer's value there) is written back with
/// `nil` in the other slots, not left out because not every slot sets it,
/// and reads back on the third material only.
#[test]
fn a_filament_override_of_some_slots_writes_nil_for_the_others() {
    for key in print_keys::FILAMENT_OVERRIDES {
        assert!(
            print_keys::key(key).is_some_and(KeyDef::overrides_printer),
            "{key}"
        );
    }
    let settings = json!({
        "printer_settings_id": "Bench Mini 0.4 nozzle",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "nozzle_diameter": ["0.4"],
        "retraction_length": ["0.8"],
        "print_settings_id": "Fine",
        "layer_height": "0.08",
        "filament_settings_id": ["Generic PETG", "Generic PLA", "Generic TPU"],
        "filament_type": ["PETG", "PLA", "TPU"],
        "filament_colour": ["#FF0000", "#00FF00", "#0000FF"],
        "filament_retraction_length": ["nil", "nil", "0.4"],
    });
    let data = project(
        "BambuStudio-02.00.00.00",
        vec![(
            "Metadata/project_settings.config",
            settings.to_string().into_bytes(),
        )],
        Vec::new(),
    );
    let imported = read_file("override.3mf", &data, None).unwrap();
    assert!(imported.report.problems.is_empty(), "{:?}", imported.report);
    let lengths: Vec<Option<&Value>> = imported
        .materials
        .iter()
        .map(|(m, _)| m.filament.get("filament_retract_length"))
        .collect();
    assert_eq!(lengths, [None, None, Some(&json!(0.4))]);

    // A body of each material, each in a component of its own.
    let dir = mitcad_model::testing::unique_temp_dir("print-override");
    let path = dir.join("override.3mf").to_str().unwrap().to_owned();
    std::fs::write(&path, &data).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let result = command(
        &mut doc,
        json!({"cmd": "import_print_profile", "path": path}),
    );
    let materials: Vec<String> = result["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    for (i, material) in materials.iter().enumerate() {
        let name = format!("Slot {}", i + 1);
        doc.create_component(
            Some(name.as_str()),
            Transform::translation([20.0 * i as f64, 0.0, 0.0]),
            true,
        )
        .unwrap();
        small_block(&mut doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        command(
            &mut doc,
            json!({"cmd": "set_component_material", "component": name, "material": material}),
        );
    }

    let (exported, back) = export_to(&mut doc, "override-back.3mf", "bambu_studio");
    assert_eq!(exported["filaments"], json!([1, 2, 3]));
    let package = mitcad_3mf::read_package(&back).unwrap();
    let written: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(
        written["filament_retraction_length"],
        json!(["nil", "nil", "0.4"])
    );
    // Only the third filament says it differs from its preset.
    let different = written["different_settings_to_system"].as_array().unwrap();
    assert!(
        !different[1]
            .as_str()
            .unwrap()
            .contains("filament_retraction_length")
    );
    assert!(
        different[3]
            .as_str()
            .unwrap()
            .contains("filament_retraction_length")
    );
    let read_back = read_file("override-back.3mf", &back, None).unwrap();
    let lengths: Vec<Option<&Value>> = read_back
        .materials
        .iter()
        .map(|(m, _)| m.filament.get("filament_retract_length"))
        .collect();
    assert_eq!(lengths, [None, None, Some(&json!(0.4))]);

    // PrusaSlicer's list the same way.
    let (_, prusa) = export_to(&mut doc, "override-prusa.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&prusa).unwrap();
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        config.contains("; filament_retract_length = nil,nil,0.4\n"),
        "{config}"
    );
}

/// Imports a slicer's settings file into a new design and gives it a body
/// of each of `materials` (indices of the materials read), each in a
/// component of its own.
pub(super) fn design_of(file: &str, data: &[u8], materials: &[usize]) -> Document<MockKernel> {
    let dir = mitcad_model::testing::unique_temp_dir("print-slots");
    let path = dir.join(file).to_str().unwrap().to_owned();
    std::fs::write(&path, data).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let result = command(
        &mut doc,
        json!({"cmd": "import_print_profile", "path": path}),
    );
    assert_eq!(result["report"]["problems"], json!([]), "{result}");
    let ids: Vec<String> = result["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    for (i, material) in materials.iter().enumerate() {
        let name = format!("Slot {}", i + 1);
        doc.create_component(
            Some(name.as_str()),
            Transform::translation([20.0 * i as f64, 0.0, 0.0]),
            true,
        )
        .unwrap();
        small_block(&mut doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        command(
            &mut doc,
            json!({"cmd": "set_component_material", "component": name,
                   "material": ids[*material]}),
        );
    }
    doc
}

/// The export's warnings as text.
fn warnings_of(result: &Value) -> Vec<String> {
    result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect()
}

/// mitcad#640: Bambu Studio fails to slice a project whose lists across
/// filaments keep the size of the project the settings were imported
/// from. Two bodies of the second and the fourth of a four-filament
/// project's filaments: every per-filament list has two entries, those
/// two filaments' (found by `filament_self_index`, or by the presets the
/// printer block keeps), the flushing matrix is their 2 x 2 (the import's
/// purge volumes of their pairs, mitcad#705) and the vector their two
/// pairs (their colours'). Bambu Studio's -1 stays in its own file, and is left
/// out of OrcaSlicer's and PrusaSlicer's with a warning.
#[test]
fn bambu_lists_across_filaments_have_one_entry_per_written_slot() {
    let matrix: Vec<String> = (0..16)
        .map(|k| {
            let (i, j) = (k / 4, k % 4);
            if i == j {
                "0".to_owned()
            } else {
                ((i + 1) * 100 + j + 1).to_string()
            }
        })
        .collect();
    let vector: Vec<String> = (0..8)
        .map(|k| ((k / 2 + 1) * 10 + k % 2).to_string())
        .collect();
    for self_index in [true, false] {
        let mut settings = json!({
            "printer_settings_id": "Bench Mini 0.4 nozzle",
            "printable_area": ["0x0", "180x0", "180x180", "0x180"],
            "nozzle_diameter": ["0.4"],
            "print_settings_id": "0.08mm Extra Fine",
            "layer_height": "0.08",
            "raft_first_layer_expansion": "-1",
            "tree_support_wall_count": "-1",
            "filament_settings_id": ["Generic PETG", "Generic PLA", "Generic PETG HF", "Generic TPU"],
            "filament_type": ["PETG", "PLA", "PETG", "TPU"],
            "filament_colour": ["#FF0000", "#00FF00", "#0000FF", "#FFFFFF"],
            "filament_map": ["1", "1", "1", "1"],
            "default_filament_colour": ["#111111", "#222222", "#333333", "#444444"],
            "volumetric_speed_coefficients":
                ["1 0 0 0 0 0", "2 0 0 0 0 0", "3 0 0 0 0 0", "4 0 0 0 0 0"],
            "flush_volumes_matrix": matrix,
            "flush_volumes_vector": vector,
        });
        if self_index {
            settings["filament_self_index"] = json!(["1", "2", "3", "4"]);
        }
        let data = project(
            "BambuStudio-02.00.00.00",
            vec![(
                "Metadata/project_settings.config",
                settings.to_string().into_bytes(),
            )],
            Vec::new(),
        );
        let mut doc = design_of(&format!("four-{self_index}.3mf"), &data, &[1, 3]);
        let (exported, back) = export_to(&mut doc, "two-bambu.3mf", "bambu_studio");
        assert_eq!(exported["filaments"], json!([1, 2]));
        let package = mitcad_3mf::read_package(&back).unwrap();
        let written: Map<String, Value> =
            serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap())
                .unwrap();
        assert_eq!(
            written["filament_settings_id"],
            json!(["Generic PLA", "Generic TPU"])
        );
        assert_eq!(
            written["default_filament_colour"],
            json!(["#222222", "#444444"])
        );
        assert_eq!(
            written["volumetric_speed_coefficients"],
            json!(["2 0 0 0 0 0", "4 0 0 0 0 0"])
        );
        assert_eq!(written["filament_map"], json!(["1", "1"]));
        // From the second to the fourth filament and back: the project's
        // table became the design's purge volumes of those materials
        // (mitcad#705), and each slot's values are its colour's (out, in).
        assert_eq!(
            written["flush_volumes_matrix"],
            json!(["0", "204", "402", "0"])
        );
        let green = mitcad_model::print_purge::colour_defaults(Some([0.0, 1.0, 0.0]));
        let white = mitcad_model::print_purge::colour_defaults(Some([1.0, 1.0, 1.0]));
        let text = |x: f64| json!(print_keys::number_text(x));
        assert_eq!(
            written["flush_volumes_vector"],
            json!([
                text(green.out),
                text(green.into),
                text(white.out),
                text(white.into)
            ])
        );
        assert_eq!(written["flush_multiplier"], "1");
        if self_index {
            assert_eq!(written["filament_self_index"], json!(["1", "2"]));
        }
        let per_filament = per_filament_keys(Dialect::BambuStudio);
        for (key, value) in &written {
            if let Value::Array(items) = value
                && per_filament(key.as_str())
            {
                assert_eq!(items.len(), 2, "{key}: {value}");
            }
        }
        // Bambu Studio's own -1.
        assert_eq!(written["raft_first_layer_expansion"], "-1");
        assert_eq!(written["tree_support_wall_count"], "-1");
        assert!(
            !warnings_of(&exported)
                .iter()
                .any(|w| w.contains("raft_first_layer_expansion")),
            "{exported}"
        );

        // OrcaSlicer refuses a project with them: left out, with a warning.
        let (exported, back) = export_to(&mut doc, "two-orca.3mf", "orcaslicer");
        let package = mitcad_3mf::read_package(&back).unwrap();
        let written: Map<String, Value> =
            serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap())
                .unwrap();
        let warnings = warnings_of(&exported);
        for key in ["raft_first_layer_expansion", "tree_support_wall_count"] {
            assert!(!written.contains_key(key), "{key}");
            assert!(
                warnings.iter().any(|w| w.contains(key)),
                "{key}: {warnings:?}"
            );
        }
        assert_eq!(written["layer_height"], "0.08");
        // PrusaSlicer takes no negative raft expansion either.
        let (exported, back) = export_to(&mut doc, "two-prusa.3mf", "prusaslicer");
        let package = mitcad_3mf::read_package(&back).unwrap();
        let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
            .unwrap()
            .to_owned();
        assert!(
            !config.contains("; raft_first_layer_expansion ="),
            "{config}"
        );
        assert!(
            warnings_of(&exported)
                .iter()
                .any(|w| w.contains("raft_first_layer_expansion")),
            "{exported}"
        );
    }
}

/// mitcad#640: PrusaSlicer refuses a list of choices written as strings
/// (`"uniform_cooling";"uniform_cooling"`): its per-extruder choices are
/// `,`-separated and unquoted, its strings `;`-separated and quoted only
/// where they need it. The reader takes both forms.
#[test]
fn prusaslicer_lists_of_choices_are_comma_separated() {
    let config = "printer_settings_id = Bench MK\n\
        bed_shape = 0x0,250x0,250x210,0x210\n\
        nozzle_diameter = 0.4,0.4\n\
        print_settings_id = Standard\n\
        layer_height = 0.2\n\
        filament_settings_id = \"Generic PLA\";\"Generic PETG\"\n\
        filament_type = PLA;PETG\n\
        filament_colour = #FF0000;#00FF00\n\
        cooling_slowdown_logic = \"uniform_cooling\";\"consistent_surface\"\n";
    let imported = read_file("bench.ini", config.as_bytes(), None).unwrap();
    let logic: Vec<&Value> = imported
        .materials
        .iter()
        .map(|(m, _)| &m.filament["cooling_slowdown_logic"])
        .collect();
    assert_eq!(
        logic,
        [&json!("uniform_cooling"), &json!("consistent_surface")]
    );

    let mut doc = design_of("bench.ini", config.as_bytes(), &[0, 1]);
    let (exported, back) = export_to(&mut doc, "choices-prusa.3mf", "prusaslicer");
    assert_eq!(exported["filaments"], json!([1, 2]));
    let package = mitcad_3mf::read_package(&back).unwrap();
    let written = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        written.contains("; cooling_slowdown_logic = uniform_cooling,consistent_surface\n"),
        "{written}"
    );
    assert!(
        written.contains("; filament_type = PLA;PETG\n"),
        "{written}"
    );
    assert!(
        written.contains("; filament_settings_id = \"Generic PLA\";\"Generic PETG\"\n"),
        "{written}"
    );
    assert!(
        written.contains("; nozzle_diameter = 0.4,0.4\n"),
        "{written}"
    );
    // And it reads back.
    let read_back = read_file("choices-prusa.3mf", &back, None).unwrap();
    let logic: Vec<&Value> = read_back
        .materials
        .iter()
        .map(|(m, _)| &m.filament["cooling_slowdown_logic"])
        .collect();
    assert_eq!(
        logic,
        [&json!("uniform_cooling"), &json!("consistent_surface")]
    );
}

/// mitcad#1062: a material that names the slicer's filament preset it is
/// based on (`filament_settings_id`, kept per slicer, as the Materials
/// window's Filament tab sets it with `edit_material`) is written under
/// that preset's name and with it as the preset its filament inherits
/// from (`inherits_group`), so that the slicer builds on its own preset; a
/// material that names none keeps its own name and inherits from nothing.
/// The Bambu Studio family takes a member's name for the others;
/// PrusaSlicer has its own, and its project config keeps the parents as
/// `inherits_cummulative` (the process, each filament, the printer;
/// mitcad#1065).
#[test]
fn a_material_s_slicer_preset_is_written_and_inherited() {
    let mut doc = design();
    command(
        &mut doc,
        json!([
            {"cmd": "create_material", "id": "shop-petg-hf", "name": "Shop PETG HF",
             "based_on": "pla", "filament": {"filament_type": "PETG"}},
            {"cmd": "set_component_material", "component": "Arm", "material": "shop-petg-hf"},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "shop-petg-hf",
               "filament": {"filament_settings_id": {"bambu_studio": "Generic PETG HF @BBL X1C",
                                                     "prusaslicer": "Prusament PETG"}}}),
    );
    let project_settings = |data: &[u8]| -> Map<String, Value> {
        let package = mitcad_3mf::read_package(data).unwrap();
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap()
    };
    // Bambu Studio: the root's PLA first (no preset: its own name), then
    // the arm's material under its preset; the process names no preset,
    // the printer its own.
    let (_, data) = export_to(&mut doc, "preset-bambu.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert_eq!(
        settings["filament_settings_id"],
        json!(["PLA Plastic", "Generic PETG HF @BBL X1C"])
    );
    assert_eq!(
        settings["inherits_group"],
        json!(["", "", "Generic PETG HF @BBL X1C", "Bench 0.4 nozzle"])
    );
    // OrcaSlicer takes Bambu Studio's name.
    let (_, data) = export_to(&mut doc, "preset-orca.3mf", "orcaslicer");
    let settings = project_settings(&data);
    assert_eq!(
        settings["filament_settings_id"][1],
        "Generic PETG HF @BBL X1C"
    );
    assert_eq!(settings["inherits_group"][2], "Generic PETG HF @BBL X1C");
    // PrusaSlicer its own.
    let (_, data) = export_to(&mut doc, "preset-prusa.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        config.contains("; filament_settings_id = \"PLA Plastic\";\"Prusament PETG\"\n"),
        "{config}"
    );
    // The arm's filament inherits from its preset, the root's PLA from
    // nothing; the process names no preset, the printer its PrusaSlicer one.
    assert!(
        config.contains("; inherits_cummulative = ;;\"Prusament PETG\";\"Bench MK\"\n"),
        "{config}"
    );

    // Without a preset again (the field emptied): its own name, inheriting
    // from nothing.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "shop-petg-hf",
               "filament": {"filament_settings_id": null}}),
    );
    let (_, data) = export_to(&mut doc, "no-preset.3mf", "bambu_studio");
    let settings = project_settings(&data);
    assert_eq!(
        settings["filament_settings_id"],
        json!(["PLA Plastic", "Shop PETG HF"])
    );
    assert_eq!(
        settings["inherits_group"],
        json!(["", "", "", "Bench 0.4 nozzle"])
    );
    let (_, data) = export_to(&mut doc, "no-preset-prusa.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        config.contains("; inherits_cummulative = ;;;\"Bench MK\"\n"),
        "{config}"
    );
}

#[test]
fn a_project_s_slots_of_one_preset_in_two_colours_stay_two_materials() {
    // Four slots as a slicer's project keeps them: one preset in white and
    // in black, and presets whose names differ only in the type word. A
    // read filament keeps its colour only as its appearance (mitcad#1053),
    // so the filament merge (mitcad#1054) compares that: else the two
    // PETG slots became one material and the TPU slid into the third slot.
    let settings = json!({
        "printer_settings_id": "Bench Mini 0.4 nozzle",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "nozzle_diameter": ["0.4"],
        "print_settings_id": "Standard",
        "layer_height": "0.2",
        "filament_settings_id": ["Generic PETG @Bench Mini", "Generic PLA @Bench Mini",
                                 "Generic PETG @Bench Mini", "Generic TPU @Bench Mini"],
        "filament_type": ["PETG", "PLA", "PETG", "TPU"],
        "filament_colour": ["#FFFFFF", "#FFFFFF", "#000000", "#FFFFFF"],
        "nozzle_temperature": ["255", "220", "255", "240"],
        "filament_density": ["1.27", "1.24", "1.27", "1.24"],
    });
    let data = project(
        "BambuStudio-02.00.00.00",
        vec![(
            "Metadata/project_settings.config",
            settings.to_string().into_bytes(),
        )],
        Vec::new(),
    );
    let dir = mitcad_model::testing::unique_temp_dir("print-colour-slots");
    let path = dir.join("slots.3mf").to_str().unwrap().to_owned();
    std::fs::write(&path, &data).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let result = command(
        &mut doc,
        json!({"cmd": "import_print_profile", "path": path, "into": "design"}),
    );
    let names: Vec<&str> = result["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Generic PETG @Bench Mini",
            "Generic PLA @Bench Mini",
            // Two materials of a design never share a name: the second
            // slot of the same preset takes the design's next free name.
            "Generic PETG @Bench Mini (1)",
            "Generic TPU @Bench Mini"
        ],
        "{result}"
    );
    let id = |i: usize| result["materials"][i]["id"].as_str().unwrap().to_owned();
    let black = doc.material(&id(2)).unwrap();
    assert_eq!(black.filament["filament_type"], "PETG");
    assert_eq!(black.filament["temperature"], 255.0);
    assert_eq!(black.density, 1.27);
    assert!(black.variants.is_empty());
    let tpu = doc.material(&id(3)).unwrap();
    assert_eq!(tpu.filament["filament_type"], "TPU");
    assert_eq!(tpu.filament["temperature"], 240.0);

    // A slot each, written back in the project's order and colours.
    let mut doc = design_of("slots.3mf", &data, &[0, 1, 2, 3]);
    let (_, back) = export_to(&mut doc, "slots-back.3mf", "bambu_studio");
    let package = mitcad_3mf::read_package(&back).unwrap();
    let written: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(
        written["filament_type"],
        json!(["PETG", "PLA", "PETG", "TPU"])
    );
    assert_eq!(
        written["filament_settings_id"],
        settings["filament_settings_id"]
    );
    assert_eq!(
        written["filament_colour"],
        json!(["#FFFFFF", "#FFFFFF", "#000000", "#FFFFFF"])
    );
}
