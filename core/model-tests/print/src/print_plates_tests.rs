// SPDX-License-Identifier: MIT
//! Print plates (mitcad#1050) with the mock kernel: bodies and
//! occurrences on plates with counts (one plate per item, what no plate
//! names on the first), the plate's values in the settings order
//! (profile < plate < component < occurrence < body), the plates saved
//! with the design, and the exports: a Bambu-family 3MF of two plates
//! with the plate's spiral mode only on its plate, PrusaSlicer's beds
//! where it lays them out, a Cura export of a file per plate (the tests
//! read the files written), the copies of an item apart also without
//! centring, the pieces apart beyond their brims (mitcad#1491) and a
//! plate's layout beside the design; and the items of a
//! deleted last feature's body or a deleted last occurrence staying
//! missing after a reload (mitcad#1320).

use serde_json::{Map, Value, json};

use crate::print_files_tests::{command, design, small_block};
use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
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
    object_range(model, object, 0)
}

/// The range of a build object's mesh along `axis` (0 x, 1 y) as the file
/// places it.
fn object_range(
    model: &mitcad_3mf::Model,
    object: &mitcad_3mf::BuildObject,
    axis: usize,
) -> (f64, f64) {
    let mut range = (f64::INFINITY, f64::NEG_INFINITY);
    for &(part, k) in &object.components {
        let placement = model.parts[part].placements[k];
        for vertex in &model.parts[part].mesh.vertices {
            let v = placement.apply(*vertex)[axis];
            range = (range.0.min(v), range.1.max(v));
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
fn the_copies_of_an_item_lie_apart_also_without_centring() {
    // mitcad#1311: the root's block twice on the first plate (with the
    // arm's, on no plate), written with and without Center on Plate.
    let (mut doc, root_body, _, _) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": root_body.to_string(), "count": 2}]}),
    );
    let export = |doc: &mut Document<MockKernel>, name: &str, center: bool| {
        let path = folder(name).join("copies.3mf");
        let result = command(
            doc,
            json!({"cmd": "export", "path": path.to_str().unwrap(), "format": "3mf",
                   "plates": "one_file", "center": center}),
        );
        assert_eq!(result["plates"][0]["pieces"], 3, "{result}");
        let read = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read.objects.len(), 3, "{result}");
        read.objects
            .iter()
            .map(|object| object_x(&read, object))
            .collect::<Vec<(f64, f64)>>()
    };

    // Not centred: the second copy beside the first, a gap of 5 mm
    // between them, not on top of it.
    let apart = export(&mut doc, "plates-copies-apart", false);
    let (first, second) = (apart[0], apart[1]);
    let width = first.1 - first.0;
    assert!(width > 1.0, "{apart:?}");
    assert!(second.0 - first.0 >= width - 1e-6, "{apart:?}");
    assert!((second.0 - (first.1 + 5.0)).abs() < 1e-6, "{apart:?}");
    assert!(((second.1 - second.0) - width).abs() < 1e-6, "{apart:?}");

    // Centred, as before: the three pieces side by side, 5 mm apart, the
    // row centred on the 256 mm plate.
    let row = export(&mut doc, "plates-copies-centred", true);
    for pair in row.windows(2) {
        assert!((pair[1].0 - (pair[0].1 + 5.0)).abs() < 1e-6, "{row:?}");
    }
    let middle = (row[0].0 + row[2].1) / 2.0;
    assert!((middle - 128.0).abs() < 1e-6, "{row:?}");
}

#[test]
fn neighbours_lie_apart_beyond_their_brims() {
    // mitcad#1491: the root's block twice on the first plate (with the
    // arm's, on no plate); the plate's brim widens the gaps between the
    // pieces, the copies' too, so that the slicer's brims do not meet.
    let (mut doc, root_body, _, _) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": root_body.to_string(), "count": 2}]}),
    );
    let export = |doc: &mut Document<MockKernel>, name: &str, extra: Value| {
        let path = folder(name).join("brims.3mf");
        let mut export = json!({"cmd": "export", "path": path.to_str().unwrap(),
                                "format": "3mf", "plates": "one_file"});
        for (key, value) in extra.as_object().unwrap() {
            export[key.as_str()] = value.clone();
        }
        let result = command(doc, export);
        assert_eq!(result["plates"][0]["pieces"], 3, "{result}");
        let read = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read.objects.len(), 3, "{result}");
        read.objects
            .iter()
            .map(|object| object_x(&read, object))
            .collect::<Vec<(f64, f64)>>()
    };
    // The gaps between neighbours along X, each `gap` mm.
    let assert_gaps = |row: &[(f64, f64)], gap: f64| {
        for pair in row.windows(2) {
            assert!(
                (pair[1].0 - (pair[0].1 + gap)).abs() < 1e-6,
                "{gap}: {row:?}"
            );
        }
    };

    // No brim: 5 mm apart, as before.
    assert_gaps(&export(&mut doc, "brims-none", json!({})), 5.0);

    // An 8 mm brim, of the slicers' automatic type, on the plate: 8 + 5 + 8
    // mm, the row still centred on the 256 mm plate.
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "plate1",
               "values": {"brim_width": 8, "brim_type": "auto_brim"}}),
    );
    let row = export(&mut doc, "brims-8", json!({}));
    assert_gaps(&row, 21.0);
    let middle = (row[0].0 + row[2].1) / 2.0;
    assert!((middle - 128.0).abs() < 1e-6, "{row:?}");

    // Not centred, the second copy of the item as far beyond the first.
    let apart = export(&mut doc, "brims-copies", json!({"center": false}));
    assert!((apart[1].0 - (apart[0].1 + 21.0)).abs() < 1e-6, "{apart:?}");

    // No brim by its type: 5 mm again; a spacing of its own replaces the
    // 5 mm.
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "plate1",
               "values": {"brim_type": "no_brim"}}),
    );
    assert_gaps(&export(&mut doc, "brims-off", json!({})), 5.0);
    assert_gaps(
        &export(&mut doc, "brims-spacing", json!({"spacing": 12})),
        12.0,
    );

    // A spacing off the bed, or out of its range, is refused.
    let path = folder("brims-refused").join("refused.3mf");
    let path = path.to_str().unwrap();
    for bad in [
        json!({"cmd": "export", "path": path, "format": "3mf", "spacing": 5}),
        json!({"cmd": "export", "path": path, "format": "3mf", "plates": "one_file",
               "spacing": -1}),
    ] {
        assert!(doc.command(&bad.to_string()).is_err(), "{bad}");
    }
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

/// A 4 mm box of the root at `x`, a new body.
fn box_at(x: f64) -> Value {
    json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [x, 0],
           "length": 4, "width": 4, "height": 4, "operation": "new_body"}})
}

/// The bodies of the `print_plates` query's placements on no plate.
fn unplaced_bodies(plates: &Value) -> Vec<String> {
    plates["unplaced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["body"].as_str().unwrap().to_owned())
        .collect()
}

/// mitcad#1320: the uid of a deleted last feature is not given out again
/// after a save and a load, so the plate item that keeps its body stays
/// missing, and the new feature's body is not on the plate (it prints
/// unplaced, on the first plate).
#[test]
fn a_deleted_last_feature_s_plate_item_stays_missing_after_a_reload() {
    let mut doc = Document::new(MockKernel::default());
    let first = command(&mut doc, box_at(0.0));
    assert_eq!(first["uid"], "F1", "{first}");
    let second = command(&mut doc, box_at(20.0));
    assert_eq!(second["uid"], "F2", "{second}");
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": "F2.b0", "count": 5}]}),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    // The item stays on its plate, missing.
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"][0]["items"][0]["body"], "F2.b0", "{plates}");
    assert_eq!(plates["plates"][0]["items"][0]["missing"], true, "{plates}");

    // The file keeps the number the next feature takes.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["next_feature"], json!(3), "{saved}");
    assert!(file.get("next_occurrence").is_none());
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.recompute();

    // A new box is F3, not F2: the item stays missing and the new body is
    // on no plate.
    let added = command(&mut loaded, box_at(40.0));
    assert_eq!(added["uid"], "F3", "{added}");
    let plates = query(&loaded, json!({"query": "print_plates"}));
    let items = plates["plates"][0]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{plates}");
    assert_eq!(items[0]["body"], "F2.b0", "{plates}");
    assert_eq!(items[0]["count"], 5, "{plates}");
    assert_eq!(items[0]["missing"], true, "{plates}");
    assert_eq!(unplaced_bodies(&plates), ["F1.b0", "F3.b0"], "{plates}");
    // The missing item prints nothing: the two unplaced boxes only.
    assert_eq!(plates["plates"][0]["pieces"], 2, "{plates}");
    // One past the largest uid again: no number of its own in the file.
    let file: Value = serde_json::from_str(&loaded.to_json()).unwrap();
    assert!(file.get("next_feature").is_none());
}

/// mitcad#1320: likewise the uid of a deleted last occurrence: placing the
/// component again after a save and a load makes a new occurrence, which
/// is not on the plate that keeps the deleted one's item.
#[test]
fn a_deleted_last_occurrence_s_plate_item_stays_missing_after_a_reload() {
    let (mut doc, _, arm_body, arm) = plated();
    assert_eq!(arm, "O1");
    let component: ComponentUid = doc
        .instances()
        .into_iter()
        .find(|i| i.body == arm_body)
        .expect("the arm's body is placed")
        .component;
    let second: OccurrenceUid = doc
        .add_occurrence(
            component,
            ComponentUid::ROOT,
            Transform::translation([80.0, 0.0, 0.0]),
        )
        .unwrap();
    assert_eq!(second.to_string(), "O2");
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"occurrence": "O2", "count": 5}]}),
    );
    doc.delete_occurrence(second).unwrap();
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(
        plates["plates"][0]["items"][0]["occurrence"], "O2",
        "{plates}"
    );
    assert_eq!(plates["plates"][0]["items"][0]["missing"], true, "{plates}");

    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["next_occurrence"], json!(3), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.recompute();

    // Placed again: O3, not O2. The item stays missing; the new
    // occurrence's body is on no plate.
    let third: OccurrenceUid = loaded
        .add_occurrence(
            component,
            ComponentUid::ROOT,
            Transform::translation([120.0, 0.0, 0.0]),
        )
        .unwrap();
    assert_eq!(third.to_string(), "O3");
    loaded.recompute();
    let plates = query(&loaded, json!({"query": "print_plates"}));
    let items = plates["plates"][0]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{plates}");
    assert_eq!(items[0]["occurrence"], "O2", "{plates}");
    assert_eq!(items[0]["missing"], true, "{plates}");
    let unplaced: Vec<(String, String)> = plates["unplaced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| {
            (
                u["body"].as_str().unwrap().to_owned(),
                u["occurrence"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert!(
        unplaced.contains(&(arm_body.to_string(), "O3".to_owned())),
        "{plates}"
    );
    assert!(
        unplaced.contains(&(arm_body.to_string(), "O1".to_owned())),
        "{plates}"
    );
}

/// The bodies an export's plate lists.
fn plate_bodies(result: &Value, plate: usize) -> Vec<String> {
    result["plates"][plate]["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_body_with_an_item_of_its_own_prints_on_one_plate() {
    // mitcad#1310: the arm's body on the first plate as a body item, the
    // arm's occurrence on the second.
    let (mut doc, root_body, arm_body, arm) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "edit_print_plate", "plate": "plate2", "values": {"layer_height": 0.3}}),
    );
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": arm_body.to_string()}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate2",
               "items": [{"occurrence": arm.clone()}]}),
    );
    // The query counts what the export makes: nothing on the second.
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"][0]["pieces"], 2, "{plates}");
    assert_eq!(plates["plates"][1]["pieces"], 0, "{plates}");

    let path = folder("plates-own-item").join("plates.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "bambu_studio",
               "plates": "one_file"}),
    );
    let first = plate_bodies(&result, 0);
    assert!(first.contains(&arm_body.to_string()), "{result}");
    assert!(first.contains(&root_body.to_string()), "{result}");
    assert_eq!(plate_bodies(&result, 1), Vec::<String>::new(), "{result}");
    assert_eq!(result["plates"][1]["pieces"], 0);
    let warnings = result["plates"][1]["warnings"].to_string();
    assert!(warnings.contains("print on Plate 1"), "{warnings}");
    // Its settings are the first plate's, where it prints.
    let on = query(
        &doc,
        json!({"query": "print_settings", "body": arm_body.to_string(), "occurrence": arm.clone()}),
    );
    assert_eq!(on["plate"]["id"], "plate1", "{on}");
    let height = on["process"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "layer_height")
        .cloned()
        .unwrap();
    assert_eq!(height["value"], 0.2, "{height}");
    // The second plate's block lists no object of it.
    let package = mitcad_3mf::read_package(&std::fs::read(&path).unwrap()).unwrap();
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    let blocks = plate_blocks(&model);
    assert_eq!(blocks[0].matches("<model_instance>").count(), 2, "{model}");
    assert_eq!(blocks[1].matches("<model_instance>").count(), 0, "{model}");
    // Its layout alike.
    let layout = query(&doc, json!({"query": "plate_layout", "plate": "plate2"}));
    assert_eq!(layout["pieces"], 0, "{layout}");
    assert_eq!(layout["bodies"], json!([]));
}

/// [`plated`] with a pin placed in the arm (a block of its own): the
/// arm's body and occurrence, the pin's body and its occurrence's path
/// from the root (`O1/O<n>`).
fn with_pin() -> (Document<MockKernel>, BodyUid, String, BodyUid, String) {
    let (mut doc, _, arm_body, arm) = plated();
    let arm_uid: OccurrenceUid = arm.parse().unwrap();
    let arm_component = doc.assembly().occurrence(arm_uid).unwrap().component;
    doc.activate_component(arm_component).unwrap();
    let (_, pin) = doc
        .create_component(Some("Pin"), Transform::translation([0.0, 20.0, 0.0]), true)
        .unwrap();
    let pin_body = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let pin_path = format!("{arm}/{pin}");
    (doc, arm_body, arm, pin_body, pin_path)
}

#[test]
fn an_occurrence_item_inside_another_s_prints_on_one_plate() {
    // mitcad#1310: the arm on the first plate, the pin inside it on the
    // second: the outer item's plate takes the pin's body.
    let (mut doc, arm_body, arm, pin_body, pin) = with_pin();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"occurrence": arm.clone()}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate2",
               "items": [{"occurrence": pin.clone(), "count": 2}]}),
    );
    let path = folder("plates-nested").join("plates.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "bambu_studio",
               "plates": "one_file"}),
    );
    let first = plate_bodies(&result, 0);
    assert!(first.contains(&pin_body.to_string()), "{result}");
    assert!(first.contains(&arm_body.to_string()), "{result}");
    assert_eq!(plate_bodies(&result, 1), Vec::<String>::new(), "{result}");
    assert_eq!(result["plates"][1]["pieces"], 0, "{result}");
    // Each placement once in the whole file: the root's block, the arm's
    // and the pin's.
    let read = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    let placements: usize = read.parts.iter().map(|p| p.placements.len()).sum();
    assert_eq!(placements, 3, "{result}");
    let plates = query(&doc, json!({"query": "print_plates"}));
    assert_eq!(plates["plates"][1]["pieces"], 0, "{plates}");
    let layout = query(&doc, json!({"query": "plate_layout", "plate": "plate2"}));
    assert_eq!(layout["bodies"], json!([]), "{layout}");
}

/// A `width` x `depth` x 5 block in the active component.
fn block(doc: &mut Document<MockKernel>, width: f64, depth: f64) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(width), &num(depth))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

#[test]
fn a_plate_s_row_of_pieces_wraps_within_its_bed() {
    // mitcad#1316: ten 50 mm blocks on the first of two 256 mm plates
    // (with the root's block, on no plate), the arm on the second. As the
    // design has them (no automatic orientation): 50 x 20 mm each.
    let (mut doc, _, _, arm) = plated();
    let wide = block(&mut doc, 50.0, 20.0);
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": wide.to_string(), "count": 10}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate2",
               "items": [{"occurrence": arm.clone()}]}),
    );
    let path = folder("plates-wrap").join("plates.3mf");
    let export = |doc: &mut Document<MockKernel>| {
        command(
            doc,
            json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "bambu_studio",
                   "plates": "one_file", "auto_orient": false}),
        )
    };
    let result = export(&mut doc);
    assert_eq!(result["plates"][0]["pieces"], 11, "{result}");
    assert_eq!(result["plates"][1]["origin"], json!([307.2, 0.0]));
    // Every build item lies within its plate's bed: the first's on
    // 0..256 mm, the second's a fifth of the width beyond; none across
    // the gap between them.
    let read = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(read.objects.len(), 12);
    let within =
        |range: (f64, f64), from: f64| range.0 >= from - 1e-6 && range.1 <= from + 256.0 + 1e-6;
    for object in &read.objects {
        let (x, y) = (
            object_range(&read, object, 0),
            object_range(&read, object, 1),
        );
        assert!(within(y, 0.0), "{}: y {y:?}", object.name);
        assert!(
            within(x, 0.0) || within(x, 307.2),
            "{}: x {x:?}",
            object.name
        );
    }
    let warnings = result["plates"][0]["warnings"].to_string();
    assert!(!warnings.contains("do not fit"), "{warnings}");

    // Sixty of them take fifteen rows: more than the bed's depth, named.
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": wide.to_string(), "count": 60}]}),
    );
    let result = export(&mut doc);
    let warnings = result["plates"][0]["warnings"].to_string();
    assert!(
        warnings.contains("Plate 1: the 61 pieces do not fit on the 256 x 256 mm bed"),
        "{warnings}"
    );
    assert!(result["warnings"].to_string().contains("do not fit"));
    assert!(
        !result["plates"][1]["warnings"]
            .to_string()
            .contains("do not fit")
    );
}

#[test]
fn plate_files_whose_names_clash_are_written_apart() {
    // mitcad#1316: `A/B` and `A:B` both make `A_B`.
    let (mut doc, root_body, _, arm) = plated();
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "A/B"}));
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "A:B"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "A/B",
               "items": [{"body": root_body.to_string()}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "A:B",
               "items": [{"occurrence": arm}]}),
    );
    let dir = folder("plates-clash");
    let path = dir.join("parts.3mf");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path.to_str().unwrap(), "dialect": "cura",
               "plates": "file_per_plate"}),
    );
    let first = dir.join("parts - A_B.3mf");
    let second = dir.join("parts - A_B (2).3mf");
    assert_eq!(
        result["files"],
        json!([first.to_str().unwrap(), second.to_str().unwrap()]),
        "{result}"
    );
    assert_eq!(result["plates"][0]["path"], first.to_str().unwrap());
    assert_eq!(result["plates"][1]["path"], second.to_str().unwrap());
    // Each file is its plate's.
    for (file, objects) in [(&first, 1), (&second, 1)] {
        let read = mitcad_3mf::read(&std::fs::read(file).unwrap()).unwrap();
        let placements: usize = read.parts.iter().map(|p| p.placements.len()).sum();
        assert_eq!(placements, objects, "{}", file.display());
    }
}

#[test]
fn an_occurrence_item_follows_its_occurrence_to_another_parent() {
    // mitcad#1318: three of the arm on the second plate; the arm moved
    // into a cell placed in the root.
    let (mut doc, _, _, arm) = plated();
    let (_, cell) = doc
        .create_component(Some("Cell"), Transform::IDENTITY, true)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let cell = cell.to_string();
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(&mut doc, json!({"cmd": "add_print_plate"}));
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate2",
               "items": [{"occurrence": arm.clone(), "count": 3}]}),
    );
    let item = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "print_plates"}))["plates"][1].clone()
    };
    assert_eq!(item(&doc)["pieces"], 3);

    let moved = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": arm.clone(), "parent": cell.clone()}),
    );
    let updated = moved["updated"].as_array().unwrap();
    assert!(updated.iter().any(|u| u == "Print plates"), "{moved}");
    let plate = item(&doc);
    let path = format!("{cell}/{arm}");
    assert_eq!(plate["items"][0]["occurrence"], path, "{plate}");
    assert_eq!(plate["items"][0]["count"], 3);
    assert!(plate["items"][0].get("missing").is_none(), "{plate}");
    assert_eq!(plate["pieces"], 3, "{plate}");
    assert_eq!(
        query(&doc, json!({"query": "print_plates"}))["unplaced"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the root's block alone"
    );

    // Undo puts the old path back.
    command(&mut doc, json!({"cmd": "undo"}));
    let plate = item(&doc);
    assert_eq!(plate["items"][0]["occurrence"], arm, "{plate}");
    assert_eq!(plate["pieces"], 3);
}
