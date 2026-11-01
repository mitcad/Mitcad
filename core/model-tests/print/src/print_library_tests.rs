// SPDX-License-Identifier: MIT
//! The libraries of materials and print profiles (mitcad#640, part 4)
//! with the mock kernel: the base library through a resolver, the user's
//! and the project's folders, the nearest one winning, the design's
//! complete copies with their origin, a library change that leaves the
//! design as it was until an update is asked for, and a design's item
//! saved into a library.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::appearance::Appearance;
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::material::{Material, MaterialClass};
use mitcad_model::print_library::{
    ItemData, ItemKind, Libraries, LibraryKind, item_path, project_library,
};
use mitcad_model::print_profile::{PrintProfile, ProfileKind};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The base library's files.
struct BaseLibrary(Vec<(String, Vec<u8>)>);

impl LinkResolver for BaseLibrary {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        Err(format!("no component {}", request.component))
    }

    fn print_library(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(self.0.clone())
    }
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A teal PETG with its own appearance.
fn petg(version: u32) -> ItemData {
    ItemData::Material {
        material: Material {
            id: "acme/petg".to_owned(),
            name: "Acme PETG".to_owned(),
            density: 1.27,
            appearance: Some("acme_teal".to_owned()),
            filament: BTreeMap::from([
                ("filament_type".to_owned(), json!("PETG")),
                ("temperature".to_owned(), json!(240)),
            ]),
            slicer: BTreeMap::new(),
            variants: Vec::new(),
            appearances: Vec::new(),
            based_on: Some("petg".to_owned()),
            class: None,
            version: Some(version),
            origin: None,
        },
        appearances: vec![Appearance {
            id: "acme_teal".to_owned(),
            name: "Acme teal".to_owned(),
            base_color: [0.0, 0.5, 0.5],
            ..Appearance::default()
        }],
    }
}

/// A draft process with a layer height.
fn draft(version: u32, layer: f64) -> ItemData {
    let mut profile = PrintProfile::new("draft", "Draft", ProfileKind::Process);
    profile
        .values
        .insert("layer_height".to_owned(), json!(layer));
    profile.version = Some(version);
    ItemData::Profile(profile)
}

fn write_item(folder: &Path, item: &ItemData) {
    let path = folder.join(item_path(item.kind(), item.id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, item.text()).unwrap();
}

/// A design reading the base library, and a project folder.
fn setup(name: &str) -> (Document<MockKernel>, PathBuf) {
    let dir = mitcad_model::testing::unique_temp_path(name);
    let project = dir.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let base = vec![
        (
            item_path(ItemKind::Material, "acme/petg"),
            petg(1).text().into_bytes(),
        ),
        (
            item_path(ItemKind::Process, "draft"),
            draft(1, 0.3).text().into_bytes(),
        ),
    ];
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Arc::new(BaseLibrary(base))));
    (doc, project)
}

#[test]
fn library_items_are_copied_whole_with_their_origin() {
    let (mut doc, project) = setup("print-copy");
    write_item(&project_library(&project), &draft(5, 0.28));
    let libraries = doc.print_libraries(Some(&project));
    assert!(libraries.problems.is_empty(), "{:?}", libraries.problems);
    // The project's draft is nearer than the base's.
    let nearest = libraries.find(None, ItemKind::Process, "draft").unwrap();
    assert_eq!(nearest.library, LibraryKind::Project);

    // A material comes with its appearance.
    let answer = command(
        &mut doc,
        json!({"cmd": "copy_from_print_library", "kind": "material", "id": "acme/petg",
               "project": project.to_string_lossy()}),
    );
    assert_eq!(
        [&answer["id"], &answer["library"], &answer["version"]],
        [&json!("acme/petg"), &json!("base"), &json!(1)]
    );
    let material = doc.material("acme/petg").unwrap().clone();
    assert_eq!(material.density, 1.27);
    assert_eq!(material.appearance.as_deref(), Some("acme_teal"));
    assert_eq!(material.origin.as_ref().unwrap().library, "base");
    assert_eq!(
        doc.appearance("acme_teal").unwrap().base_color,
        [0.0, 0.5, 0.5]
    );
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Copy Acme PETG from Library"
    );
    // The nearest process.
    let answer = command(
        &mut doc,
        json!({"cmd": "copy_from_print_library", "kind": "process", "id": "draft",
               "project": project.to_string_lossy()}),
    );
    assert_eq!(answer["library"], "project");
    assert_eq!(
        doc.print_profile("draft").unwrap().values["layer_height"],
        0.28
    );

    // The listing shows each item where it is, with the design's copies.
    let listing = query(
        &doc,
        json!({"query": "print_library", "project": project.to_string_lossy()}),
    );
    let items = listing["items"].as_array().unwrap();
    let base_draft = items
        .iter()
        .find(|i| i["library"] == "base" && i["id"] == "draft")
        .unwrap();
    assert_eq!(base_draft["shadowed"], true);
    assert_eq!(base_draft["in_design"], json!([]));
    let project_draft = items
        .iter()
        .find(|i| i["library"] == "project" && i["id"] == "draft")
        .unwrap();
    assert_eq!(project_draft["in_design"], json!(["draft"]));
    assert_eq!(listing["updates"], json!([]));

    // A library change leaves the design as it was and is offered.
    write_item(&project_library(&project), &draft(6, 0.2));
    let libraries = doc.print_libraries(Some(&project));
    assert_eq!(
        doc.print_profile("draft").unwrap().values["layer_height"],
        0.28
    );
    let updates = doc.print_library_updates(&libraries);
    assert_eq!(updates.len(), 1);
    assert_eq!((updates[0].version, updates[0].newer), (5, 6));
    // Taken on request.
    let version = doc
        .update_from_print_library(&libraries, ItemKind::Process, "draft")
        .unwrap();
    assert_eq!(version, 6);
    assert_eq!(
        doc.print_profile("draft").unwrap().values["layer_height"],
        0.2
    );
    assert!(doc.print_library_updates(&libraries).is_empty());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        doc.print_profile("draft").unwrap().values["layer_height"],
        0.28
    );

    // A design copied elsewhere keeps its copies whole.
    let saved = doc.to_json();
    let alone = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(alone.material("acme/petg").unwrap().density, 1.27);
    assert!(alone.appearance("acme_teal").is_some());
}

#[test]
fn a_design_s_items_are_saved_into_a_library() {
    let (mut doc, project) = setup("print-save");
    let dir = project.parent().unwrap().to_path_buf();
    let user = dir.join("user");
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
               "values": {"max_print_height": 200}}),
    );
    let libraries = Libraries::load(Vec::new(), Some(&user), Some(&project));
    let (path, version) = doc
        .save_to_print_library(&libraries, LibraryKind::User, ItemKind::Printer, "bench")
        .unwrap();
    assert_eq!(version, 1);
    assert_eq!(path, user.join("print/printers/bench.json"));
    let origin = doc.print_profile("bench").unwrap().origin.clone().unwrap();
    assert_eq!((origin.library.as_str(), origin.version), ("user", Some(1)));
    // Saved again: the next version.
    let libraries = Libraries::load(Vec::new(), Some(&user), Some(&project));
    let item = libraries
        .find(Some(LibraryKind::User), ItemKind::Printer, "bench")
        .unwrap();
    assert_eq!(item.data.version(), 1);
    let (_, version) = doc
        .save_to_print_library(&libraries, LibraryKind::User, ItemKind::Printer, "bench")
        .unwrap();
    assert_eq!(version, 2);
    // Another design's own printer of the same id gets a free id there,
    // and undo takes its file away (mitcad#655).
    let mut other = Document::new(MockKernel::default());
    command(
        &mut other,
        json!({"cmd": "create_print_profile", "kind": "printer", "name": "Bench"}),
    );
    let libraries = Libraries::load(Vec::new(), Some(&user), Some(&project));
    let (path, version) = other
        .save_to_print_library(&libraries, LibraryKind::User, ItemKind::Printer, "bench")
        .unwrap();
    assert_eq!(
        (path.clone(), version),
        (user.join("print/printers/bench-2.json"), 1)
    );
    let origin = other
        .print_profile("bench")
        .unwrap()
        .origin
        .clone()
        .unwrap();
    assert_eq!(origin.id, "bench-2");
    // Its name too: the library has a Bench (mitcad#655).
    assert_eq!(other.print_profile("bench").unwrap().name, "Bench 2");
    other.undo();
    assert!(!path.exists());
    assert_eq!(other.print_profile("bench").unwrap().origin, None);
    // A built-in material, into the project.
    let (path, _) = doc
        .save_to_print_library(&libraries, LibraryKind::Project, ItemKind::Material, "pla")
        .unwrap();
    assert_eq!(path, project_library(&project).join("materials/pla.json"));
    // The base library is not written.
    let error = doc
        .save_to_print_library(&libraries, LibraryKind::Base, ItemKind::Material, "pla")
        .unwrap_err()
        .to_string();
    assert!(error.contains("component library"), "{error}");
}

/// The base library's items in this repository
/// (`tools/libraries/print-library`, which the parts library's generator
/// copies): all read, materials with library appearances.
#[test]
fn the_base_library_s_items_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/libraries/print-library");
    let libraries = Libraries::load(mitcad_model::print_library::folder_files(&root), None, None);
    assert!(libraries.problems.is_empty(), "{:?}", libraries.problems);
    let count = |kind: ItemKind| {
        libraries
            .items
            .iter()
            .filter(|i| i.data.kind() == kind)
            .count()
    };
    assert_eq!(count(ItemKind::Material), 6);
    assert_eq!(count(ItemKind::Printer), 9);
    assert_eq!(count(ItemKind::Process), 4);
    for item in &libraries.items {
        assert_eq!(item.data.version(), 1, "{}", item.path);
        if let ItemData::Material { material, .. } = &item.data {
            let look = material.appearance.as_deref().unwrap();
            assert!(
                mitcad_model::appearance::library_appearance(look).is_some(),
                "{look}"
            );
        }
    }
    // A printer copied into a design is its build volume there.
    let mut doc = Document::new(MockKernel::default());
    let printer = libraries
        .find(None, ItemKind::Printer, "bambu-lab-a1-mini")
        .unwrap();
    let id = doc.copy_from_print_library(printer).unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Printer",
               "key": "profile", "value": id}),
    );
    let volume = query(&doc, json!({"query": "build_volume"}));
    assert_eq!(volume["height"], 180.0);
    assert_eq!(volume["bed"][0], json!([-90.0, -90.0]));
}

/// The rows of `library_items` of a kind.
fn rows(doc: &Document<MockKernel>, kind: &str, project: &Path) -> Vec<Value> {
    query(
        doc,
        json!({"query": "library_items", "kind": kind, "project": project.to_string_lossy()}),
    )["items"]
        .as_array()
        .unwrap()
        .clone()
}

fn row<'a>(rows: &'a [Value], place: &str, id: &str) -> &'a Value {
    rows.iter()
        .find(|r| r["place"] == place && r["id"] == id)
        .unwrap_or_else(|| panic!("no {place} row {id}: {rows:?}"))
}

#[test]
fn library_items_list_each_item_once_where_it_lives() {
    let (mut doc, project) = setup("print-items");
    write_item(&project_library(&project), &draft(5, 0.28));
    command(
        &mut doc,
        json!({"cmd": "copy_from_print_library", "kind": "material", "id": "acme/petg",
               "project": project.to_string_lossy()}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_material", "name": "Mine", "density": 2.0}),
    );
    let materials = rows(&doc, "material", &project);
    let steel = row(&materials, "mitcad", "steel");
    assert_eq!(steel["library"], Value::Null);
    assert_eq!(steel["read_only"], true);
    assert_eq!(steel["in_design"], "steel");
    assert_eq!(steel["group"], "metals");
    // The built-in PLA has a filament type: an FDM plastic (mitcad#1102).
    assert_eq!(row(&materials, "mitcad", "pla")["group"], "fdm_plastics");
    assert_eq!(row(&materials, "mitcad", "pla")["class_fixed"], true);
    assert_eq!(
        row(&materials, "mitcad", "resin_standard")["group"],
        "resins"
    );
    // The base library's item and the design's copy are one row.
    let petg = row(&materials, "mitcad", "acme/petg");
    assert_eq!(petg["library"], "base");
    assert_eq!(petg["in_design"], "acme/petg");
    assert_eq!(petg["changed"], false);
    assert_eq!(petg["group"], "fdm_plastics");
    assert_eq!(petg["class"], "fdm_plastic");
    assert!(petg["display_color"].is_array(), "{petg}");
    assert_eq!(
        materials.iter().filter(|r| r["id"] == "acme/petg").count(),
        1
    );
    let mine = row(&materials, "design", "custom1");
    assert_eq!(
        (&mine["name"], &mine["read_only"], &mine["in_design"]),
        (&json!("Mine"), &json!(false), &json!("custom1"))
    );
    assert_eq!(mine["group"], "other");
    assert_eq!(mine["class"], "other");

    // The project's draft only: the base's is shadowed.
    let processes = rows(&doc, "process", &project);
    assert_eq!(processes.len(), 1, "{processes:?}");
    let draft_row = row(&processes, "project", "draft");
    assert_eq!(
        (&draft_row["in_design"], &draft_row["chosen"]),
        (&Value::Null, &json!(false))
    );
    // Chosen for the design: copied in, in the same undo step.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "profile", "value": "draft", "library": "project",
               "project": project.to_string_lossy()}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Use Process Draft"
    );
    let processes = rows(&doc, "process", &project);
    let draft_row = row(&processes, "project", "draft");
    assert_eq!(
        (
            &draft_row["in_design"],
            &draft_row["chosen"],
            &draft_row["changed"]
        ),
        (&json!("draft"), &json!(true), &json!(false))
    );
    // Chosen again: the design's copy is reused.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "profile", "value": "draft", "library": "project",
               "project": project.to_string_lossy()}),
    );
    assert_eq!(doc.print_profiles().len(), 1);
    // A newer version in the library.
    write_item(&project_library(&project), &draft(6, 0.2));
    let processes = rows(&doc, "process", &project);
    assert_eq!(row(&processes, "project", "draft")["changed"], true);
    // Undone: the design has no copy and chooses none.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(doc.print_profile("draft").is_none());
}

#[test]
fn library_items_are_edited_where_they_live() {
    let (mut doc, project) = setup("print-edit");
    let folder = project_library(&project);
    write_item(&folder, &petg(3));
    let path = project.to_string_lossy().to_string();
    // Chosen for the root: copied in with its appearance, one undo step.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0",
               "material": "acme/petg", "library": "project", "project": path}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    let undo = query(&doc, json!({"query": "document"}))["undo"].clone();
    assert!(
        undo.as_str().unwrap().starts_with("Set Default Material"),
        "{undo}"
    );
    let copy = doc.material("acme/petg").unwrap().clone();
    assert_eq!(copy.origin.as_ref().unwrap().version, Some(3));
    assert!(doc.appearance("acme_teal").is_some());

    // Edited in the project's library: the version goes up and the
    // design's copy takes the same change.
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "acme/petg", "library": "project",
               "project": path, "density": 1.3, "filament": {"temperature": 250}}),
    );
    assert_eq!(answer["version"], 4);
    let read = |doc: &Document<MockKernel>| {
        let libraries = doc.print_libraries(Some(&project));
        let item = libraries
            .find(Some(LibraryKind::Project), ItemKind::Material, "acme/petg")
            .unwrap();
        let ItemData::Material { material, .. } = &item.data else {
            unreachable!()
        };
        material.clone()
    };
    let item = read(&doc);
    assert_eq!((item.density, item.version), (1.3, Some(4)));
    assert_eq!(item.filament["temperature"], 250);
    let copy = doc.material("acme/petg").unwrap().clone();
    assert_eq!(copy.density, 1.3);
    assert_eq!(copy.origin.as_ref().unwrap().version, Some(4));
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Edit Material Acme PETG"
    );
    // Undo restores the design's copy and the library's file (mitcad#655),
    // redo writes them again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(doc.material("acme/petg").unwrap().density, 1.27);
    assert_eq!((read(&doc).density, read(&doc).version), (1.27, Some(3)));
    let materials = rows(&doc, "material", &project);
    assert_eq!(row(&materials, "project", "acme/petg")["changed"], false);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(doc.material("acme/petg").unwrap().density, 1.3);
    assert_eq!((read(&doc).density, read(&doc).version), (1.3, Some(4)));
    command(&mut doc, json!({"cmd": "undo"}));

    // An edit goes on top of the library's newest version, key by key.
    write_item(&folder, &petg(5));
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "acme/petg", "library": "project",
               "project": path, "filament": {"bed_temperature": 80}}),
    );
    let item = read(&doc);
    assert_eq!((item.density, item.version), (1.27, Some(6)));
    assert_eq!(item.filament["bed_temperature"], 80);
    // The older copy takes the change and stays older.
    let copy = doc.material("acme/petg").unwrap().clone();
    assert_eq!(copy.filament["bed_temperature"], 80);
    assert_eq!(copy.origin.as_ref().unwrap().version, Some(3));

    // Mitcad's library is read-only.
    let error = doc
        .command(
            &json!({"cmd": "edit_material", "id": "acme/petg", "library": "base",
                    "density": 2.0})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("read-only"), "{error}");

    // Deleted from the library: the design keeps the copy the root uses,
    // as its own unsaved material (mitcad#655).
    let answer = command(
        &mut doc,
        json!({"cmd": "delete_material", "id": "acme/petg", "library": "project",
               "project": path}),
    );
    assert_eq!(
        (&answer["kept"], &answer["removed"]),
        (&json!(["acme/petg"]), &json!([]))
    );
    let file = folder.join("materials/acme/petg.json");
    assert!(!file.exists());
    assert_eq!(doc.material("acme/petg").unwrap().origin, None);
    let materials = rows(&doc, "material", &project);
    assert_eq!(
        row(&materials, "design", "acme/petg")["in_design"],
        "acme/petg"
    );
    // The base library's item of the same id is listed again.
    assert_eq!(
        row(&materials, "mitcad", "acme/petg")["in_design"],
        Value::Null
    );
    // Undone: the file and the copy's origin come back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(file.exists());
    assert_eq!(
        doc.material("acme/petg")
            .unwrap()
            .origin
            .as_ref()
            .unwrap()
            .library,
        "project"
    );
    command(&mut doc, json!({"cmd": "redo"}));
    assert!(!file.exists());
}

#[test]
fn library_items_are_made_and_duplicated() {
    let (mut doc, project) = setup("print-make");
    let folder = project_library(&project);
    let path = project.to_string_lossy().to_string();
    let depth = doc.undo_depth();
    // Duplicate of a base material into the project: `<name> (copy)`
    // (mitcad#828), its id the name's, with the appearance it carries.
    let answer = command(
        &mut doc,
        json!({"cmd": "create_material", "library": "project", "project": path,
               "based_on": {"library": "base", "id": "acme/petg"}}),
    );
    assert_eq!(
        (&answer["id"], &answer["library"], &answer["version"]),
        (&json!("acme-petg-copy"), &json!("project"), &json!(1))
    );
    let libraries = doc.print_libraries(Some(&project));
    let made = libraries
        .find(
            Some(LibraryKind::Project),
            ItemKind::Material,
            "acme-petg-copy",
        )
        .unwrap();
    let ItemData::Material {
        material,
        appearances,
    } = &made.data
    else {
        unreachable!()
    };
    assert_eq!(material.name, "Acme PETG (copy)");
    assert_eq!(material.based_on.as_deref(), Some("acme/petg"));
    assert_eq!(appearances.len(), 1);
    assert_eq!(appearances[0].id, "acme_teal");
    // A new empty one.
    let answer = command(
        &mut doc,
        json!({"cmd": "create_material", "library": "project", "project": path}),
    );
    assert_eq!(answer["id"], "material-1"); // named "Material 1" (mitcad#655, mitcad#828)
    // The design did not change; each file is an undo step of its own.
    assert_eq!(doc.undo_depth(), depth + 2);
    assert!(doc.custom_materials().is_empty());
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(!folder.join("materials/material-1.json").exists());
    command(&mut doc, json!({"cmd": "redo"}));
    assert!(folder.join("materials/material-1.json").exists());

    // Profiles likewise.
    let answer = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "library": "project", "project": path,
               "based_on": {"library": "base", "id": "draft"}}),
    );
    assert_eq!(answer["id"], "draft-copy");
    let answer = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "library": "project", "project": path,
               "kind": "printer"}),
    );
    assert_eq!(answer["id"], "printer-1");
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": "draft-copy", "library": "project",
               "project": path, "values": {"layer_height": 0.24}}),
    );
    assert_eq!(answer["version"], 2);
    assert!(doc.print_profiles().is_empty());
    let answer = command(
        &mut doc,
        json!({"cmd": "delete_print_profile", "id": "draft-copy", "library": "project",
               "project": path}),
    );
    assert_eq!(answer["removed"], json!([]));
    assert!(!folder.join("print/processes/draft-copy.json").exists());

    // A library's material duplicated into the design: with its
    // appearance, in one undo step.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "create_material", "name": "Teal", "project": path,
               "based_on": {"library": "base", "id": "acme/petg"}}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    let teal = doc.custom_materials()[0].clone();
    assert_eq!(teal.name, "Teal");
    assert_eq!(teal.based_on.as_deref(), Some("acme/petg"));
    assert_eq!(teal.origin, None);
    assert_eq!(teal.appearance.as_deref(), Some("acme_teal"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(doc.custom_materials().is_empty());
    assert!(doc.appearance("acme_teal").is_none());

    // A name a library has gets a counter in the design too (mitcad#655).
    let mut named = PrintProfile::new("process-1", "Process 1", ProfileKind::Process);
    named.version = Some(1);
    write_item(&folder, &ItemData::Profile(named));
    let made = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process", "project": path}),
    );
    assert_eq!(
        doc.print_profile(made["id"].as_str().unwrap())
            .unwrap()
            .name,
        "Process 2"
    );
}

/// The group of a design's material in `library_items`.
fn group_of(doc: &Document<MockKernel>, project: &Path, id: &str) -> Value {
    let rows = rows(doc, "material", project);
    row(&rows, "design", id)["group"].clone()
}

/// The id a `create_material` answered.
fn created(doc: &mut Document<MockKernel>, create: Value) -> String {
    let answer = command(doc, create);
    answer["id"].as_str().unwrap().to_owned()
}

#[test]
fn a_material_class_is_stored_kept_by_a_copy_and_groups_the_rows() {
    let (mut doc, project) = setup("material-class");
    // A new material of a class chosen.
    let create = json!({"cmd": "create_material", "name": "Resin", "class": "plastic"});
    let resin = created(&mut doc, create);
    assert_eq!(group_of(&doc, &project, &resin), "plastics");
    // Without a class it is stored as worked out (no filament: other);
    // a print setting makes it an FDM plastic, the stored class kept
    // (mitcad#1102).
    let plain = created(&mut doc, json!({"cmd": "create_material", "name": "Plain"}));
    let edit = json!({"cmd": "edit_material", "id": plain, "filament": {"temperature": 210}});
    command(&mut doc, edit);
    assert_eq!(group_of(&doc, &project, &plain), "fdm_plastics");
    let materials = rows(&doc, "material", &project);
    assert_eq!(row(&materials, "design", &plain)["class"], "fdm_plastic");
    assert_eq!(row(&materials, "design", &plain)["class_fixed"], true);
    let stored = |doc: &Document<MockKernel>, id: &str| {
        let material = doc.custom_materials().iter().find(|m| m.id == id);
        material.unwrap().class
    };
    assert_eq!(stored(&doc, &plain), Some(MaterialClass::Other));
    // FDM Plastic and Resin chosen for materials without print settings
    // are stored and group them.
    let create = json!({"cmd": "create_material", "name": "Spool", "class": "fdm_plastic"});
    let spool = created(&mut doc, create);
    assert_eq!(group_of(&doc, &project, &spool), "fdm_plastics");
    assert_eq!(stored(&doc, &spool), Some(MaterialClass::FdmPlastic));
    let create = json!({"cmd": "create_material", "name": "Grey resin", "class": "resin"});
    let grey = created(&mut doc, create);
    assert_eq!(group_of(&doc, &project, &grey), "resins");
    assert_eq!(stored(&doc, &grey), Some(MaterialClass::Resin));
    // A filament made FDM Plastic keeps `plastic`, which older versions
    // read.
    let create = json!({"cmd": "create_material", "name": "Filament", "class": "fdm_plastic",
                        "filament": {"filament_type": "PLA"}});
    let filament = created(&mut doc, create);
    assert_eq!(group_of(&doc, &project, &filament), "fdm_plastics");
    assert_eq!(stored(&doc, &filament), Some(MaterialClass::Plastic));
    // A copy of a built-in metal keeps its class, stored on the copy.
    let create = json!({"cmd": "create_material", "based_on": "aluminum", "name": "Alu"});
    let copy = created(&mut doc, create);
    let stored = doc
        .custom_materials()
        .iter()
        .find(|m| m.id == copy)
        .unwrap()
        .class;
    assert_eq!(stored, Some(MaterialClass::Metal));
    // The class changed in the editor: one undo step, the row follows.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": copy, "class": "other"}),
    );
    assert_eq!(group_of(&doc, &project, &copy), "other");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(group_of(&doc, &project, &copy), "metals");
    // An unknown class is refused.
    let wood = json!({"cmd": "create_material", "class": "wood"});
    assert!(doc.command(&wood.to_string()).is_err());
}

/// The `changed` mark of the project's draft in `library_items`.
fn draft_changed(doc: &Document<MockKernel>, project: &Path) -> Value {
    row(&rows(doc, "process", project), "project", "draft")["changed"].clone()
}

#[test]
fn library_edits_undo_with_their_file_and_a_kept_change_is_asked_once() {
    let (mut doc, project) = setup("print-undo");
    let folder = project_library(&project);
    let path = project.to_string_lossy().to_string();
    write_item(&folder, &draft(2, 0.25));
    let file = folder.join("print/processes/draft.json");
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "profile", "value": "draft", "library": "project",
               "project": path}),
    );
    let layer = |doc: &Document<MockKernel>| {
        doc.print_profile("draft").unwrap().values["layer_height"].clone()
    };

    // An edit of a saved item: the file and the design in one undo step,
    // which undo and redo write back.
    let before = std::fs::read_to_string(&file).unwrap();
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": "draft", "library": "project",
               "project": path, "values": {"layer_height": 0.2}}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    let after = std::fs::read_to_string(&file).unwrap();
    assert_ne!(before, after);
    assert_eq!(layer(&doc), 0.2);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert_eq!(layer(&doc), 0.25);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), after);
    assert_eq!(layer(&doc), 0.2);
    assert_eq!(draft_changed(&doc, &project), false);

    // Changed in another design: asked about.
    write_item(&folder, &draft(4, 0.16));
    assert_eq!(draft_changed(&doc, &project), true);
    let libraries = doc.print_libraries(Some(&project));
    assert_eq!(doc.print_library_updates(&libraries).len(), 1);
    // Kept: this design's values stay and the change is not asked about
    // again, one undo step.
    let answer = command(
        &mut doc,
        json!({"cmd": "decline_print_library_update", "kind": "process", "id": "draft",
               "project": path}),
    );
    assert_eq!(answer["version"], 4);
    assert_eq!(layer(&doc), 0.2);
    assert_eq!(draft_changed(&doc, &project), false);
    assert!(doc.print_library_updates(&libraries).is_empty());
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Keep Draft"
    );
    // The row shows what the design uses.
    let processes = rows(&doc, "process", &project);
    assert_eq!(
        row(&processes, "project", "draft")["item"]["values"]["layer_height"],
        0.2
    );
    // Changed again: asked again; the changes taken.
    write_item(&folder, &draft(5, 0.12));
    assert_eq!(draft_changed(&doc, &project), true);
    command(
        &mut doc,
        json!({"cmd": "update_from_print_library", "kind": "process", "id": "draft",
               "project": path}),
    );
    assert_eq!(layer(&doc), 0.12);
    assert_eq!(draft_changed(&doc, &project), false);

    // Deleted while the design uses it: the file goes, the design keeps
    // the process as its own; undo brings the file back.
    let answer = command(
        &mut doc,
        json!({"cmd": "delete_print_profile", "id": "draft", "library": "project",
               "project": path}),
    );
    assert_eq!(answer["kept"], json!(["draft"]));
    assert!(!file.exists());
    assert_eq!(doc.print_profile("draft").unwrap().origin, None);
    let processes = rows(&doc, "process", &project);
    assert_eq!(row(&processes, "design", "draft")["chosen"], true);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(file.exists());
    assert_eq!(draft_changed(&doc, &project), false);
    assert_eq!(layer(&doc), 0.12);
}

#[test]
fn an_edit_to_the_library_s_newer_value_reaches_a_copy_that_kept_its_own() {
    // mitcad#758: another design changed the library's item, this design
    // kept its values, and then types the library's value: the library's
    // file stays as it is, the copy takes the value.
    let (mut doc, project) = setup("print-same-value");
    let folder = project_library(&project);
    let path = project.to_string_lossy().to_string();

    // A material.
    write_item(&folder, &petg(1));
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0",
               "material": "acme/petg", "library": "project", "project": path}),
    );
    let mut newer = petg(2);
    if let ItemData::Material { material, .. } = &mut newer {
        material
            .filament
            .insert("temperature".to_owned(), json!(245));
    }
    write_item(&folder, &newer);
    let answer = command(
        &mut doc,
        json!({"cmd": "decline_print_library_update", "kind": "material", "id": "acme/petg",
               "project": path}),
    );
    assert_eq!(answer["version"], 2);
    let temperature = |doc: &Document<MockKernel>| {
        doc.material("acme/petg").unwrap().filament["temperature"].clone()
    };
    assert_eq!(temperature(&doc), 240);
    let file = folder.join(item_path(ItemKind::Material, "acme/petg"));
    let before = std::fs::read_to_string(&file).unwrap();
    let depth = doc.undo_depth();
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "acme/petg", "library": "project",
               "project": path, "filament": {"temperature": 245}}),
    );
    assert_eq!(answer["version"], 2);
    assert_eq!(temperature(&doc), 245);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Edit Material Acme PETG"
    );
    // The same again changes nothing: no step.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "acme/petg", "library": "project",
               "project": path, "filament": {"temperature": 245}}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    // Undone: the copy's own value is back, the file as it was.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(temperature(&doc), 240);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

    // A process profile.
    write_item(&folder, &draft(2, 0.25));
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "profile", "value": "draft", "library": "project",
               "project": path}),
    );
    write_item(&folder, &draft(3, 0.2));
    let answer = command(
        &mut doc,
        json!({"cmd": "decline_print_library_update", "kind": "process", "id": "draft",
               "project": path}),
    );
    assert_eq!(answer["version"], 3);
    let layer = |doc: &Document<MockKernel>| {
        doc.print_profile("draft").unwrap().values["layer_height"].clone()
    };
    assert_eq!(layer(&doc), 0.25);
    let file = folder.join(item_path(ItemKind::Process, "draft"));
    let before = std::fs::read_to_string(&file).unwrap();
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_print_profile", "id": "draft", "library": "project",
               "project": path, "values": {"layer_height": 0.2}}),
    );
    assert_eq!(answer["version"], 3);
    assert_eq!(layer(&doc), 0.2);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Edit Process Draft"
    );
    assert_eq!(draft_changed(&doc, &project), false);
}

/// My Library (mitcad#704): a built-in material duplicated into a library
/// by its id, renamed, its density changed and deleted, and a library's
/// printer duplicated, on an empty design of its own that keeps none of
/// them.
#[test]
fn a_library_is_managed_on_an_empty_design() {
    let (mut doc, project) = setup("my-library");
    let folder = project_library(&project);
    let path = project.to_string_lossy().to_string();
    // Duplicate of the built-in PLA, by its id.
    let answer = command(
        &mut doc,
        json!({"cmd": "create_material", "library": "project", "project": path,
               "based_on": "pla"}),
    );
    assert_eq!(answer["id"], "pla-plastic-copy");
    let file = folder.join(item_path(ItemKind::Material, "pla-plastic-copy"));
    let read = |file: &Path| -> Value {
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
    };
    let item = read(&file);
    assert_eq!(item["material"]["name"], "PLA Plastic (copy)");
    assert_eq!(item["material"]["based_on"], "pla");
    assert_eq!(item["material"]["class"], "plastic");
    assert_eq!(item["material"]["filament"]["filament_type"], "PLA");
    // Listed in the project's place, the built-in PLA still built in.
    let materials = rows(&doc, "material", &project);
    assert_eq!(
        row(&materials, "project", "pla-plastic-copy")["in_design"],
        Value::Null
    );
    assert_eq!(row(&materials, "mitcad", "pla")["read_only"], true);
    // Renamed, then its density: each the next version of the file.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "library": "project", "project": path,
               "id": "pla-plastic-copy", "name": "Bench PLA"}),
    );
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_material", "library": "project", "project": path,
               "id": "pla-plastic-copy", "density": 1.3}),
    );
    assert_eq!(answer["version"], 3);
    let item = read(&file);
    assert_eq!(
        (&item["material"]["name"], &item["material"]["density"]),
        (&json!("Bench PLA"), &json!(1.3))
    );
    // A library's printer duplicated within the library.
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "library": "project", "project": path,
               "kind": "printer", "name": "Bench X", "values": {"max_print_height": 250}}),
    );
    let answer = command(
        &mut doc,
        json!({"cmd": "create_print_profile", "library": "project", "project": path,
               "kind": "printer", "based_on": {"library": "project", "id": "bench-x"}}),
    );
    assert_eq!(answer["id"], "bench-x-copy");
    let printer = read(&folder.join(item_path(ItemKind::Printer, "bench-x-copy")));
    assert_eq!(printer["profile"]["name"], "Bench X (copy)");
    // As a number, whether the file writes 250 or 250.0.
    assert_eq!(
        printer["profile"]["values"]["max_print_height"].as_f64(),
        Some(250.0)
    );
    // Deleted: the file goes, nothing of the design to keep.
    let answer = command(
        &mut doc,
        json!({"cmd": "delete_material", "library": "project", "project": path,
               "id": "pla-plastic-copy"}),
    );
    assert_eq!(
        (&answer["kept"], &answer["removed"]),
        (&json!([]), &json!([]))
    );
    assert!(!file.exists());
    // The design has none of them.
    assert!(doc.custom_materials().is_empty());
    assert!(doc.print_profiles().is_empty());
}

#[test]
fn library_items_give_the_parts_of_a_profile_name_for_the_picker() {
    // mitcad#1108: a printer's name without its nozzle and the nozzle it
    // says; a process's name before its `@` and the printer after it.
    let (mut doc, project) = setup("print-name-parts");
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "printer",
               "name": "Acme Lab X1 Carbon 0.4 nozzle"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "printer", "name": "Bench"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "process",
               "name": "0.20mm Standard @Acme X1C"}),
    );
    let printers = rows(&doc, "printer", &project);
    let x1 = printers
        .iter()
        .find(|r| r["name"] == "Acme Lab X1 Carbon 0.4 nozzle")
        .unwrap();
    assert_eq!(x1["bare_name"], "Acme Lab X1 Carbon");
    assert_eq!(x1["name_nozzle"], json!(0.4));
    let bench = printers.iter().find(|r| r["name"] == "Bench").unwrap();
    assert_eq!(
        (&bench["bare_name"], &bench["name_nozzle"]),
        (&json!("Bench"), &Value::Null)
    );
    let processes = rows(&doc, "process", &project);
    let standard = processes
        .iter()
        .find(|r| r["name"] == "0.20mm Standard @Acme X1C")
        .unwrap();
    assert_eq!(
        (&standard["bare_name"], &standard["for_printer"]),
        (&json!("0.20mm Standard"), &json!("Acme X1C"))
    );
    // The base library's Draft names no printer.
    let draft_row = row(&processes, "mitcad", "draft");
    assert_eq!(
        (&draft_row["bare_name"], &draft_row["for_printer"]),
        (&json!("Draft"), &Value::Null)
    );
}
