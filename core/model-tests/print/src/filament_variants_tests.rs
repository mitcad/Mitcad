// SPDX-License-Identifier: MIT
//! One filament, its variants per printer and nozzle (mitcad#1054), with
//! the mock kernel and synthetic slicer folders the tests write (never a
//! real slicer's):
//!
//! - a material with a base and two variants gives each printer and
//!   nozzle its values, and an unknown printer the base;
//! - three presets of one filament for two printers come in as one
//!   material, the values they share as its base, each printer and nozzle
//!   chosen a variant;
//! - the same design sent with one printer and then with another writes
//!   each variant's values into the 3MF;
//! - a library's separate materials of one filament, kept from before,
//!   are offered for a merge and merged by the import; an item file from
//!   before variants reads as it was.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::print_library::{ItemData, ItemKind, Libraries, LibraryKind};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

use super::print_files_tests::{command, export_to, small_block};

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value.to_string()).unwrap();
}

/// A design of one block of Shop PLA (210 °C, bed 60 °C) with a variant
/// for Bench X with a 0.4 mm nozzle (220 °C) and one for Bench Y with a
/// 0.4 mm nozzle (205 °C, bed 55 °C), and printers: Bench X with a 0.4
/// and a 0.6 mm nozzle, Bench Y with 0.4 mm, and another one.
fn design() -> (Document<MockKernel>, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let body = small_block(&mut doc);
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "id": "x4",
             "name": "Bench X 0.4 nozzle",
             "values": {"nozzle_diameter": 0.4, "printer_model": {"bambu_studio": "Bench X"},
                        "printer_settings_id": {"bambu_studio": "Bench X 0.4 nozzle"}}},
            {"cmd": "create_print_profile", "kind": "printer", "id": "x6",
             "name": "Bench X 0.6 nozzle",
             "values": {"nozzle_diameter": 0.6, "printer_model": {"bambu_studio": "Bench X"}}},
            {"cmd": "create_print_profile", "kind": "printer", "id": "y4",
             "name": "Bench Y 0.4 nozzle", "values": {"nozzle_diameter": 0.4}},
            {"cmd": "create_print_profile", "kind": "printer", "id": "other",
             "name": "Other printer", "values": {"nozzle_diameter": 0.4}},
            {"cmd": "create_material", "id": "shop_pla", "name": "Shop PLA", "density": 1.24,
             "class": "plastic",
             "filament": {"filament_type": "PLA", "temperature": 210, "bed_temperature": 60}},
            {"cmd": "edit_material", "id": "shop_pla",
             "variant": {"printer": "Bench X", "nozzle": 0.4, "filament": {"temperature": 220}}},
            {"cmd": "edit_material", "id": "shop_pla",
             "variant": {"printer": "Bench Y", "nozzle": 0.4,
                         "filament": {"temperature": 205, "bed_temperature": 55}}},
            {"cmd": "set_body_material", "uid": body, "material": "shop_pla"},
        ]),
    );
    (doc, body)
}

/// The root chooses a printer profile.
fn use_printer(doc: &mut Document<MockKernel>, id: &str) {
    command(
        doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
               "value": id}),
    );
}

/// What the body prints with: its temperature, bed temperature and the
/// variant's name (null: the base).
fn printed(doc: &Document<MockKernel>, body: BodyUid) -> (Value, Value, Value) {
    let settings = query(doc, json!({"query": "print_settings", "body": body}));
    let filament = &settings["filament"];
    let value = |key: &str| {
        filament["values"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["key"] == key)
            .map(|v| v["value"].clone())
            .unwrap_or(Value::Null)
    };
    (
        value("temperature"),
        value("bed_temperature"),
        filament["variant"]["label"].clone(),
    )
}

#[test]
fn a_material_s_variants_resolve_per_printer_and_nozzle() {
    let (mut doc, body) = design();
    let material = doc.material("shop_pla").unwrap();
    assert_eq!(material.variants.len(), 2);
    // Bench X with its 0.4 nozzle: its variant over the base.
    use_printer(&mut doc, "x4");
    assert_eq!(
        printed(&doc, body),
        (json!(220), json!(60), json!("Bench X, 0.4 mm nozzle"))
    );
    // Bench X with a 0.6 nozzle has no variant of its own: the printer's.
    use_printer(&mut doc, "x6");
    assert_eq!(
        printed(&doc, body),
        (json!(220), json!(60), json!("Bench X, 0.4 mm nozzle"))
    );
    let settings = query(&doc, json!({"query": "print_settings", "body": body}));
    assert_eq!(settings["filament"]["variant"]["match"], "printer");
    assert_eq!(settings["filament"]["variants"], 2);
    // Bench Y.
    use_printer(&mut doc, "y4");
    assert_eq!(
        printed(&doc, body),
        (json!(205), json!(55), json!("Bench Y, 0.4 mm nozzle"))
    );
    let settings = query(&doc, json!({"query": "print_settings", "body": body}));
    assert_eq!(settings["filament"]["variant"]["match"], "exact");
    // The values the variant gives are marked with it.
    let marked: Vec<&str> = settings["filament"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["variant"].is_string())
        .map(|v| v["key"].as_str().unwrap())
        .collect();
    assert_eq!(marked, ["temperature", "bed_temperature"]);
    // The body's material names its variant too.
    let material = query(&doc, json!({"query": "body_material", "body": body}));
    assert_eq!(material["variant"]["label"], "Bench Y, 0.4 mm nozzle");
    // An unknown printer, and none: the base.
    use_printer(&mut doc, "other");
    assert_eq!(printed(&doc, body), (json!(210), json!(60), Value::Null));
    command(
        &mut doc,
        json!({"cmd": "remove_property", "owner": "C0", "set": "Print Printer", "key": "profile"}),
    );
    assert_eq!(printed(&doc, body), (json!(210), json!(60), Value::Null));
    // A variant changed, then removed, each one undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "shop_pla",
               "variant": {"printer": "Bench Y", "nozzle": 0.4, "filament": {"temperature": 207}}}),
    );
    use_printer(&mut doc, "y4");
    assert_eq!(printed(&doc, body).0, json!(207));
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "shop_pla",
               "variant": {"printer": "Bench Y", "nozzle": 0.4, "remove": true}}),
    );
    assert_eq!(printed(&doc, body), (json!(210), json!(60), Value::Null));
    // A colour per variant is refused: a colour of its own is a material.
    let refused = doc
        .command(
            &json!({"cmd": "edit_material", "id": "shop_pla",
                    "variant": {"printer": "Bench X", "nozzle": 0.4,
                                "filament": {"filament_colour": "#000000"}}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("colour"), "{refused}");
}

#[test]
fn variants_survive_the_design_file_and_old_files_load_unchanged() {
    let (doc, _) = design();
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let stored = file["materials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "shop_pla")
        .unwrap()
        .clone();
    assert_eq!(stored["variants"][0]["printer"], "Bench X");
    assert_eq!(stored["variants"][0]["nozzle"], 0.4);
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.material("shop_pla"), doc.material("shop_pla"));
    assert_eq!(loaded.to_json(), saved);
    // A design from before variants: its materials have none, and it is
    // written as it was read.
    let mut old = file.clone();
    for material in old["materials"].as_array_mut().unwrap() {
        if let Value::Object(fields) = material {
            fields.remove("variants");
        }
    }
    let old_text = old.to_string();
    let loaded = Document::from_json(&old_text, MockKernel::default()).unwrap();
    assert!(loaded.material("shop_pla").unwrap().variants.is_empty());
    let resaved: Value = serde_json::from_str(&loaded.to_json()).unwrap();
    assert_eq!(resaved["materials"], old["materials"]);
}

#[test]
fn the_same_design_sent_to_two_printers_writes_each_variant() {
    let (mut doc, _) = design();
    let sent = |doc: &mut Document<MockKernel>, name: &str| {
        let (_, data) = export_to(doc, name, "bambu_studio");
        let package = mitcad_3mf::read_package(&data).unwrap();
        let settings: Map<String, Value> =
            serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap())
                .unwrap();
        (
            settings["nozzle_temperature"].clone(),
            settings["printer_settings_id"].clone(),
        )
    };
    use_printer(&mut doc, "x4");
    assert_eq!(
        sent(&mut doc, "bench-x.3mf"),
        (json!(["220"]), json!("Bench X 0.4 nozzle"))
    );
    use_printer(&mut doc, "y4");
    assert_eq!(sent(&mut doc, "bench-y.3mf").0, json!(["205"]));
    use_printer(&mut doc, "other");
    assert_eq!(sent(&mut doc, "other.3mf").0, json!(["210"]));
}

/// A Bambu Studio data folder in the test's own folder with the user's
/// three presets of Shop PLA, for Bench X with a 0.4 and a 0.6 mm nozzle
/// and for Bench Y (each its temperature, the type and the speed shared),
/// and one of Shop PETG.
fn slicer_folder(name: &str) -> (PathBuf, PathBuf) {
    let dir = mitcad_model::testing::unique_temp_path(&format!("filament-variants-{name}"));
    let slicer = dir.join("BambuStudio");
    let filament = slicer.join("user").join("default").join("filament");
    for (preset, temperature, printer) in [
        ("Shop PLA @Bench X", "220", "Bench X 0.4 nozzle"),
        ("Shop PLA @Bench X 0.6 nozzle", "230", "Bench X 0.6 nozzle"),
        ("Shop PLA @Bench Y", "205", "Bench Y 0.4 nozzle"),
    ] {
        write(
            &filament.join(format!("{preset}.json")),
            &json!({"name": preset, "from": "User", "filament_type": ["PLA"],
                    "filament_density": ["1.24"], "filament_max_volumetric_speed": ["21"],
                    "nozzle_temperature": [temperature],
                    "compatible_printers": [printer]}),
        );
    }
    write(
        &filament.join("Shop PETG.json"),
        &json!({"name": "Shop PETG", "from": "User", "filament_type": ["PETG"],
                "nozzle_temperature": ["240"]}),
    );
    (dir, slicer)
}

/// The project library's materials.
fn library_materials(project: &Path) -> Vec<mitcad_model::material::Material> {
    Libraries::load(Vec::new(), None, Some(project))
        .items
        .into_iter()
        .filter(|i| i.library == LibraryKind::Project && i.data.kind() == ItemKind::Material)
        .map(|i| match i.data {
            ItemData::Material { material, .. } => material,
            ItemData::Profile(_) => unreachable!(),
        })
        .collect()
}

#[test]
fn three_presets_of_one_filament_import_as_one_material_with_variants() {
    let (dir, slicer) = slicer_folder("import");
    let project = dir.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let found = query(
        &doc,
        json!({"query": "slicer_presets", "into": "design",
               "folders": [{"dialect": "bambu_studio", "path": slicer}]}),
    );
    let listed = &found["slicers"][0];
    // Each filament preset names its filament and its printer and nozzle;
    // the slicer the printers and nozzles of them all.
    let presets = listed["presets"].as_array().unwrap();
    let x6 = presets
        .iter()
        .find(|p| p["name"] == "Shop PLA @Bench X 0.6 nozzle")
        .unwrap();
    assert_eq!(x6["filament"], "Shop PLA");
    assert_eq!(
        x6["printers"],
        json!([{"printer": "Bench X", "nozzle": 0.6}])
    );
    // In the presets' order: by their files' names.
    assert_eq!(
        listed["printers"],
        json!([
            {"printer": "Bench X", "nozzle": 0.6},
            {"printer": "Bench X", "nozzle": 0.4},
            {"printer": "Bench Y", "nozzle": 0.4}
        ])
    );
    assert_eq!(found["merge"], json!([]));
    let paths: Vec<Value> = presets.iter().map(|p| json!({"path": p["path"]})).collect();

    // The printers in use: Bench X with 0.4 and Bench Y. The 0.6 preset
    // stays out; Shop PLA is one material of two variants, the values all
    // three share its own.
    let result = command(
        &mut doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "presets": paths,
               "printers": [{"printer": "Bench X", "nozzle": 0.4},
                            {"printer": "Bench Y", "nozzle": 0.4}]}),
    );
    let names: Vec<&str> = result["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Shop PETG", "Shop PLA"]);
    let id = result["materials"][1]["id"].as_str().unwrap();
    let shop = doc.material(id).unwrap();
    assert_eq!(shop.filament["filament_type"], "PLA");
    assert_eq!(shop.filament["filament_max_volumetric_speed"], 21.0);
    assert!(!shop.filament.contains_key("temperature"), "not shared");
    let variants: Vec<(String, Value)> = shop
        .variants
        .iter()
        .map(|v| (v.label(), v.filament["temperature"].clone()))
        .collect();
    assert_eq!(
        variants,
        [
            ("Bench X, 0.4 mm nozzle".to_owned(), json!(220.0)),
            ("Bench Y, 0.4 mm nozzle".to_owned(), json!(205.0)),
        ]
    );
    // Each variant keeps its own preset's name for the slicer.
    assert_eq!(
        shop.variants[0].filament["filament_settings_id"],
        json!({"bambu_studio": "Shop PLA @Bench X"})
    );
    assert_eq!(doc.undo_depth(), 1, "one undo step");

    // All three into the project's library: three variants.
    command(
        &mut doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "into": "project", "project": project, "presets": paths}),
    );
    let materials = library_materials(&project);
    let shop = materials.iter().find(|m| m.name == "Shop PLA").unwrap();
    let labels: Vec<String> = shop.variants.iter().map(|v| v.label()).collect();
    assert_eq!(
        labels,
        [
            "Bench X, 0.6 mm nozzle",
            "Bench X, 0.4 mm nozzle",
            "Bench Y, 0.4 mm nozzle"
        ]
    );
    assert_eq!(materials.len(), 2);
}

#[test]
fn separate_materials_of_a_library_are_offered_and_merged() {
    let (dir, slicer) = slicer_folder("merge");
    let project = dir.join("project");
    let folder = project.join("print-library").join("materials");
    // Two materials of Shop PLA kept apart by an import from before
    // variants: item files of that time (no variants), and a design copy.
    for (id, name, temperature, printer) in [
        (
            "shop-pla-bench-x",
            "Shop PLA @Bench X",
            220,
            "Bench X 0.4 nozzle",
        ),
        (
            "shop-pla-bench-y",
            "Shop PLA @Bench Y",
            205,
            "Bench Y 0.4 nozzle",
        ),
    ] {
        write(
            &folder.join(format!("{id}.json")),
            &json!({"format": "mitcad-print-item", "version": 1,
                    "material": {"id": id, "name": name, "density": 1.24, "version": 1,
                                 "filament": {"filament_type": "PLA", "temperature": temperature},
                                 "slicer": {"bambu_studio": {
                                     "compatible_printers": format!("[\"{printer}\"]")}}}}),
        );
    }
    let materials = library_materials(&project);
    assert_eq!(materials.len(), 2);
    assert!(
        materials.iter().all(|m| m.variants.is_empty()),
        "item files from before read as they were"
    );
    let mut doc = Document::new(MockKernel::default());
    let found = query(
        &doc,
        json!({"query": "slicer_presets", "into": "project", "project": project,
               "folders": [{"dialect": "bambu_studio", "path": slicer}]}),
    );
    assert_eq!(
        found["merge"],
        json!([{"name": "Shop PLA", "items": [
            {"id": "shop-pla-bench-x", "name": "Shop PLA @Bench X"},
            {"id": "shop-pla-bench-y", "name": "Shop PLA @Bench Y"}]}])
    );
    // The import merges them first: one item with the two variants, the
    // second's file gone; nothing else imported.
    let result = command(
        &mut doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "into": "project", "project": project, "presets": [],
               "merge_separate": true}),
    );
    assert_eq!(result["merged"][0]["id"], "shop-pla-bench-x");
    assert_eq!(result["merged"][0]["name"], "Shop PLA");
    assert!(!folder.join("shop-pla-bench-y.json").exists());
    let materials = library_materials(&project);
    assert_eq!(materials.len(), 1);
    let shop = &materials[0];
    assert_eq!(shop.name, "Shop PLA");
    assert_eq!(shop.version, Some(2));
    assert_eq!(shop.filament["filament_type"], "PLA");
    assert!(!shop.filament.contains_key("temperature"));
    let labels: Vec<String> = shop.variants.iter().map(|v| v.label()).collect();
    assert_eq!(labels, ["Bench X, 0.4 mm nozzle", "Bench Y, 0.4 mm nozzle"]);
    // Nothing left to merge.
    let found = query(
        &doc,
        json!({"query": "slicer_presets", "into": "project", "project": project,
               "folders": [{"dialect": "bambu_studio", "path": slicer}]}),
    );
    assert_eq!(found["merge"], json!([]));
}

/// Two library items of one filament, each carrying an appearance of its
/// own inline (mitcad#1150): the merged item offers and carries both, so
/// the second's is not lost with its file.
#[test]
fn a_library_merge_keeps_every_item_s_appearances() {
    use mitcad_model::appearance::Appearance;
    let project = mitcad_model::testing::unique_temp_path("filament-variants-merge-looks");
    let folder = project.join("print-library").join("materials");
    for (id, name, temperature, printer, look) in [
        (
            "shop-pla-bench-x",
            "Shop PLA @Bench X",
            220,
            "Bench X 0.4 nozzle",
            "shop_red",
        ),
        (
            "shop-pla-bench-y",
            "Shop PLA @Bench Y",
            205,
            "Bench Y 0.4 nozzle",
            "shop_blue",
        ),
    ] {
        let material: mitcad_model::material::Material = serde_json::from_value(
            json!({"id": id, "name": name, "density": 1.24, "version": 1, "appearance": look,
                   "filament": {"filament_type": "PLA", "temperature": temperature},
                   "slicer": {"bambu_studio": {
                       "compatible_printers": format!("[\"{printer}\"]")}}}),
        )
        .unwrap();
        let item = ItemData::Material {
            material,
            appearances: vec![Appearance {
                id: look.to_owned(),
                name: look.to_owned(),
                ..Appearance::default()
            }],
        };
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{id}.json")), item.text()).unwrap();
    }
    let mut doc = Document::new(MockKernel::default());
    let libraries = Libraries::load(Vec::new(), None, Some(&project));
    let ids = ["shop-pla-bench-x".to_owned(), "shop-pla-bench-y".to_owned()];
    let id = doc
        .merge_library_materials(&libraries, LibraryKind::Project, &ids)
        .unwrap()
        .id;
    assert_eq!(id, "shop-pla-bench-x");
    assert!(!folder.join("shop-pla-bench-y.json").exists());
    let items: Vec<ItemData> = Libraries::load(Vec::new(), None, Some(&project))
        .items
        .into_iter()
        .filter(|i| i.library == LibraryKind::Project && i.data.kind() == ItemKind::Material)
        .map(|i| i.data)
        .collect();
    assert_eq!(items.len(), 1);
    let ItemData::Material {
        material,
        appearances,
    } = &items[0]
    else {
        unreachable!();
    };
    assert_eq!(material.appearance_list(), ["shop_red", "shop_blue"]);
    let carried: Vec<&str> = appearances.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(carried, ["shop_red", "shop_blue"]);
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn the_design_s_separate_materials_merge_and_its_bodies_follow() {
    let mut doc = Document::new(MockKernel::default());
    let body = small_block(&mut doc);
    command(
        &mut doc,
        json!([
            {"cmd": "create_material", "id": "pla_x", "name": "Shop PLA @Bench X", "density": 1.24,
             "class": "plastic", "filament": {"filament_type": "PLA", "temperature": 220}},
            {"cmd": "create_material", "id": "pla_y", "name": "Shop PLA @Bench Y", "density": 1.24,
             "class": "plastic", "filament": {"filament_type": "PLA", "temperature": 205}},
            {"cmd": "set_body_material", "uid": body, "material": "pla_y"},
            {"cmd": "set_component_material", "component": "C0", "material": "pla_y"},
        ]),
    );
    let depth = doc.undo_depth();
    let result = command(
        &mut doc,
        json!({"cmd": "merge_materials", "ids": ["pla_x", "pla_y"]}),
    );
    assert_eq!(result["id"], "pla_x");
    assert_eq!(doc.undo_depth(), depth + 1, "one undo step");
    assert!(doc.material("pla_y").is_none());
    let merged = doc.material("pla_x").unwrap();
    assert_eq!(merged.name, "Shop PLA");
    assert_eq!(merged.variants.len(), 2);
    assert_eq!(merged.variants[1].label(), "Bench Y, any nozzle");
    let material = query(&doc, json!({"query": "body_material", "body": body}));
    assert_eq!(material["id"], "pla_x", "the body names the merged one");
    assert_eq!(
        doc.component_material(mitcad_model::ids::ComponentUid::ROOT),
        Some("pla_x")
    );
    // Two materials for any printer are no one filament's presets.
    command(
        &mut doc,
        json!([
            {"cmd": "create_material", "id": "a", "name": "Plain", "density": 1.0,
             "class": "plastic", "filament": {"temperature": 200}},
            {"cmd": "create_material", "id": "b", "name": "Plain 2", "density": 1.0,
             "class": "plastic", "filament": {"temperature": 201}},
        ]),
    );
    assert!(
        doc.command(&json!({"cmd": "merge_materials", "ids": ["a", "b"]}).to_string())
            .is_err()
    );
}
