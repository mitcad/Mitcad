// SPDX-License-Identifier: MIT
//! Printer and process profiles and print settings (mitcad#640, part 3)
//! with the mock kernel: profiles made, edited and deleted as undo steps,
//! the `Print` property sets with the key map's keys and types, the
//! inheritance root → components down the occurrence path → occurrence →
//! body in `print_settings`, the filament from the material, the build
//! volume, the design file and the comparison of versions.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;
use mitcad_model_diff::diff_states;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// A 20 x 20 x 20 block (F2.b0) on a rectangle on XY.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

/// A 10 x 10 x 5 block in the active component.
fn small_block(doc: &mut Document<MockKernel>) -> BodyUid {
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

/// A printer with a 256 x 256 bed from 0, 0 and 250 mm of height.
fn bench_printer(doc: &mut Document<MockKernel>) {
    command(
        doc,
        json!({"cmd": "create_print_profile", "kind": "printer", "name": "Bench printer",
               "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]],
                          "max_print_height": 250, "nozzle_diameter": 0.4,
                          "printer_settings_id": {"bambu_studio": "Bench 0.4 nozzle"}}}),
    );
}

/// The value of a key in a `print_settings` answer's set.
fn setting(settings: &Value, set: &str, key: &str) -> Value {
    settings[set]["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == key)
        .cloned()
        .unwrap_or_else(|| panic!("no {set} {key} in {settings}"))
}

#[test]
fn profiles_are_made_edited_and_deleted() {
    let mut doc = block();
    bench_printer(&mut doc);
    assert_eq!(undo_label(&doc), "Create Profile Bench printer");
    let profiles = query(&doc, json!({"query": "print_profiles"}));
    assert_eq!(profiles[0]["id"], "bench-printer");
    assert_eq!(profiles[0]["kind"], "printer");
    assert_eq!(profiles[0]["values"]["max_print_height"], 250.0);
    assert_eq!(
        profiles[0]["values"]["printer_settings_id"],
        json!({"bambu_studio": "Bench 0.4 nozzle"})
    );
    assert_eq!(profiles[0]["chosen_by"], json!([]));

    // Without a name: Process 1, process1 (mitcad#655: one pattern for
    // every kind; mitcad#828).
    let made = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process"}),
    );
    assert_eq!(made["id"], "process1");
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": "process1", "name": "Fine",
               "values": {"layer_height": 0.12, "fill_density": "15%"}}),
    );
    assert_eq!(undo_label(&doc), "Edit Process Process 1");
    let fine = doc.print_profile("process1").unwrap();
    assert_eq!(fine.name, "Fine");
    // A percentage is kept as its number.
    assert_eq!(fine.values["fill_density"], 15.0);
    // A copy of it, and nothing changed is no step.
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "based_on": "process1", "name": "Fine copy"}),
    );
    let copy = doc.print_profile("fine-copy").unwrap();
    assert_eq!(copy.based_on.as_deref(), Some("process1"));
    assert_eq!(copy.values["layer_height"], 0.12);
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": "process1", "name": "Fine"}),
    );
    assert_eq!(undo_label(&doc), "Create Profile Fine copy");

    // Refused: values of another set or kind, names taken, kinds changed.
    for (bad, expected) in [
        (
            json!({"cmd": "create_print_profile", "kind": "process",
                   "values": {"bed_shape": [[0, 0], [1, 0], [1, 1]]}}),
            "no key of the Print Process set",
        ),
        (
            json!({"cmd": "create_print_profile", "kind": "process",
                   "values": {"layer_height": "thin"}}),
            "layer_height",
        ),
        (
            json!({"cmd": "create_print_profile", "kind": "process", "name": "Fine"}),
            "a process profile is named Fine",
        ),
        (
            json!({"cmd": "create_print_profile", "name": "No kind"}),
            "needs a kind",
        ),
        (
            json!({"cmd": "edit_print_profile", "id": "process1", "kind": "printer"}),
            "kind cannot change",
        ),
        (
            json!({"cmd": "delete_print_profile", "id": "nothing"}),
            "there is no profile nothing",
        ),
    ] {
        let error = refused(&mut doc, bad);
        assert!(error.contains(expected), "{error}");
    }

    command(
        &mut doc,
        json!({"cmd": "delete_print_profile", "id": "fine-copy"}),
    );
    assert_eq!(undo_label(&doc), "Delete Profile Fine copy");
    assert!(doc.print_profile("fine-copy").is_none());
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(doc.print_profile("fine-copy").is_some());
}

#[test]
fn print_sets_keep_the_key_map_s_keys_and_types() {
    let mut doc = block();
    bench_printer(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer",
               "key": "profile", "value": "bench-printer"}),
    );
    assert_eq!(undo_label(&doc), "Use Printer Bench printer");
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "layer_height", "value": 0.2}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "fill_density", "value": "20%"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "F2.b0", "set": "Print Process",
               "key": "fill_pattern", "value": "gyroid"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "F2.b0", "set": "Print Slicer (PrusaSlicer)",
               "key": "gcode_comments", "value": "1"}),
    );
    let sets = query(&doc, json!({"query": "property_sets", "component": "C0"}));
    let properties: Vec<Value> = sets["levels"][0]["sets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["properties"].as_array().unwrap().clone())
        .collect();
    let find = |key: &str| {
        properties
            .iter()
            .find(|p| p["key"] == key)
            .cloned()
            .unwrap_or_else(|| panic!("no {key} in {sets}"))
    };
    let profile = find("profile");
    assert_eq!(profile["type"], "Print printer profile");
    assert_eq!(profile["name"], "Profile");
    assert_eq!(profile["choices"], json!(["bench-printer"]));
    let layer = find("layer_height");
    assert_eq!(layer["type"], "length");
    assert_eq!(layer["value"], 0.2);
    assert_eq!(layer["name"], "Layer height");
    let density = find("fill_density");
    assert_eq!(density["type"], "number");
    assert_eq!(density["value"], 20.0);
    assert_eq!(density["unit"], "%");
    // A choice is an own type of its choices.
    let body = query(&doc, json!({"query": "property_sets", "body": "F2.b0"}));
    let pattern = body["levels"][0]["sets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["properties"].as_array().unwrap().clone())
        .find(|p| p["key"] == "fill_pattern")
        .unwrap();
    assert_eq!(pattern["type"], "Print fill_pattern");
    assert!(
        pattern["choices"]
            .as_array()
            .unwrap()
            .contains(&json!("gyroid"))
    );

    // Refused: the filament (the material's), keys of another set or of no
    // set, values not of the key, profiles the design has not.
    for (bad, expected) in [
        (
            json!({"cmd": "set_property", "owner": "C0", "set": "Print Filament",
                   "key": "temperature", "value": 215}),
            "material",
        ),
        (
            json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
                   "key": "bed_shape", "value": "0x0,1x0,1x1"}),
            "no key of the Print Process set",
        ),
        (
            json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
                   "key": "fill_pattern", "value": "cheese"}),
            "fill_pattern",
        ),
        (
            json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
                   "key": "profile", "value": "bench-printer"}),
            "there is no process profile bench-printer",
        ),
        (
            json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer",
                   "key": "printer_settings_id", "value": "Bench"}),
            "a profile keeps it per slicer",
        ),
    ] {
        let error = refused(&mut doc, bad);
        assert!(error.contains(expected), "{error}");
    }

    // What the body prints with: the root's printer profile, the root's
    // and its own process values, its slicer key.
    let settings = query(&doc, json!({"query": "print_settings", "body": "F2.b0"}));
    assert_eq!(settings["printer"]["profile"]["id"], "bench-printer");
    let height = setting(&settings, "printer", "max_print_height");
    assert_eq!(height["value"], 250.0);
    assert_eq!(height["from"], "profile");
    assert_eq!(height["owner"], "C0");
    assert_eq!(height["text"], "250 mm");
    let pattern = setting(&settings, "process", "fill_pattern");
    assert_eq!(pattern["from"], "property");
    assert_eq!(pattern["level"], "body");
    assert_eq!(setting(&settings, "process", "fill_density")["text"], "20%");
    assert_eq!(
        settings["slicer"],
        json!([{"from": "property", "owner": "F2.b0", "level": "body",
                "owner_name": "Body1", "slicer": "prusaslicer",
                "key": "gcode_comments", "value": "1"}])
    );
}

#[test]
fn settings_inherit_down_the_occurrence_path() {
    let mut doc = Document::new(MockKernel::default());
    let (arm, arm1) = doc
        .create_component(Some("Arm"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    let (_pin, pin1) = doc
        .create_component(Some("Pin"), Transform::IDENTITY, true)
        .unwrap();
    let pin_body = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let arm2 = doc.copy_occurrence(arm1, None).unwrap();
    bench_printer(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process", "name": "Standard",
               "values": {"layer_height": 0.2, "perimeters": 2}}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process", "name": "Strong",
               "values": {"perimeters": 5, "fill_density": 40}}),
    );
    let set = |doc: &mut Document<MockKernel>, owner: String, key: &str, value: Value| {
        let set = if key == "printer" {
            "Print Printer"
        } else {
            "Print Process"
        };
        let key = if key == "printer" { "profile" } else { key };
        command(
            doc,
            json!({"cmd": "set_property", "owner": owner, "set": set, "key": key, "value": value}),
        );
    };
    set(&mut doc, "C0".to_owned(), "printer", json!("bench-printer"));
    set(&mut doc, "C0".to_owned(), "profile", json!("standard"));
    set(&mut doc, "C0".to_owned(), "fill_density", json!(15));
    set(&mut doc, arm.to_string(), "profile", json!("strong"));
    set(&mut doc, arm2.to_string(), "layer_height", json!(0.28));
    set(&mut doc, pin_body.to_string(), "perimeters", json!(3));

    // In the first arm: the body's own, the arm's profile over the root's
    // value, the root's profile for what the arm's has not.
    let settings = query(
        &doc,
        json!({"query": "print_settings", "body": pin_body.to_string(),
               "occurrence": "Arm:1/Pin:1"}),
    );
    let levels: Vec<Value> = settings["levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["owner"].clone())
        .collect();
    let pin = doc.body_component(pin_body).unwrap();
    assert_eq!(
        levels,
        [
            json!(pin_body),
            json!(pin1),
            json!(pin),
            json!(arm1),
            json!(arm),
            json!("C0")
        ]
    );
    assert_eq!(settings["process"]["profile"]["id"], "strong");
    assert_eq!(settings["printer"]["profile"]["id"], "bench-printer");
    let perimeters = setting(&settings, "process", "perimeters");
    assert_eq!(perimeters["value"], 3);
    assert_eq!(perimeters["from"], "property");
    assert_eq!(perimeters["level"], "body");
    let density = setting(&settings, "process", "fill_density");
    assert_eq!(density["value"], 40.0);
    assert_eq!(density["from"], "profile");
    assert_eq!(density["profile"], "strong");
    assert_eq!(density["owner"], json!(arm));
    let layer = setting(&settings, "process", "layer_height");
    assert_eq!(layer["value"], 0.2);
    assert_eq!(layer["profile"], "standard");
    assert_eq!(layer["owner"], "C0");

    // In the second arm its occurrence's own value wins.
    let second = query(
        &doc,
        json!({"query": "print_settings", "body": pin_body.to_string(),
               "occurrence": "Arm:2/Pin:1"}),
    );
    let layer = setting(&second, "process", "layer_height");
    assert_eq!(layer["value"], 0.28);
    assert_eq!(layer["level"], "occurrence");
    assert_eq!(layer["owner"], json!(arm2));

    // Without a path: the body's own component and the root.
    let alone = query(
        &doc,
        json!({"query": "print_settings", "body": pin_body.to_string()}),
    );
    assert_eq!(setting(&alone, "process", "fill_density")["value"], 15.0);
    assert_eq!(alone["process"]["profile"]["id"], "standard");
    // The design's own.
    let design = query(&doc, json!({"query": "print_settings"}));
    assert_eq!(
        design["levels"],
        json!([{"owner": "C0", "level": "component", "name": "Root"}])
    );

    // The filament is the material's: its density, and its appearance's
    // colour when it sets none.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": "pla"}),
    );
    let effective = doc.effective_print(&mitcad_model::PrintTarget::Body(pin_body, None));
    assert_eq!(effective.material, "pla");
    assert_eq!(effective.filament["filament_type"].0, "PLA");
    assert_eq!(effective.filament["filament_density"].0, 1.24);
    assert!(
        effective.filament["filament_colour"]
            .0
            .as_str()
            .unwrap()
            .starts_with('#')
    );

    // Deleting a profile takes the choices of it away.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_print_profile", "id": "strong"}),
    );
    assert_eq!(deleted["owners"], json!([arm]));
    let after = query(
        &doc,
        json!({"query": "print_settings", "body": pin_body.to_string(),
               "occurrence": "Arm:1/Pin:1"}),
    );
    assert_eq!(setting(&after, "process", "fill_density")["value"], 15.0);
    assert_eq!(after["process"]["profile"]["id"], "standard");
}

#[test]
fn the_build_volume_is_the_design_printer_s_at_the_origin() {
    let mut doc = block();
    let empty = query(&doc, json!({"query": "build_volume"}));
    assert_eq!(empty["bed"], Value::Null);
    bench_printer(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer",
               "key": "profile", "value": "bench-printer"}),
    );
    // A body placed beyond the bed.
    doc.create_component(Some("Far"), Transform::translation([200.0, 0.0, 0.0]), true)
        .unwrap();
    let far = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let volume = query(&doc, json!({"query": "build_volume"}));
    assert_eq!(volume["printer"]["id"], "bench-printer");
    assert_eq!(
        volume["bed"],
        json!([
            [-128.0, -128.0],
            [128.0, -128.0],
            [128.0, 128.0],
            [-128.0, 128.0]
        ])
    );
    assert_eq!(volume["height"], 250.0);
    let outside = volume["outside"].as_array().unwrap();
    assert_eq!(outside.len(), 1, "{volume}");
    assert_eq!(outside[0]["body"], json!(far));
    assert_eq!(outside[0]["reasons"], json!(["beyond the bed"]));
    // A component's own bed overrides the design's.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer",
               "key": "max_print_height", "value": 10}),
    );
    let lower = query(&doc, json!({"query": "build_volume"}));
    assert_eq!(lower["height"], 10.0);
    let reasons: Vec<Value> = lower["outside"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["reasons"].clone())
        .collect();
    assert!(
        reasons.contains(&json!(["taller than the printable height"])),
        "{lower}"
    );
}

#[test]
fn profiles_survive_the_design_file_and_are_compared() {
    let mut doc = block();
    let plain = doc.to_json();
    let before = doc.state().clone();
    bench_printer(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer",
               "key": "profile", "value": "bench-printer"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["print_profiles"][0]["id"], "bench-printer");
    assert_eq!(file["print_profiles"][0]["kind"], "printer");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(
        loaded
            .effective_print(&mitcad_model::PrintTarget::Component(ComponentUid::ROOT))
            .printer
            .map(|(id, _)| id),
        Some("bench-printer".to_owned())
    );
    // Older files have none.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(old.print_profiles().is_empty());

    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff
        .print_profiles
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(texts, ["Bench printer (bench-printer): added"]);

    // Broken entries are refused with where they are.
    for (broken, expected) in [
        (
            json!([{"id": "a", "name": "A", "kind": "printer"},
                   {"id": "a", "name": "B", "kind": "process"}]),
            "print_profiles[1]: profile a is listed more than once",
        ),
        (
            json!([{"id": "a", "name": "A", "kind": "process", "values": {"bed_shape": 1}}]),
            "print_profiles[0]: profile a:",
        ),
    ] {
        let mut file: Value = serde_json::from_str(&plain).unwrap();
        file["print_profiles"] = broken;
        let error = Document::from_json(&file.to_string(), MockKernel::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn the_design_s_process_lists_what_overrides_it() {
    // mitcad#655: the chosen process's row names the owners below the root
    // that set its values, and the keys they set.
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process", "name": "Fine"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "profile", "value": "fine"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "layer_height", "value": 0.2}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "F2.b0", "set": "Print Process",
               "key": "perimeters", "value": 5}),
    );
    let rows = query(&doc, json!({"query": "library_items", "kind": "process"}));
    let fine = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "fine")
        .cloned()
        .unwrap();
    assert_eq!(fine["chosen"], true);
    let overrides = fine["overrides"].as_array().unwrap();
    assert_eq!(overrides.len(), 1, "{overrides:?}");
    assert_eq!(overrides[0]["owner"], "F2.b0");
    assert_eq!(overrides[0]["keys"], json!(["perimeters"]));
    assert_eq!(overrides[0]["values"]["perimeters"].as_f64(), Some(5.0));
    assert!(overrides[0]["name"].is_string());
    // The same owners in the answer itself (mitcad#747).
    assert_eq!(rows["print_overrides"], fine["overrides"]);
}

#[test]
fn own_process_values_are_listed_with_slicer_default() {
    // mitcad#747: with no process chosen (Slicer default) no row carries
    // `overrides`, but a body's own process values are still exported, so
    // the answer lists their owners itself.
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "F2.b0", "set": "Print Process",
               "key": "perimeters", "value": 4}),
    );
    let rows = query(&doc, json!({"query": "library_items", "kind": "process"}));
    assert!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["chosen"] == false && r.get("overrides").is_none())
    );
    let owners = rows["print_overrides"].as_array().unwrap();
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert_eq!(owners[0]["owner"], "F2.b0");
    assert_eq!(owners[0]["keys"], json!(["perimeters"]));
    assert_eq!(owners[0]["values"]["perimeters"].as_f64(), Some(4.0));
    // The printer's set has none; a material's answer no such list.
    let printers = query(&doc, json!({"query": "library_items", "kind": "printer"}));
    assert_eq!(printers["print_overrides"], json!([]));
    let materials = query(&doc, json!({"query": "library_items", "kind": "material"}));
    assert!(materials.get("print_overrides").is_none());
}

#[test]
fn new_items_are_named_alike() {
    // mitcad#655, mitcad#828: "Printer 1", "Process 1", "Material 1", then
    // "… 2"; never "New …", which read as 3D Print's New process… action.
    let mut doc = block();
    for _ in 0..2 {
        command(
            &mut doc,
            json!({"cmd": "create_print_profile", "kind": "printer"}),
        );
    }
    let names: Vec<String> = doc
        .print_profiles()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names, ["Printer 1", "Printer 2"]);
    command(&mut doc, json!({"cmd": "create_material"}));
    assert_eq!(doc.custom_materials()[0].name, "Material 1");
    // A copy is named after its source, " (copy)" (mitcad#828).
    command(
        &mut doc,
        json!({"cmd": "create_material", "based_on": "custom1"}),
    );
    assert_eq!(doc.custom_materials()[1].name, "Material 1 (copy)");
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "based_on": "printer1"}),
    );
    assert_eq!(doc.print_profiles()[2].name, "Printer 1 (copy)");
    // The next copy " (copy 2)", also of the copy, never "(copy) (copy)".
    let copy = doc.print_profiles()[2].id.clone();
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "based_on": copy}),
    );
    assert_eq!(doc.print_profiles()[3].name, "Printer 1 (copy 2)");
    command(
        &mut doc,
        json!({"cmd": "create_material", "based_on": "custom1"}),
    );
    assert_eq!(doc.custom_materials()[2].name, "Material 1 (copy 2)");
    // A new process made the design's choice in the same undo step: 3D
    // Print's New process… twice gives Process 1 and Process 2.
    let first = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process", "choose": true}),
    );
    let first_name = doc
        .print_profile(first["id"].as_str().unwrap())
        .unwrap()
        .name
        .clone();
    assert_eq!(first_name, "Process 1");
    let depth = doc.undo_depth();
    let made = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process", "choose": true}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    let made_name = doc
        .print_profile(made["id"].as_str().unwrap())
        .unwrap()
        .name
        .clone();
    assert_eq!(made_name, "Process 2");
    let rows = query(&doc, json!({"query": "library_items", "kind": "process"}));
    let row = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == made["id"])
        .cloned()
        .unwrap();
    assert_eq!(row["chosen"], true);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(doc.print_profile(made["id"].as_str().unwrap()).is_none());
}
