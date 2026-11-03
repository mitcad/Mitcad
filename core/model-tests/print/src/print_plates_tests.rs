// SPDX-License-Identifier: MIT
//! Print plates (mitcad#1050) with the mock kernel: bodies and
//! occurrences on plates with counts (one plate per item, what no plate
//! names on the first), the plate's values in the settings order
//! (profile < plate < component < occurrence < body), the plates saved
//! with the design, and the exports: a Bambu-family 3MF of two plates
//! with the plate's spiral mode only on its plate, PrusaSlicer's beds
//! where it lays them out, a Cura export of a file per plate (the tests
//! read the files written) and a plate's layout beside the design.

use serde_json::{Map, Value, json};

use crate::print_files_tests::{command, design};
use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// A query's answer.
fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The design of `print_files_tests::design` (a root block of PLA and an
/// arm's block of ABS with five walls of its own, placed at x = 40, on a
/// 256 mm bench printer with a 0.2 mm standard process), its two bodies
/// (the root's, the arm's) and the arm's occurrence.
fn plated() -> (Document<MockKernel>, BodyUid, BodyUid, String) {
    let doc = design();
    let bodies: Vec<BodyUid> = doc.bodies().iter().map(|b| b.uid).collect();
    assert_eq!(bodies.len(), 2);
    let arm = doc
        .instances()
        .into_iter()
        .find(|i| i.body == bodies[1])
        .expect("the arm's body is placed");
    assert_eq!(arm.path.len(), 1);
    let occurrence = arm.path[0].to_string();
    (doc, bodies[0], bodies[1], occurrence)
}

/// A process value as `print_settings` gives it for a body.
fn process_value(doc: &Document<MockKernel>, body: BodyUid, key: &str) -> Value {
    let settings = query(
        doc,
        json!({"query": "print_settings", "body": body.to_string()}),
    );
    settings["process"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == key)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn bodies_and_occurrences_go_on_plates_with_counts() {
    let (mut doc, root_body, arm_body, arm) = plated();
    // No plates: nothing to list.
    assert_eq!(
        query(&doc, json!({"query": "print_plates"}))["plates"],
        json!([])
    );

    let added = command(&mut doc, json!({"cmd": "add_print_plate"}));
    assert_eq!(added["id"], "plate1", "{added}");
    assert_eq!(added["name"], "Plate 1");
    assert_eq!(doc.undo_label(), Some("Add Plate 1"));
    let added = command(&mut doc, json!({"cmd": "add_print_plate", "name": "Vase"}));
    assert_eq!(added["id"], "plate2");
    // A name is a plate's once.
    assert!(
        doc.command(&json!({"cmd": "add_print_plate", "name": "vase"}).to_string())
            .is_err()
    );

    // Everything on no plate goes on the first.
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"][0]["pieces"], 2, "{plates}");
    assert_eq!(plates["plates"][1]["pieces"], 0);
    assert_eq!(plates["unplaced"].as_array().unwrap().len(), 2);

    // Four of the root's block on the vase plate: one undo step.
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "Vase",
               "items": [{"body": root_body.to_string(), "count": 4}]}),
    );
    let name = doc.body_name(root_body);
    assert_eq!(
        doc.undo_label().map(str::to_owned),
        Some(format!("Put 4 of {name} on Vase"))
    );
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(
        plates["plates"][1]["items"],
        json!([{"body": root_body.to_string(), "name": name, "count": 4}])
    );
    assert_eq!(plates["plates"][1]["pieces"], 4);
    assert_eq!(plates["plates"][0]["pieces"], 1, "the arm's block alone");

    // An item is on one plate: put on the first, it leaves the vase plate
    // and keeps its count.
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": root_body.to_string()}]}),
    );
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"][1]["items"], json!([]));
    assert_eq!(plates["plates"][0]["items"][0]["count"], 4);
    command(&mut doc, json!({"cmd": "undo"}));

    // The arm's occurrence brings its bodies: twice on the vase plate.
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate2",
               "items": [{"occurrence": arm.clone(), "count": 2}]}),
    );
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"][1]["pieces"], 6, "{plates}");
    assert_eq!(plates["unplaced"], json!([]));
    assert_eq!(plates["plates"][0]["pieces"], 0);
    // The arm's body prints on the vase plate through its occurrence.
    let on = query(
        &doc,
        json!({"query": "print_settings", "body": arm_body.to_string(), "occurrence": arm.clone()}),
    );
    assert_eq!(on["plate"]["id"], "plate2", "{on}");

    // A count is 1 to 999; a body that is not there is refused.
    for bad in [
        json!({"cmd": "place_on_plate", "plate": "plate2",
               "items": [{"body": root_body.to_string(), "count": 0}]}),
        json!({"cmd": "place_on_plate", "plate": "plate2", "items": [{"body": "F99.b0"}]}),
        json!({"cmd": "place_on_plate", "plate": "plate9",
               "items": [{"body": root_body.to_string()}]}),
    ] {
        assert!(doc.command(&bad.to_string()).is_err(), "{bad}");
    }

    // Taken off: back on the first plate.
    command(
        &mut doc,
        json!({"cmd": "remove_from_plates", "items": [{"occurrence": arm.clone()}]}),
    );
    assert!(doc.undo_label().unwrap().starts_with("Take "));
    assert!(doc.undo_label().unwrap().ends_with(" off Vase"));
    command(&mut doc, json!({"cmd": "undo"}));

    // Saved with the design and read back alike.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["print_plates"][1]["items"],
        json!([{"body": root_body.to_string(), "count": 4}, {"occurrence": [arm.clone()], "count": 2}])
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.print_plates(), doc.print_plates());
    assert_eq!(loaded.to_json(), saved);

    // Moved first, renamed, deleted: one undo step each; what was on a
    // deleted plate goes on the first.
    command(
        &mut doc,
        json!({"cmd": "move_print_plate", "plate": "Vase", "index": 0}),
    );
    assert_eq!(doc.print_plates()[0].id, "plate2");
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "plate1", "name": "Rest"}),
    );
    assert_eq!(doc.undo_label(), Some("Rename Plate 1"));
    command(
        &mut doc,
        json!({"cmd": "delete_print_plate", "plate": "Vase"}),
    );
    assert_eq!(doc.undo_label(), Some("Delete Vase"));
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"].as_array().unwrap().len(), 1);
    assert_eq!(plates["plates"][0]["pieces"], 2);
}

#[test]
fn a_plate_value_is_a_level_between_the_components_and_the_design() {
    let (mut doc, root_body, arm_body, arm) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "add_print_plate", "name": "Coarse"}),
    );
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "Coarse",
               "items": [{"body": root_body.to_string()}, {"occurrence": arm.clone()}]}),
    );
    // The plate's layer height and walls over the profile's.
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "Coarse",
               "values": {"layer_height": 0.3, "perimeters": 3}}),
    );
    assert_eq!(doc.undo_label(), Some("Edit Coarse"));
    let height = process_value(&doc, root_body, "layer_height");
    assert_eq!(height["value"], 0.3, "{height}");
    assert_eq!(height["from"], "plate");
    assert_eq!(height["owner_name"], "Coarse");
    assert_eq!(process_value(&doc, root_body, "perimeters")["value"], 3);
    // The arm's component sets its own walls: a component wins over the
    // plate.
    let walls = query(
        &doc,
        json!({"query": "print_settings", "body": arm_body.to_string(), "occurrence": arm.clone()}),
    )["process"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "perimeters")
        .cloned()
        .unwrap();
    assert_eq!(walls["value"], 5, "{walls}");
    assert_eq!(walls["from"], "property");

    // A body's own value wins over the plate's.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": root_body.to_string(), "set": "Print Process",
               "key": "layer_height", "value": 0.12}),
    );
    let height = process_value(&doc, root_body, "layer_height");
    assert_eq!(height["value"], 0.12, "{height}");
    assert_eq!(height["level"], "body");
    // The settings on another plate, asked for: the first plate has none,
    // the body's own value still.
    let elsewhere = query(
        &doc,
        json!({"query": "print_settings", "body": root_body.to_string(), "plate": "plate1"}),
    );
    assert_eq!(elsewhere["plate"]["id"], "plate1");
    command(&mut doc, json!({"cmd": "undo"}));
    let height = process_value(&doc, root_body, "layer_height");
    assert_eq!(height["value"], 0.3);

    // Only process keys, of their kind; null takes a value away.
    for bad in [
        json!({"cmd": "edit_print_plate", "plate": "Coarse", "values": {"nozzle_diameter": 0.6}}),
        json!({"cmd": "edit_print_plate", "plate": "Coarse", "values": {"layer_height": "thick"}}),
        json!({"cmd": "edit_print_plate", "plate": "Coarse", "name": " "}),
    ] {
        assert!(doc.command(&bad.to_string()).is_err(), "{bad}");
    }
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "Coarse", "values": {"layer_height": null}}),
    );
    assert_eq!(doc.undo_label(), Some("Clear Layer height of Coarse"));
    let height = process_value(&doc, root_body, "layer_height");
    assert_eq!(height["value"], 0.2);
    assert_eq!(height["from"], "profile");
    // The query lists the plate's own values.
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(
        plates["plates"][1]["values"][0]["key"], "perimeters",
        "{plates}"
    );
}

/// A temporary folder of this test's files.
fn folder(name: &str) -> std::path::PathBuf {
    mitcad_model::testing::unique_temp_dir(name)
}

/// The two plates of the export tests: the first with the root's block
/// (on no plate), the vase plate with two of the arm's occurrence, its
/// spiral mode on.
fn two_plates() -> Document<MockKernel> {
    let (mut doc, _, _, arm) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "Vase"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "Vase",
               "items": [{"occurrence": arm, "count": 2}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "Vase", "values": {"spiral_vase": true}}),
    );
    doc
}

/// The x range of a build object's mesh as the file places it.
fn object_x(model: &mitcad_3mf::Model, object: &mitcad_3mf::BuildObject) -> (f64, f64) {
    let mut range = (f64::INFINITY, f64::NEG_INFINITY);
    for &(part, k) in &object.components {
        let placement = model.parts[part].placements[k];
        for vertex in &model.parts[part].mesh.vertices {
            let x = placement.apply(*vertex)[0];
            range = (range.0.min(x), range.1.max(x));
        }
    }
    range
}

/// The `<plate>` blocks of a Bambu-family `model_settings.config`.
fn plate_blocks(model: &str) -> Vec<&str> {
    model
        .split("<plate>")
        .skip(1)
        .map(|rest| rest.split("</plate>").next().unwrap())
        .collect()
}

#[test]
fn a_bambu_project_of_two_plates_keeps_the_vase_mode_on_its_plate() {
    let mut doc = two_plates();
    let path = folder("plates-bambu").join("plates.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "bambu_studio",
               "plates": "one_file"}),
    );
    assert_eq!(result["files"], json!([path.to_str().unwrap()]), "{result}");
    assert_eq!(result["plates"].as_array().unwrap().len(), 2);
    assert_eq!(result["plates"][0]["pieces"], 1);
    assert_eq!(result["plates"][1]["pieces"], 2);
    assert_eq!(result["plates"][1]["origin"], json!([307.2, 0.0]));
    let data = std::fs::read(&path).unwrap();

    // Two plates in model_settings.config, the vase mode on the second.
    let package = mitcad_3mf::read_package(&data).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    let plates = plate_blocks(&model);
    assert_eq!(plates.len(), 2, "{model}");
    assert!(plates[0].contains("<metadata key=\"plater_id\" value=\"1\"/>"));
    assert!(plates[1].contains("<metadata key=\"plater_id\" value=\"2\"/>"));
    assert!(plates[1].contains("<metadata key=\"plater_name\" value=\"Vase\"/>"));
    assert!(!plates[0].contains("spiral_mode"), "{model}");
    assert!(
        plates[1].contains("<metadata key=\"spiral_mode\" value=\"true\"/>"),
        "{model}"
    );
    assert_eq!(plates[0].matches("<model_instance>").count(), 1);
    assert_eq!(plates[1].matches("<model_instance>").count(), 2);
    // Never an object's override, nor the project's.
    let objects_part = model.split("<plate>").next().unwrap();
    assert!(!objects_part.contains("spiral_mode"), "{model}");
    let settings: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert!(!settings.contains_key("spiral_mode"));

    // Each object lies on its plate: the first's on 0..256 mm, the vase
    // plate's on the plate a fifth of its width beyond.
    let read = mitcad_3mf::read(&data).unwrap();
    assert_eq!(read.objects.len(), 3);
    let (lo, hi) = object_x(&read, &read.objects[0]);
    assert!(lo >= -1e-6 && hi <= 256.0 + 1e-6, "{lo}..{hi}");
    for object in &read.objects[1..] {
        let (lo, hi) = object_x(&read, object);
        assert!(
            lo >= 307.2 - 1e-6 && hi <= 307.2 + 256.0 + 1e-6,
            "{lo}..{hi}"
        );
    }
    // The plate's object ids are those of the build.
    for index in 0..read.objects.len() {
        let id = mitcad_3mf::build_object_id(read.parts.len(), index);
        assert!(
            model.contains(&format!("<metadata key=\"object_id\" value=\"{id}\"/>")),
            "{model}"
        );
    }

    // One plate alone: in the path given, with its own block.
    let one = folder("plates-one").join("vase.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": one.to_str().unwrap(), "dialect": "orcaslicer",
               "plate": "Vase"}),
    );
    assert_eq!(result["plates"][0]["name"], "Vase", "{result}");
    let data = std::fs::read(&one).unwrap();
    assert_eq!(mitcad_3mf::read(&data).unwrap().objects.len(), 2);
}

#[test]
fn prusaslicer_takes_its_beds_from_where_the_objects_lie() {
    let mut doc = two_plates();
    let path = folder("plates-prusa").join("beds.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "prusaslicer",
               "plates": "one_file"}),
    );
    // The second bed right of the first, 0.3 of the bed's diagonal apart.
    let gap = (256f64.hypot(256.0) * 0.3).min(100.0);
    let origin = result["plates"][1]["origin"][0].as_f64().unwrap();
    assert!((origin - (256.0 + gap)).abs() < 1e-9, "{result}");
    let read = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    for object in &read.objects[1..] {
        let (lo, hi) = object_x(&read, object);
        assert!(
            lo >= origin - 1e-6 && hi <= origin + 256.0 + 1e-6,
            "{lo}..{hi}"
        );
    }
    // PrusaSlicer keeps the spiral vase for the whole project: the vase
    // plate's parts' own value is left out, with a warning.
    let package = mitcad_3mf::read_package(&std::fs::read(&path).unwrap()).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/Slic3r_PE_model.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(!model.contains("spiral_vase"), "{model}");
    let warnings = result["warnings"].to_string();
    assert!(warnings.contains("spiral_vase"), "{warnings}");
    // Cura has one build plate: one file of every plate is refused.
    assert!(
        doc.command(
            &json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "cura",
                    "plates": "one_file"})
            .to_string()
        )
        .is_err()
    );
}

#[test]
fn a_cura_export_writes_a_file_per_plate() {
    let mut doc = two_plates();
    let dir = folder("plates-cura");
    let path = dir.join("cura.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "cura",
               "plates": "file_per_plate"}),
    );
    let first = dir.join("cura - Plate 1.3mf");
    let vase = dir.join("cura - Vase.3mf");
    assert_eq!(
        result["files"],
        json!([first.to_str().unwrap(), vase.to_str().unwrap()]),
        "{result}"
    );
    assert!(!path.exists(), "no file of every plate");
    let metadata = |file: &std::path::Path| -> Vec<Vec<(String, String)>> {
        let package = mitcad_3mf::read_package(&std::fs::read(file).unwrap()).unwrap();
        package
            .objects
            .iter()
            .filter(|o| !o.metadata.is_empty())
            .map(|o| o.metadata.clone())
            .collect()
    };
    let spiral = |parts: &[Vec<(String, String)>]| {
        parts
            .iter()
            .filter(|m| m.iter().any(|(k, _)| k == "cura:magic_spiralize"))
            .count()
    };
    // The first plate's block alone, without the vase mode.
    let plate1 = metadata(&first);
    assert_eq!(plate1.len(), 1);
    assert_eq!(spiral(&plate1), 0);
    // The vase plate: the arm's block (one part of two placements), in
    // vase mode, each copy an object of the build.
    let plate2 = metadata(&vase);
    assert_eq!(plate2.len(), 1);
    assert_eq!(spiral(&plate2), 1);
    let read = mitcad_3mf::read(&std::fs::read(&vase).unwrap()).unwrap();
    assert_eq!(read.objects.len(), 2);
}

#[test]
fn a_plate_s_layout_lies_beside_the_design() {
    let (mut doc, root_body, _, _) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": root_body.to_string(), "count": 3}]}),
    );
    let layout = query(&doc, json!({"query": "plate_layout", "plate": "Plate 1"}));
    // Three of the root's block and the arm's (on no plate): two bodies.
    assert_eq!(layout["pieces"], 4, "{layout}");
    let bed = layout["bed"].as_array().unwrap();
    assert_eq!(bed.len(), 4);
    // Right of the design (its blocks end at x = 50), a fifth of the bed's
    // width beyond.
    let left = bed
        .iter()
        .map(|p| p[0].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min);
    assert!((left - (50.0 + 256.0 / 5.0)).abs() < 1e-6, "{layout}");
    let root = layout["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["body"] == root_body.to_string())
        .unwrap();
    assert_eq!(root["transforms"].as_array().unwrap().len(), 3);
    // Each on the bed: its translation inside the bed's outline in x.
    for rows in root["transforms"].as_array().unwrap() {
        let x = rows[0][3].as_f64().unwrap();
        assert!(x >= left - 1e-6 && x <= left + 256.0 + 1e-6, "{rows}");
    }
}
