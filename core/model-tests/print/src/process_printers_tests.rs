// SPDX-License-Identifier: MIT
//! The printers a process is for (mitcad#1216) with the mock kernel, by the
//! filament presets' rule (mitcad#1054): its `compatible_printers`, else
//! those of the preset it inherits from (kept by the import of an installed
//! slicer's presets), else its name's `@` part, else none (any printer).
//! `library_items` gives a process row's `printers` and a printer row's
//! `printer_names`, which the Processes manager groups by and the process
//! pickers order by.

use std::path::Path;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value.to_string()).unwrap();
}

/// The `library_items` row of the design's item of a kind and name.
fn row(doc: &Document<MockKernel>, kind: &str, name: &str) -> Value {
    query(doc, json!({"query": "library_items", "kind": kind}))["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name && r["place"] == "design")
        .unwrap_or_else(|| panic!("no {kind} {name} in the design"))
        .clone()
}

/// The printers of the design's process `name`: `printer` and `nozzle`.
fn printers(doc: &Document<MockKernel>, name: &str) -> Vec<(String, Option<f64>)> {
    row(doc, "process", name)["printers"]
        .as_array()
        .unwrap_or_else(|| panic!("process {name} has no printers field"))
        .iter()
        .map(|p| {
            (
                p["printer"].as_str().unwrap().to_owned(),
                p["nozzle"].as_f64(),
            )
        })
        .collect()
}

#[test]
fn a_process_is_for_its_compatible_printers_else_its_name_s_else_any() {
    let mut doc = Document::new(MockKernel::default());
    for change in [
        // Its compatible printers, before its name's `@` part.
        json!({"cmd": "create_print_profile", "kind": "process", "id": "shared",
               "name": "0.20mm Shared @Bench Z",
               "slicer": {"bambu_studio": {"compatible_printers":
                   ["Bench X 0.4 nozzle", "Bench Y 0.4 nozzle"]}}}),
        // Its name's `@` part, the nozzle apart.
        json!({"cmd": "create_print_profile", "kind": "process", "id": "fine",
               "name": "0.12mm Fine @Bench X 0.2 nozzle"}),
        // An empty list names none: the name's.
        json!({"cmd": "create_print_profile", "kind": "process", "id": "optimal",
               "name": "0.16mm Optimal @Bench Y",
               "slicer": {"orcaslicer": {"compatible_printers": "[]"}}}),
        // Neither: any printer.
        json!({"cmd": "create_print_profile", "kind": "process", "id": "draft", "name": "Draft"}),
        json!({"cmd": "create_print_profile", "kind": "printer", "id": "x4",
               "name": "Bench X 0.4 nozzle", "values": {"nozzle_diameter": 0.4,
               "printer_model": {"bambu_studio": "Bench X"}}}),
    ] {
        command(&mut doc, change);
    }
    assert_eq!(
        printers(&doc, "0.20mm Shared @Bench Z"),
        vec![
            ("Bench X".to_owned(), Some(0.4)),
            ("Bench Y".to_owned(), Some(0.4))
        ],
        "compatible_printers"
    );
    assert_eq!(
        printers(&doc, "0.12mm Fine @Bench X 0.2 nozzle"),
        vec![("Bench X".to_owned(), Some(0.2))],
        "the @ part"
    );
    assert_eq!(
        printers(&doc, "0.16mm Optimal @Bench Y"),
        vec![("Bench Y".to_owned(), None)],
        "an empty list: the @ part"
    );
    assert!(printers(&doc, "Draft").is_empty(), "none: any printer");
    // A printer row: the names it goes by, its model first.
    let names = row(&doc, "printer", "Bench X 0.4 nozzle")["printer_names"].clone();
    let names: Vec<&str> = names
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert_eq!(names.first(), Some(&"Bench X"));
    assert!(names.contains(&"Bench X 0.4 nozzle"));
    // By place too (the managers' list model).
    let by_place = query(
        &doc,
        json!({"query": "library_items", "kind": "process", "by_place": true}),
    );
    let draft = by_place["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Draft")
        .unwrap();
    assert_eq!(draft["printers"], json!([]));
}

#[test]
fn an_imported_process_keeps_the_printers_of_the_preset_it_inherits_from() {
    // A synthetic Bambu Studio data folder (made by the test, never a real
    // slicer's): a system process for Acme Y, a user process inheriting it
    // with an empty list of its own, one inheriting it without a list, and
    // one of the user's own named for Acme X.
    let dir = mitcad_model::testing::unique_temp_path("process-printers");
    let slicer = dir.join("BambuStudio");
    let system = slicer.join("system").join("Acme").join("process");
    write(
        &system.join("fdm_process_common.json"),
        &json!({"type": "process", "name": "fdm_process_common", "instantiation": "false",
                "layer_height": "0.2"}),
    );
    write(
        &system.join("0.20mm Standard @Acme Y.json"),
        &json!({"type": "process", "name": "0.20mm Standard @Acme Y", "instantiation": "true",
                "inherits": "fdm_process_common",
                "compatible_printers": ["Acme Y 0.4 nozzle"]}),
    );
    let user = slicer.join("user").join("default").join("process");
    write(
        &user.join("My Standard.json"),
        &json!({"name": "My Standard", "inherits": "0.20mm Standard @Acme Y", "from": "User",
                "compatible_printers": [], "layer_height": "0.24"}),
    );
    write(
        &user.join("My Loose.json"),
        &json!({"name": "My Loose", "inherits": "0.20mm Standard @Acme Y", "from": "User",
                "layer_height": "0.28"}),
    );
    write(
        &user.join("Fine @Acme X.json"),
        &json!({"name": "Fine @Acme X", "from": "User", "layer_height": "0.12"}),
    );
    let mut doc = Document::new(MockKernel::default());
    let presets: Vec<Value> = ["My Standard", "My Loose", "Fine @Acme X"]
        .iter()
        .map(|name| json!({"path": user.join(format!("{name}.json"))}))
        .collect();
    let result = command(
        &mut doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "into": "design", "project": dir.join("project"), "presets": presets}),
    );
    assert_eq!(result["processes"].as_array().unwrap().len(), 3);
    let acme_y = vec![("Acme Y".to_owned(), Some(0.4))];
    assert_eq!(
        printers(&doc, "My Standard"),
        acme_y,
        "its own empty list hides not the inherited one"
    );
    assert_eq!(printers(&doc, "My Loose"), acme_y, "inherited");
    assert_eq!(
        printers(&doc, "Fine @Acme X"),
        vec![("Acme X".to_owned(), None)],
        "its name's"
    );
}
