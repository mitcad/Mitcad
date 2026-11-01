// SPDX-License-Identifier: MIT
//! An installed slicer's user presets (mitcad#830) with the mock kernel: a
//! synthetic Bambu Studio data folder (made by the test, never a real
//! slicer's) with a printer, a process and a filament preset of the user,
//! the printer and the filament inheriting from synthetic system presets.
//! `slicer_presets` lists them with the item of the same kind and name
//! the target has; `import_slicer_presets` brings the ticked ones in with
//! their inherited values, a same-name one as an override (the existing
//! item's next version) or a copy (`<name> 2`), into a library or into
//! the design in one undo step.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::print_library::{
    ItemData, ItemKind, Libraries, LibraryKind, item_path, project_library,
};
use mitcad_model::print_profile::{PrintProfile, ProfileKind};
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

/// The test's own folder and a Bambu Studio data folder in it: system
/// presets of a vendor Acme (a printer two levels deep, a PLA) and the
/// user's My X (inherits the Acme printer), Fine (a process of its own)
/// and My PLA (inherits Acme PLA).
fn slicer_folder(name: &str) -> (PathBuf, PathBuf) {
    let dir = mitcad_model::testing::unique_temp_path(&format!("presets-{name}"));
    let slicer = dir.join("BambuStudio");
    let system = slicer.join("system").join("Acme");
    write(
        &system.join("machine").join("fdm_machine_common.json"),
        &json!({"type": "machine", "name": "fdm_machine_common", "instantiation": "false",
                "nozzle_diameter": ["0.4"], "printable_height": "250"}),
    );
    write(
        &system.join("machine").join("Acme X 0.4 nozzle.json"),
        &json!({"type": "machine", "name": "Acme X 0.4 nozzle", "inherits": "fdm_machine_common",
                "printer_model": "Acme X",
                "printable_area": ["0x0", "256x0", "256x256", "0x256"]}),
    );
    write(
        &system.join("filament").join("Acme PLA.json"),
        &json!({"type": "filament", "name": "Acme PLA", "filament_type": ["PLA"],
                "filament_density": ["1.24"], "nozzle_temperature": ["210"]}),
    );
    let user = slicer.join("user").join("default");
    write(
        &user.join("machine").join("My X.json"),
        &json!({"name": "My X", "inherits": "Acme X 0.4 nozzle", "from": "User",
                "printable_height": "240"}),
    );
    write(
        &user.join("process").join("Fine.json"),
        &json!({"name": "Fine", "from": "User", "layer_height": "0.12"}),
    );
    write(
        &user.join("filament").join("My PLA.json"),
        &json!({"name": "My PLA", "inherits": "Acme PLA", "from": "User",
                "nozzle_temperature": ["215"]}),
    );
    (dir, slicer)
}

/// The presets `slicer_presets` finds in `slicer` for the target.
fn presets(doc: &Document<MockKernel>, slicer: &Path, into: &str, project: &Path) -> Vec<Value> {
    let found = query(
        doc,
        json!({"query": "slicer_presets", "into": into, "project": project,
               "folders": [{"dialect": "bambu_studio", "path": slicer}]}),
    );
    let slicers = found["slicers"].as_array().unwrap();
    assert_eq!(slicers.len(), 1);
    assert_eq!(slicers[0]["dialect"], "bambu_studio");
    assert_eq!(slicers[0]["name"], "Bambu Studio");
    slicers[0]["presets"].as_array().unwrap().clone()
}

fn import(
    doc: &mut Document<MockKernel>,
    slicer: &Path,
    into: &str,
    project: &Path,
    presets: Value,
) -> Value {
    command(
        doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "into": into, "project": project, "presets": presets}),
    )
}

/// The project library's item of a kind and name.
fn library_item(project: &Path, kind: ItemKind, name: &str) -> ItemData {
    Libraries::load(Vec::new(), None, Some(project))
        .items
        .into_iter()
        .find(|i| {
            i.library == LibraryKind::Project && i.data.kind() == kind && i.data.name() == name
        })
        .unwrap_or_else(|| panic!("the project's library has no {} {name}", kind.id()))
        .data
}

#[test]
fn installed_presets_are_listed_marked_and_imported_into_a_library() {
    let (dir, slicer) = slicer_folder("library");
    let project = dir.join("project");
    // The project's library has a process named Fine.
    let mut fine = PrintProfile::new("fine", "Fine", ProfileKind::Process);
    fine.values.insert("layer_height".to_owned(), json!(0.3));
    fine.version = Some(1);
    let fine = ItemData::Profile(fine);
    let file = project_library(&project).join(item_path(fine.kind(), fine.id()));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, fine.text()).unwrap();

    let mut doc = Document::new(MockKernel::default());
    let found = presets(&doc, &slicer, "project", &project);
    let listed: Vec<(&str, &str)> = found
        .iter()
        .map(|p| (p["kind"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    assert_eq!(
        listed,
        vec![
            ("printer", "My X"),
            ("process", "Fine"),
            ("material", "My PLA")
        ]
    );
    assert_eq!(found[0]["inherits"], "Acme X 0.4 nozzle");
    assert_eq!(found[0]["account"], "default");
    // Only Fine's name is the library's.
    assert!(found[0]["same"].is_null() && found[2]["same"].is_null());
    assert_eq!(
        found[1]["same"],
        json!({"id": "fine", "name": "Fine", "place": "project"})
    );

    // All three ticked, Fine overriding the library's.
    let result = import(
        &mut doc,
        &slicer,
        "project",
        &project,
        json!([{"path": found[0]["path"]},
               {"path": found[1]["path"], "same_name": "override"},
               {"path": found[2]["path"]}]),
    );
    assert_eq!(result["into"], "project");
    assert_eq!(result["overridden"][0]["id"], "fine");
    assert_eq!(result["overridden"][0]["version"], 2);
    assert_eq!(result["copied"], json!([]));
    assert_eq!(result["items"].as_array().unwrap().len(), 3);
    // Override: the existing item's next version with the preset's values.
    let ItemData::Profile(process) = library_item(&project, ItemKind::Process, "Fine") else {
        panic!("a process");
    };
    assert_eq!(process.version, Some(2));
    assert_eq!(process.values["layer_height"], 0.12);
    // The printer with what it inherits two levels up filled in.
    let ItemData::Profile(printer) = library_item(&project, ItemKind::Printer, "My X") else {
        panic!("a printer");
    };
    assert_eq!(printer.values["max_print_height"], 240.0, "its own value");
    assert_eq!(
        printer.values["bed_shape"],
        json!([[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]]),
        "from its system preset"
    );
    assert_eq!(
        printer.values["nozzle_diameter"], 0.4,
        "from that one's base"
    );
    // The filament as a material: its own temperature, the density and
    // type of the system preset.
    let ItemData::Material { material, .. } = library_item(&project, ItemKind::Material, "My PLA")
    else {
        panic!("a material");
    };
    assert_eq!(material.filament["temperature"], 215.0);
    assert_eq!(material.density, 1.24);
    let report = &result["report"];
    assert!(
        !report["left_out"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("system preset")),
        "nothing left to the system presets: {report}"
    );

    // Now every name is the library's; Fine again as a copy.
    let found = presets(&doc, &slicer, "project", &project);
    assert!(found.iter().all(|p| p["same"]["place"] == "project"));
    let result = import(
        &mut doc,
        &slicer,
        "project",
        &project,
        json!([{"path": found[1]["path"]}]),
    );
    assert_eq!(
        result["copied"],
        json!([{"kind": "process", "name": "Fine 2", "from": "Fine"}])
    );
    assert_eq!(result["items"][0]["name"], "Fine 2");
    let ItemData::Profile(copy) = library_item(&project, ItemKind::Process, "Fine 2") else {
        panic!("a process");
    };
    assert_eq!(copy.version, Some(1));
    assert_ne!(
        copy.id, "fine",
        "a copy never takes the existing item's file"
    );
    let ItemData::Profile(kept) = library_item(&project, ItemKind::Process, "Fine") else {
        panic!("a process");
    };
    assert_eq!(kept.version, Some(2), "the existing item stays as it was");
}

#[test]
fn installed_presets_import_into_the_design_in_one_undo_step() {
    let (dir, slicer) = slicer_folder("design");
    let project = dir.join("project");
    let mut doc = Document::new(MockKernel::default());
    let found = presets(&doc, &slicer, "design", &project);
    assert!(found.iter().all(|p| p["same"].is_null()));
    let all: Vec<Value> = found.iter().map(|p| json!({"path": p["path"]})).collect();
    let result = import(&mut doc, &slicer, "design", &project, json!(all));
    assert_eq!(result["into"], "design");
    assert_eq!(result["printers"][0]["name"], "My X");
    assert_eq!(result["processes"][0]["name"], "Fine");
    assert_eq!(result["materials"][0]["name"], "My PLA");
    assert_eq!(doc.undo_label(), Some("Import Presets of Bambu Studio"));
    let fine = result["processes"][0]["id"].as_str().unwrap().to_owned();
    let printer = result["printers"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        doc.print_profile(&printer).unwrap().values["nozzle_diameter"],
        0.4,
        "inherited values filled in"
    );

    // The design's own now: marked, against the design.
    let found = presets(&doc, &slicer, "design", &project);
    assert_eq!(found[1]["same"]["place"], "design");
    assert_eq!(found[1]["same"]["id"], fine.as_str());

    // The design's Fine changed; then Fine overriding it and My X as a
    // copy, in one undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": fine, "values": {"layer_height": 0.3}}),
    );
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        json!([{"path": found[1]["path"], "same_name": "override"},
               {"path": found[0]["path"]}]),
    );
    assert_eq!(result["overridden"][0]["id"], fine.as_str());
    assert_eq!(
        result["copied"],
        json!([{"kind": "printer", "name": "My X 2", "from": "My X"}])
    );
    assert_eq!(
        doc.print_profile(&fine).unwrap().values["layer_height"],
        0.12
    );
    let names = |doc: &Document<MockKernel>| -> Vec<String> {
        doc.print_profiles()
            .iter()
            .map(|p| p.name.clone())
            .collect()
    };
    assert!(names(&doc).contains(&"My X 2".to_owned()));
    assert_eq!(
        doc.undo().as_deref(),
        Some("Import Presets of Bambu Studio")
    );
    assert_eq!(
        doc.print_profile(&fine).unwrap().values["layer_height"],
        0.3,
        "one undo takes the override back"
    );
    assert!(!names(&doc).contains(&"My X 2".to_owned()), "and the copy");
}

/// The slicer's own presets the user enabled (mitcad#871): the synthetic
/// folder's system presets get bases and presets meant to be chosen, a
/// printer not enabled with its process and filament, and an app config
/// naming the enabled printer model and filaments (a base among them).
#[test]
fn enabled_system_presets_are_listed_after_the_user_s_and_import_resolved() {
    let (dir, slicer) = slicer_folder("system");
    let project = dir.join("project");
    let system = slicer.join("system").join("Acme");
    write(
        &system.join("machine").join("fdm_machine_common.json"),
        &json!({"type": "machine", "name": "fdm_machine_common", "instantiation": "false",
                "printer_variant": "0.4", "nozzle_diameter": ["0.4"],
                "printable_height": "250"}),
    );
    write(
        &system.join("machine").join("Acme X 0.4 nozzle.json"),
        &json!({"type": "machine", "name": "Acme X 0.4 nozzle", "instantiation": "true",
                "inherits": "fdm_machine_common", "printer_model": "Acme X",
                "printable_area": ["0x0", "256x0", "256x256", "0x256"]}),
    );
    write(
        &system.join("machine").join("Acme Y 0.4 nozzle.json"),
        &json!({"type": "machine", "name": "Acme Y 0.4 nozzle", "instantiation": "true",
                "inherits": "fdm_machine_common", "printer_model": "Acme Y"}),
    );
    write(
        &system.join("process").join("fdm_process_common.json"),
        &json!({"type": "process", "name": "fdm_process_common", "instantiation": "false",
                "layer_height": "0.2", "wall_loops": "2"}),
    );
    write(
        &system.join("process").join("0.12mm Fine @Acme X.json"),
        &json!({"type": "process", "name": "0.12mm Fine @Acme X", "instantiation": "true",
                "inherits": "fdm_process_common", "compatible_printers": ["Acme X 0.4 nozzle"],
                "layer_height": "0.12"}),
    );
    write(
        &system.join("process").join("0.20mm Standard @Acme Y.json"),
        &json!({"type": "process", "name": "0.20mm Standard @Acme Y", "instantiation": "true",
                "inherits": "fdm_process_common", "compatible_printers": ["Acme Y 0.4 nozzle"]}),
    );
    write(
        &system.join("filament").join("fdm_filament_common.json"),
        &json!({"type": "filament", "name": "fdm_filament_common", "instantiation": "false",
                "filament_density": ["1.24"]}),
    );
    write(
        &system.join("filament").join("Acme PETG @Y.json"),
        &json!({"type": "filament", "name": "Acme PETG @Y", "instantiation": "true",
                "inherits": "fdm_filament_common", "filament_type": ["PETG"],
                "compatible_printers": ["Acme Y 0.4 nozzle"]}),
    );
    std::fs::write(
        slicer.join("BambuStudio.conf"),
        r#"{
    "models": [{"vendor": "Acme", "model": "Acme X", "nozzle_diameter": "0.4"}],
    "filaments": ["Acme PLA", "Acme PETG @Y", "fdm_filament_common"]
}
# MD5 checksum 0123456789ABCDEF0123456789ABCDEF
"#,
    )
    .unwrap();

    let mut doc = Document::new(MockKernel::default());
    let found = presets(&doc, &slicer, "design", &project);
    let listed: Vec<(&str, &str, bool)> = found
        .iter()
        .map(|p| {
            (
                p["kind"].as_str().unwrap(),
                p["name"].as_str().unwrap(),
                p["system"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            ("printer", "My X", false),
            ("process", "Fine", false),
            ("material", "My PLA", false),
            ("printer", "Acme X 0.4 nozzle", true),
            ("process", "0.12mm Fine @Acme X", true),
            ("material", "Acme PLA", true),
        ],
        "the user's, then exactly the enabled system ones"
    );
    assert_eq!(found[0]["account"], "default");
    assert!(found[0]["vendor"].is_null());
    assert!(found[4]["account"].is_null());
    assert_eq!(found[4]["vendor"], "Acme");
    assert_eq!(found[4]["inherits"], "fdm_process_common");
    assert!(found[4]["same"].is_null());

    // A system process imported: its values and its base's, its name kept.
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        json!([{"path": found[4]["path"]}]),
    );
    assert_eq!(result["processes"][0]["name"], "0.12mm Fine @Acme X");
    let id = result["processes"][0]["id"].as_str().unwrap().to_owned();
    let process = doc.print_profile(&id).unwrap();
    assert_eq!(process.values["layer_height"], 0.12, "its own value");
    assert_eq!(process.values["perimeters"], 2, "from its base");
    // A system printer: the variant's base filled in.
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        json!([{"path": found[3]["path"]}]),
    );
    let printer = result["printers"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        doc.print_profile(&printer).unwrap().values["max_print_height"],
        250.0
    );

    // Now the design's: marked, and the name rule applies.
    let found = presets(&doc, &slicer, "design", &project);
    assert_eq!(found[4]["same"]["id"], id.as_str());
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": id, "values": {"layer_height": 0.3}}),
    );
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        json!([{"path": found[4]["path"], "same_name": "override"}]),
    );
    assert_eq!(result["overridden"][0]["id"], id.as_str());
    assert_eq!(doc.print_profile(&id).unwrap().values["layer_height"], 0.12);
}
