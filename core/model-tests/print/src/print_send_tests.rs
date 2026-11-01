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
    let dir = mitcad_model::testing::unique_temp_dir("print-send");
    let path = dir.join(name).to_str().unwrap().to_owned();
    let rows: Value =
        serde_json::from_str(&doc.query(&json!({"query": "bodies"}).to_string()).unwrap()).unwrap();
    let bodies: Vec<Value> = rows
        .as_array()
        .map(|rows| rows.iter().map(|r| r["uid"].clone()).collect())
        .unwrap_or_default();
    let result = command(
        doc,
        json!({"cmd": "export", "path": path, "format": "3mf", "bodies": bodies,
               "refinement": "medium", "on_bed": true, "dialect": dialect}),
    );
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
    // of its own (steel, no filament values).
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
    // The Box's does not: the default material, written as PLA with
    // MitCad's generic PLA values, which the export says.
    let other = part_block(&model, "Body2");
    let other_slot = part_value(other, "extruder").unwrap();
    assert_ne!(lid_slot, other_slot);
    assert_eq!(
        slot_entry(&project, "filament_settings_id", other_slot),
        "Steel"
    );
    assert_eq!(slot_entry(&project, "filament_type", other_slot), "PLA");
    assert_eq!(
        slot_entry(&project, "nozzle_temperature", other_slot),
        "210"
    );
    assert_eq!(slot_entry(&project, "hot_plate_temp", other_slot), "60");
    let warnings = result["warnings"].to_string();
    assert!(
        warnings.contains("Steel (filament 2) sets no ")
            && warnings.contains("nozzle temperature")
            && warnings.contains("written as generic PLA's"),
        "{warnings}"
    );
    // The log line of tests: each part's material and own values.
    assert_eq!(printed(&result, "Body1")["material"], "petg-hf");
    assert_eq!(printed(&result, "Body2")["material"], "steel");
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
