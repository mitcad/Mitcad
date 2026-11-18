// SPDX-License-Identifier: MIT
//! Print settings set on a component, an occurrence or a body reach the
//! slicer (mitcad#1052): the path a user takes, a material assigned to a
//! component and print values set on the levels below it, then the
//! design sent to a Bambu-family slicer as 3D Print sends it, and what
//! the written `project_settings.config` and `model_settings.config` carry.

use serde_json::{Map, Value, json};

use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

use super::print_files_tests::{command, small_block};

/// The written 3MF's two settings files: the project's (JSON) and the
/// parts' (XML text).
fn settings_files(data: &[u8]) -> (Map<String, Value>, String) {
    let package = mitcad_3mf::read_package(data).unwrap();
    let project: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    (project, model)
}

/// The `model_settings.config` block of a part, by its name.
fn part_block<'a>(model: &'a str, name: &str) -> &'a str {
    let marker = format!("<metadata key=\"name\" value=\"{name}\"/>");
    let at = model
        .find(&marker)
        .unwrap_or_else(|| panic!("no part {name} in {model}"));
    let start = model[..at].rfind("<part ").expect("a part's block");
    let end = at + model[at..].find("</part>").expect("the part's end");
    &model[start..end]
}

/// A part's metadata value in its block.
fn part_value<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("<metadata key=\"{key}\" value=\"");
    let at = block.find(&marker)? + marker.len();
    Some(&block[at..at + block[at..].find('"')?])
}

/// A filament list's entry for a 1-based extruder.
fn slot_entry<'a>(project: &'a Map<String, Value>, key: &str, extruder: &str) -> &'a str {
    let slot: usize = extruder.parse::<usize>().unwrap() - 1;
    project
        .get(key)
        .and_then(|v| v.get(slot))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{key} has no entry {extruder}: {project:?}"))
}

/// Sends the design's bodies as 3D Print does: on the print bed, every
/// body, the Bambu Studio dialect.
fn send(doc: &mut Document<MockKernel>, name: &str) -> (Value, Vec<u8>) {
    send_for(doc, name, "bambu_studio")
}

/// Sends the design's bodies as 3D Print does, for a slicer.
fn send_for(doc: &mut Document<MockKernel>, name: &str, dialect: &str) -> (Value, Vec<u8>) {
    send_with(doc, name, dialect, json!({}))
}

/// [`send_for`] with more of the export's fields (`print_materials`,
/// mitcad#2008).
fn send_with(
    doc: &mut Document<MockKernel>,
    name: &str,
    dialect: &str,
    more: Value,
) -> (Value, Vec<u8>) {
    let dir = mitcad_model::testing::unique_temp_dir("print-send");
    let path = dir.join(name).to_str().unwrap().to_owned();
    let rows: Value =
        serde_json::from_str(&doc.query(&json!({"query": "bodies"}).to_string()).unwrap()).unwrap();
    let bodies: Vec<Value> = rows
        .as_array()
        .map(|rows| rows.iter().map(|r| r["uid"].clone()).collect())
        .unwrap_or_default();
    let mut export = json!({"cmd": "export", "path": path, "format": "3mf", "bodies": bodies,
                            "refinement": "medium", "on_bed": true, "dialect": dialect});
    for (key, value) in more.as_object().unwrap() {
        export[key] = value.clone();
    }
    let result = command(doc, export);
    (result, std::fs::read(&path).unwrap())
}

/// The PETG HF material of the tests: a nozzle temperature and a bed
/// temperature of its own, as a user types them in Materials.
fn petg_hf(doc: &mut Document<MockKernel>) {
    command(
        doc,
        json!({"cmd": "create_material", "id": "petg-hf", "name": "PETG HF",
               "based_on": "pla",
               "filament": {"filament_type": "PETG", "temperature": 255,
                            "bed_temperature": 70}}),
    );
}

/// A part's material and own values as the export reports them.
fn printed<'a>(result: &'a Value, name: &str) -> &'a Value {
    result["printed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no {name} in {result}"))
}

#[test]
fn a_component_s_material_reaches_the_slicer_with_its_own_filament_slot() {
    // Two components: Lid and Box, a block each; the root has no material
    // of its own (the design's default steel, which prints as PLA,
    // mitcad#1727).
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Lid"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("Box"), Transform::translation([40.0, 0.0, 0.0]), true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    // PETG HF assigned to the Lid as Material..., Assign does.
    petg_hf(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Lid", "material": "petg-hf"}),
    );
    let (result, data) = send(&mut doc, "two-components.3mf");
    let (project, model) = settings_files(&data);
    let lid = part_block(&model, "Body1");
    let lid_slot = part_value(lid, "extruder").unwrap();
    // The Lid's part has a filament of its own, with PETG HF's values.
    assert_eq!(
        slot_entry(&project, "filament_settings_id", lid_slot),
        "PETG HF"
    );
    assert_eq!(slot_entry(&project, "filament_type", lid_slot), "PETG");
    assert_eq!(slot_entry(&project, "nozzle_temperature", lid_slot), "255");
    assert_eq!(slot_entry(&project, "hot_plate_temp", lid_slot), "70");
    // The Box's does not: no material of its own prints as the print
    // default, PLA (mitcad#1727: not the design's default steel), with
    // MitCad's generic PLA values, which the export says.
    let other = part_block(&model, "Body2");
    let other_slot = part_value(other, "extruder").unwrap();
    assert_ne!(lid_slot, other_slot);
    assert_eq!(
        slot_entry(&project, "filament_settings_id", other_slot),
        "PLA Plastic"
    );
    assert_eq!(slot_entry(&project, "filament_type", other_slot), "PLA");
    assert_eq!(
        slot_entry(&project, "nozzle_temperature", other_slot),
        "210"
    );
    assert_eq!(slot_entry(&project, "hot_plate_temp", other_slot), "60");
    let warnings = result["warnings"].to_string();
    assert!(
        warnings.contains("PLA Plastic (filament 2) sets no ")
            && warnings.contains("nozzle temperature")
            && warnings.contains("written as generic PLA's"),
        "{warnings}"
    );
    // The log line of tests: each part's material and own values.
    assert_eq!(printed(&result, "Body1")["material"], "petg-hf");
    assert_eq!(printed(&result, "Body2")["material"], "pla");
    let own = |name: &str| printed(&result, name)["own_values"].as_u64().unwrap();
    assert!(own("Body1") > own("Body2"), "{result}");
    // PrusaSlicer's lists alike, PETG HF's filament first.
    let (_, data) = send_for(&mut doc, "two-components-prusa.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(config.contains("; temperature = 255,210\n"), "{config}");
    assert!(config.contains("; filament_type = PETG;PLA\n"), "{config}");

    // The same with the Box a plain PLA (the built-in, its type only): its
    // filament keeps its type and takes the generic PLA's temperature,
    // not PETG HF's.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Box", "material": "pla"}),
    );
    let (_, data) = send(&mut doc, "pla-box.3mf");
    let (project, model) = settings_files(&data);
    let other_slot = part_value(part_block(&model, "Body2"), "extruder").unwrap();
    assert_eq!(slot_entry(&project, "filament_type", other_slot), "PLA");
    assert_eq!(
        slot_entry(&project, "nozzle_temperature", other_slot),
        "210"
    );
    let lid_slot = part_value(part_block(&model, "Body1"), "extruder").unwrap();
    assert_eq!(slot_entry(&project, "nozzle_temperature", lid_slot), "255");
}

#[test]
fn occurrence_and_body_settings_reach_the_slicer() {
    // Lid once, Box twice; the Lid's material is PLA, the Box's body
    // PETG HF of its own.
    let mut doc = Document::new(MockKernel::default());
    let (_lid, lid1) = doc
        .create_component(Some("Lid"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_box, box1) = doc
        .create_component(Some("Box"), Transform::translation([40.0, 0.0, 0.0]), true)
        .unwrap();
    let box_body = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    petg_hf(&mut doc);
    command(
        &mut doc,
        json!([
            {"cmd": "set_component_material", "component": "Lid", "material": "pla"},
            {"cmd": "set_body_material", "uid": box_body.to_string(), "material": "petg-hf"},
            // The occurrence level: the Lid's placement prints four walls.
            {"cmd": "set_property", "owner": lid1.to_string(), "set": "Print Process",
             "key": "perimeters", "value": 4},
            // The body level: the Box's body 30 % infill.
            {"cmd": "set_property", "owner": box_body.to_string(), "set": "Print Process",
             "key": "fill_density", "value": 30},
        ]),
    );
    let (result, data) = send(&mut doc, "levels.3mf");
    let (project, model) = settings_files(&data);
    let lid = part_block(&model, "Body1");
    let lid_slot = part_value(lid, "extruder").unwrap();
    assert_eq!(slot_entry(&project, "filament_type", lid_slot), "PLA");
    assert_eq!(part_value(lid, "wall_loops"), Some("4"), "{model}");
    let boxed = part_block(&model, "Body2");
    let box_slot = part_value(boxed, "extruder").unwrap();
    assert_ne!(lid_slot, box_slot);
    assert_eq!(slot_entry(&project, "filament_type", box_slot), "PETG");
    assert_eq!(slot_entry(&project, "nozzle_temperature", box_slot), "255");
    assert_eq!(
        part_value(boxed, "sparse_infill_density"),
        Some("30%"),
        "{model}"
    );
    assert_eq!(part_value(boxed, "wall_loops"), None, "{model}");
    assert_eq!(printed(&result, "Body2")["material"], "petg-hf");
    assert!(
        !result["warnings"].to_string().contains("prints otherwise"),
        "{result}"
    );

    // A second Box whose occurrence prints otherwise (mitcad#1061): the
    // Box's body is a part per placement, each with its own values, the
    // body's own infill and material in both.
    let box2 = doc.copy_occurrence(box1, None).unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": box2.to_string(), "set": "Print Process",
               "key": "perimeters", "value": 6}),
    );
    let (result, data) = send(&mut doc, "two-boxes.3mf");
    assert!(
        !result["warnings"].to_string().contains("prints otherwise"),
        "{result}"
    );
    let (project, model) = settings_files(&data);
    let first = part_block(&model, "Body2 (Box:1)");
    let second = part_block(&model, "Body2 (Box:2)");
    assert_eq!(part_value(first, "wall_loops"), None, "{model}");
    assert_eq!(part_value(second, "wall_loops"), Some("6"), "{model}");
    for block in [first, second] {
        assert_eq!(
            part_value(block, "sparse_infill_density"),
            Some("30%"),
            "{model}"
        );
        let slot = part_value(block, "extruder").unwrap();
        assert_eq!(slot_entry(&project, "filament_type", slot), "PETG");
    }
    assert_eq!(printed(&result, "Body2 (Box:1)")["material"], "petg-hf");
    assert_eq!(printed(&result, "Body2 (Box:2)")["material"], "petg-hf");
    // The Lid is one part still, named after its body.
    assert_eq!(
        part_value(part_block(&model, "Body1"), "wall_loops"),
        Some("4")
    );
    assert_eq!(result["printed"].as_array().unwrap().len(), 3, "{result}");
    assert_eq!(result["filaments"].as_array().unwrap().len(), 3, "{result}");
}

#[test]
fn placements_that_print_otherwise_are_parts_of_their_own() {
    // Box:1 and Box:2 of a PLA Box (mitcad#1061): Box:2's occurrence
    // prints six walls.
    let mut doc = Document::new(MockKernel::default());
    let (_box, box1) = doc
        .create_component(Some("Box"), Transform::IDENTITY, true)
        .unwrap();
    let body = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let box2 = doc.copy_occurrence(box1, None).unwrap();
    command(
        &mut doc,
        json!([
            {"cmd": "set_component_material", "component": "Box", "material": "pla"},
            {"cmd": "set_property", "owner": box2.to_string(), "set": "Print Process",
             "key": "perimeters", "value": 6},
        ]),
    );
    let (result, data) = send(&mut doc, "box-placements.3mf");
    // One body written, two parts: each placement's own values, no
    // warning about settings the file does not carry.
    assert_eq!(result["bodies"], json!([body.to_string()]), "{result}");
    assert!(
        !result["warnings"].to_string().contains("prints otherwise"),
        "{result}"
    );
    let (project, model) = settings_files(&data);
    let first = part_block(&model, "Body1 (Box:1)");
    let second = part_block(&model, "Body1 (Box:2)");
    assert_eq!(part_value(first, "wall_loops"), None, "{model}");
    assert_eq!(part_value(second, "wall_loops"), Some("6"), "{model}");
    // Both PLA: one filament slot.
    let slot = part_value(first, "extruder").unwrap();
    assert_eq!(part_value(second, "extruder"), Some(slot), "{model}");
    assert_eq!(slot_entry(&project, "filament_type", slot), "PLA");
    assert_eq!(result["filaments"], json!([1, 1]), "{result}");
    let parts = result["printed"].as_array().unwrap();
    assert_eq!(parts.len(), 2, "{result}");
    assert!(parts.iter().all(|p| p["body"] == json!(body.to_string())));
    assert_eq!(printed(&result, "Body1 (Box:1)")["material"], "pla");
    assert_eq!(printed(&result, "Body1 (Box:2)")["material"], "pla");
    let own = |name: &str| printed(&result, name)["own_values"].as_u64().unwrap();
    assert!(own("Body1 (Box:2)") > own("Body1 (Box:1)"), "{result}");

    // Box:2 printing as Box:1 again: one part, named after its body.
    command(
        &mut doc,
        json!({"cmd": "remove_property", "owner": box2.to_string(), "set": "Print Process",
               "key": "perimeters"}),
    );
    let (result, data) = send(&mut doc, "box-alike.3mf");
    let (_, model) = settings_files(&data);
    part_block(&model, "Body1");
    assert_eq!(result["printed"].as_array().unwrap().len(), 1, "{result}");
}

#[test]
fn placements_of_two_materials_take_a_filament_slot_each() {
    // A Box placed in Lid A (PLA) and in Lid B (PETG HF), without a
    // material of its own (mitcad#1061): its body prints in both.
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("A"), Transform::IDENTITY, true)
        .unwrap();
    let (_box, box1) = doc
        .create_component(Some("Box"), Transform::IDENTITY, true)
        .unwrap();
    let body = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("B"), Transform::translation([40.0, 0.0, 0.0]), true)
        .unwrap();
    doc.copy_occurrence(box1, None).unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    petg_hf(&mut doc);
    command(
        &mut doc,
        json!([
            {"cmd": "set_component_material", "component": "A", "material": "pla"},
            {"cmd": "set_component_material", "component": "B", "material": "petg-hf"},
        ]),
    );
    let (result, data) = send(&mut doc, "two-materials.3mf");
    assert!(
        !result["warnings"].to_string().contains("prints otherwise"),
        "{result}"
    );
    // A part per placement, each in a filament slot of its material.
    assert_eq!(result["filaments"], json!([1, 2]), "{result}");
    let parts = result["printed"].as_array().unwrap();
    let materials: Vec<&Value> = parts.iter().map(|p| &p["material"]).collect();
    assert_eq!(materials, [&json!("pla"), &json!("petg-hf")], "{result}");
    let names: Vec<&str> = parts.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert!(
        names[0].starts_with("Body1 (A:") && names[1].starts_with("Body1 (B:"),
        "{result}"
    );
    let (project, model) = settings_files(&data);
    let petg_slot = part_value(part_block(&model, names[1]), "extruder").unwrap();
    assert_eq!(slot_entry(&project, "filament_type", petg_slot), "PETG");
    assert_eq!(slot_entry(&project, "nozzle_temperature", petg_slot), "255");
    // The purge table has the same two slots, the body in each.
    let table: Value = serde_json::from_str(
        &doc.query(&json!({"query": "purge_volumes"}).to_string())
            .unwrap(),
    )
    .unwrap();
    let slots = table["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 2, "{table}");
    assert_eq!(slots[0]["material"], "pla", "{table}");
    assert_eq!(slots[1]["material"], "petg-hf", "{table}");
    for slot in slots {
        assert_eq!(slot["bodies"], json!([body.to_string()]), "{table}");
    }
}

#[test]
fn without_a_material_or_a_printer_the_slicer_keeps_its_own_settings() {
    // mitcad#2008 (in place of mitcad#1727's stand-in): one block at the
    // root, no material anywhere, no printer and no process: geometry
    // only. No project settings, no stand-in printer, process or PLA, no
    // machine G-code: the slicer keeps its own printer, process and
    // filaments, and the export has nothing to warn about.
    let mut doc = Document::new(MockKernel::default());
    let body = small_block(&mut doc);
    let material: Value = serde_json::from_str(
        &doc.query(&json!({"query": "body_material", "body": body.to_string()}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(material["source"], "default", "{material}");
    for dialect in [
        "bambu_studio",
        "orcaslicer",
        "qidi_studio",
        "prusaslicer",
        "cura",
    ] {
        let (result, data) = send_for(&mut doc, &format!("geometry-{dialect}.3mf"), dialect);
        assert_eq!(result["delivery"], "geometry", "{dialect}: {result}");
        assert_eq!(result["filaments"], json!([1]), "{dialect}: {result}");
        assert_eq!(result["warnings"], json!([]), "{dialect}: {result}");
        assert_eq!(result.get("warning_messages"), None, "{dialect}: {result}");
        let package = mitcad_3mf::read_package(&data).unwrap();
        for file in [
            "Metadata/project_settings.config",
            "Metadata/Slic3r_PE.config",
        ] {
            assert!(package.file(file).is_none(), "{dialect}: {file}");
        }
        assert!(
            !package.files.iter().any(|(p, _)| p.starts_with("Cura/")),
            "{dialect}"
        );
        // The geometry is there, closed as designed.
        assert_eq!(mitcad_3mf::read(&data).unwrap().parts.len(), 1, "{dialect}");
    }
    // The Bambu family's parts: the name and slot 1, nothing else.
    let (_, data) = send(&mut doc, "geometry.3mf");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    let block = part_block(&model, "Body1");
    assert_eq!(part_value(block, "extruder"), Some("1"), "{model}");
    assert_eq!(block.matches("<metadata ").count(), 2, "{model}");
    for injected in ["G92 E0", "use_relative_e_distances", "Mitcad", "PLA"] {
        assert!(!model.contains(injected), "{injected}: {model}");
    }
    // PrusaSlicer's objects: the name and extruder 1.
    let (_, data) = send_for(&mut doc, "geometry-prusa.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/Slic3r_PE_model.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        model.contains("<metadata type=\"object\" key=\"extruder\" value=\"1\"/>"),
        "{model}"
    );
    // Cura: the extruder number, no per-model process value.
    let (_, data) = send_for(&mut doc, "geometry-cura.3mf", "cura");
    let read = mitcad_3mf::read_package(&data).unwrap();
    let cura: Vec<&String> = read
        .objects
        .iter()
        .flat_map(|o| o.metadata.iter().map(|(name, _)| name))
        .filter(|name| name.starts_with("cura:"))
        .collect();
    assert_eq!(cura, ["cura:extruder_nr"], "{cura:?}");
}

#[test]
fn a_material_chosen_for_the_job_makes_a_print_project() {
    // mitcad#2008: the print job's material for the body (print_materials)
    // sends a print project. Without a printer the project names a
    // stand-in, which the export says (mitcad#1727).
    let mut doc = Document::new(MockKernel::default());
    let body = small_block(&mut doc);
    let job = json!({"print_materials": {body.to_string(): "pla"}});
    let (result, data) = send_with(&mut doc, "pla.3mf", "bambu_studio", job.clone());
    assert_eq!(result["delivery"], "project", "{result}");
    let (project, model) = settings_files(&data);
    assert_eq!(project["filament_settings_id"], json!(["PLA Plastic"]));
    assert_eq!(project["filament_type"], json!(["PLA"]));
    assert_eq!(project["printer_settings_id"], "Mitcad printer");
    assert_eq!(
        part_value(part_block(&model, "Body1"), "extruder"),
        Some("1")
    );
    assert_eq!(printed(&result, "Body1")["material"], "pla");
    // For agents: the step, with the command.
    let warnings: Vec<&str> = result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no printer profile") && w.contains("import_print_profile")),
        "{result}"
    );
    assert!(!warnings.iter().any(|w| w.contains("Steel")), "{result}");
    // For people: the same in plain words, naming no command.
    let shown: Vec<&str> = result["warning_messages"]
        .as_array()
        .unwrap_or_else(|| panic!("no warning_messages: {result}"))
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(shown.len(), warnings.len(), "{result}");
    assert!(
        shown
            .iter()
            .any(|w| w.starts_with("no printer was chosen") && w.contains("Printer row")),
        "{result}"
    );
    assert!(
        !shown.iter().any(|w| w.contains("import_print_profile")),
        "{result}"
    );
    // PrusaSlicer's project says it too.
    let (prusa, data) = send_with(&mut doc, "pla-prusa.3mf", "prusaslicer", job.clone());
    assert_eq!(prusa["delivery"], "project", "{prusa}");
    assert!(
        mitcad_3mf::read_package(&data)
            .unwrap()
            .file("Metadata/Slic3r_PE.config")
            .is_some()
    );
    assert!(
        prusa["warnings"]
            .to_string()
            .contains("import_print_profile"),
        "{prusa}"
    );
    // The job's None for the body: geometry only again.
    let none = json!({"print_materials": {body.to_string(): null}});
    let (result, _) = send_with(&mut doc, "none.3mf", "bambu_studio", none);
    assert_eq!(result["delivery"], "geometry", "{result}");

    // With a printer chosen at the root, the slicer gets its name and the
    // export says nothing of a printer; the body without a material
    // prints as PLA (mitcad#1727) in a print project.
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
             "values": {"nozzle_diameter": 0.4}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
             "value": "bench"},
        ]),
    );
    let (result, data) = send(&mut doc, "bench.3mf");
    assert_eq!(result["delivery"], "project", "{result}");
    let (project, _) = settings_files(&data);
    assert_eq!(project["printer_settings_id"], "Bench");
    assert!(
        !result["warnings"]
            .to_string()
            .contains("import_print_profile"),
        "{result}"
    );
    assert_eq!(result.get("warning_messages"), None, "{result}");
    assert_eq!(printed(&result, "Body1")["material"], "pla");
}

#[test]
fn a_job_of_some_bodies_without_a_material_is_refused_in_words() {
    // mitcad#2008: the slicer's project cannot leave one body to the
    // slicer while it sends materials for the others.
    let mut doc = Document::new(MockKernel::default());
    let first = small_block(&mut doc);
    let second = small_block(&mut doc);
    let dir = mitcad_model::testing::unique_temp_dir("print-send");
    let path = dir.join("mixed.3mf").to_str().unwrap().to_owned();
    let error = doc
        .command(
            &json!({"cmd": "export", "path": path, "format": "3mf", "dialect": "bambu_studio",
                    "print_materials": {first.to_string(): "pla", second.to_string(): null}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot leave one body"), "{error}");
    assert!(!error.contains(&first.to_string()), "{error}");
    assert!(!error.contains("Body"), "{error}");
    // Two materials: a slot each, as the job's list shows.
    petg_hf(&mut doc);
    let (result, data) = send_with(
        &mut doc,
        "two.3mf",
        "bambu_studio",
        json!({"print_materials": {first.to_string(): "pla", second.to_string(): "petg-hf"}}),
    );
    assert_eq!(result["delivery"], "project", "{result}");
    assert_eq!(result["filaments"], json!([1, 2]), "{result}");
    let (project, _) = settings_files(&data);
    assert_eq!(project["filament_type"], json!(["PLA", "PETG"]));
}

/// The `print_job` query's answer.
fn print_job(doc: &Document<MockKernel>, query: Value) -> Value {
    let mut query = query;
    query["query"] = json!("print_job");
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

#[test]
fn the_print_job_query_follows_the_list_s_rules() {
    // mitcad#2008: every body None, then one body's choice fills the
    // others, one body back to None refused, Set to all with None back to
    // geometry only; the status before Send.
    let mut doc = Document::new(MockKernel::default());
    let first = small_block(&mut doc).to_string();
    let second = small_block(&mut doc).to_string();
    petg_hf(&mut doc);
    let bodies = json!([first, second]);
    let job = print_job(&doc, json!({"bodies": bodies, "dialect": "bambu_studio"}));
    assert_eq!(job["delivery"], "geometry", "{job}");
    assert_eq!(job["status"], "receiver", "{job}");
    assert_eq!(job["common"], json!({"material": null, "mixed": false}));
    assert_eq!(job["bodies"][0]["material"], Value::Null, "{job}");
    // The first choice fills the other body.
    let job = print_job(
        &doc,
        json!({"bodies": bodies, "dialect": "bambu_studio",
               "choose": {"body": first, "material": "pla"}}),
    );
    assert_eq!(job["delivery"], "project", "{job}");
    assert_eq!(job["common"], json!({"material": "pla", "mixed": false}));
    assert_eq!(job["bodies"][1]["material"], "pla", "{job}");
    assert_eq!(job["bodies"][1]["material_name"], "PLA Plastic", "{job}");
    // No printer or process: partial, said in words.
    assert_eq!(job["status"], "partial", "{job}");
    let notes = job["notes"].to_string();
    assert!(notes.contains("No printer is chosen"), "{job}");
    assert!(notes.contains("No process is chosen"), "{job}");
    // Then only the body chosen changes: Mixed.
    let materials = json!({first.as_str(): "pla", second.as_str(): "pla"});
    let job = print_job(
        &doc,
        json!({"bodies": bodies, "dialect": "bambu_studio", "materials": materials,
               "choose": {"body": second, "material": "petg-hf"}}),
    );
    assert_eq!(job["common"], json!({"material": null, "mixed": true}));
    // One body back to None: refused in words, the list as it was.
    let job = print_job(
        &doc,
        json!({"bodies": bodies, "dialect": "bambu_studio", "materials": materials,
               "choose": {"body": second, "material": null}}),
    );
    let refused = job["refused"].as_str().unwrap();
    assert!(refused.contains("cannot leave one body"), "{job}");
    assert!(
        !refused.contains(&second) && !refused.contains("Body"),
        "{job}"
    );
    assert_eq!(job["bodies"][1]["material"], "pla", "{job}");
    // None and Set to all: geometry only again.
    let job = print_job(
        &doc,
        json!({"bodies": bodies, "dialect": "bambu_studio", "materials": materials,
               "choose": {"material": null}}),
    );
    assert_eq!(job["delivery"], "geometry", "{job}");
    assert_eq!(job["common"], json!({"material": null, "mixed": false}));
    // Pre-filled from a component's material: the other bodies take it,
    // as the list shows what is sent.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": second, "material": "petg-hf"}),
    );
    let job = print_job(&doc, json!({"bodies": bodies, "dialect": "bambu_studio"}));
    assert_eq!(job["bodies"][0]["material"], "petg-hf", "{job}");
    assert_eq!(job["bodies"][1]["material"], "petg-hf", "{job}");
}

#[test]
fn cura_takes_geometry_extruder_slots_and_process_values_only() {
    // mitcad#2008, Cura constrained: its printer and material choices are
    // Cura's own, and a chosen one is listed as left out, before Send and
    // in the export's warnings; each body's slot is its extruder.
    let mut doc = Document::new(MockKernel::default());
    let first = small_block(&mut doc).to_string();
    let second = small_block(&mut doc).to_string();
    petg_hf(&mut doc);
    let bodies = json!([first, second]);
    let job = print_job(&doc, json!({"bodies": bodies, "dialect": "cura"}));
    assert_eq!(job["capabilities"]["printer"], false, "{job}");
    assert_eq!(job["capabilities"]["materials"], false, "{job}");
    assert_eq!(
        job["capabilities"]["sends"],
        json!(["geometry", "extruder_slots", "process_values"])
    );
    assert!(
        job["capabilities"]["note"]
            .as_str()
            .unwrap()
            .contains("chosen in Cura"),
        "{job}"
    );
    assert_eq!(job["left_out"], json!([]), "{job}");
    let materials = json!({first.as_str(): "pla", second.as_str(): "petg-hf"});
    let job = print_job(
        &doc,
        json!({"bodies": bodies, "dialect": "cura", "materials": materials}),
    );
    assert_eq!(job["status"], "partial", "{job}");
    assert!(
        job["left_out"]
            .to_string()
            .contains("materials chosen are not sent"),
        "{job}"
    );
    let (result, data) = send_with(
        &mut doc,
        "cura.3mf",
        "cura",
        json!({"print_materials": materials}),
    );
    assert!(
        result["warnings"]
            .to_string()
            .contains("materials chosen are not sent"),
        "{result}"
    );
    // Each body's slot as its extruder number.
    let package = mitcad_3mf::read_package(&data).unwrap();
    let mut extruders: Vec<&str> = package
        .objects
        .iter()
        .flat_map(|o| o.metadata.iter())
        .filter(|(name, _)| name == "cura:extruder_nr")
        .map(|(_, value)| value.as_str())
        .collect();
    extruders.sort_unstable();
    assert_eq!(extruders, ["0", "1"], "{result}");
}

#[test]
fn a_plate_without_choices_goes_as_geometry_with_its_plate() {
    // mitcad#2008: Send the Design's Plates, nothing chosen: the plate's
    // objects in model_settings.config, no project settings.
    let mut doc = Document::new(MockKernel::default());
    small_block(&mut doc);
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    let dir = mitcad_model::testing::unique_temp_dir("print-send");
    let path = dir.join("plates.3mf").to_str().unwrap().to_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path, "dialect": "bambu_studio",
               "plates": "one_file"}),
    );
    assert_eq!(result["delivery"], "geometry", "{result}");
    let data = std::fs::read(&path).unwrap();
    let package = mitcad_3mf::read_package(&data).unwrap();
    assert!(package.file("Metadata/project_settings.config").is_none());
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        model.contains("<metadata key=\"plater_id\" value=\"1\"/>"),
        "{model}"
    );
    assert!(model.contains("<model_instance>"), "{model}");
}
