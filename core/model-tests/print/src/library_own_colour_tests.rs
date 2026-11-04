// SPDX-License-Identifier: MIT
//! A library material whose file lacks its default appearance
//! (mitcad#1242): a slicer import or an older version wrote the item
//! without its own colour (`<id>_colour`). A rename, a density or a print
//! setting changed is never refused for it: the own colour is made again
//! from the material's filament colour and carried by the item from then
//! on; another missing default is kept with a warning in the reply; a
//! Duplicate of such an item is made; the design's copies take the edit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::material::Material;
use mitcad_model::print_library::{ItemData, item_path, project_library};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The item's id: dashes, as a slicer import's (`generic-tpu-x`); its own
/// colour's id has underscores.
const ID: &str = "generic-tpu-x";
const OWN_COLOUR: &str = "generic_tpu_x_colour";
/// A slicer's default filament colour as an import keeps it.
const ORANGE: &str = "[\"#FF8000\"]";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A TPU of the project's library whose file names `appearance` as its
/// default and carries no appearance, its slicer's default colour orange.
fn lost_look(version: u32, appearance: &str) -> ItemData {
    let colour = BTreeMap::from([("default_filament_colour".to_owned(), ORANGE.to_owned())]);
    ItemData::Material {
        material: Material {
            id: ID.to_owned(),
            name: "Generic TPU X".to_owned(),
            density: 1.22,
            appearance: Some(appearance.to_owned()),
            filament: BTreeMap::from([
                ("filament_type".to_owned(), json!("TPU")),
                ("temperature".to_owned(), json!(220)),
            ]),
            slicer: BTreeMap::from([("bambu_studio".to_owned(), colour)]),
            variants: Vec::new(),
            appearances: Vec::new(),
            based_on: None,
            class: None,
            version: Some(version),
            origin: None,
            properties: Default::default(),
            reference: None,
        },
        appearances: Vec::new(),
    }
}

/// Writes the item into the library folder; returns its file.
fn write_item(folder: &Path, item: &ItemData) -> PathBuf {
    let path = folder.join(item_path(item.kind(), item.id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, item.text()).unwrap();
    path
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A design and a project whose library has the item without its own
/// colour.
struct Setup {
    doc: Document<MockKernel>,
    /// The project's path, as the commands name it.
    project: String,
    /// The project's library folder.
    folder: PathBuf,
    /// The item's file.
    file: PathBuf,
}

fn setup(name: &str) -> Setup {
    let project = mitcad_model::testing::unique_temp_path(name).join("project");
    std::fs::create_dir_all(&project).unwrap();
    let folder = project_library(&project);
    let file = write_item(&folder, &lost_look(1, OWN_COLOUR));
    Setup {
        doc: Document::new(MockKernel::default()),
        project: project.to_string_lossy().to_string(),
        folder,
        file,
    }
}

/// The file carries the own colour: its id, its name (`<the material's
/// name> color` when made), the slicer's default colour (#FF8000) and an
/// FDM plastic's look.
fn carries_own_colour(file: &Value, look_name: &str) {
    let look = &file["appearance"];
    assert_eq!(look["id"], OWN_COLOUR, "{file}");
    assert_eq!(look["name"], look_name);
    let color: Vec<f64> = look["base_color"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_f64().unwrap())
        .collect();
    assert!((color[0] - 1.0).abs() < 1e-6, "{color:?}");
    assert!((color[1] - 128.0 / 255.0).abs() < 1e-6, "{color:?}");
    assert!(color[2].abs() < 1e-6, "{color:?}");
    assert_eq!(look["class"], "fdm_plastic");
    assert_eq!(file["material"]["appearance"], OWN_COLOUR);
}

#[test]
fn a_library_material_without_its_own_colour_is_edited_and_carries_it_again() {
    let Setup {
        mut doc,
        project,
        folder,
        file,
    } = setup("own-colour-edit");
    // A rename, a density and a print setting, each on the file without
    // the colour.
    let edits = [
        ("name", json!("Shop TPU")),
        ("density", json!(1.25)),
        ("filament", json!({"temperature": 230})),
    ];
    for (key, value) in edits {
        write_item(&folder, &lost_look(1, OWN_COLOUR));
        let mut edit = json!({"cmd": "edit_material", "id": ID, "library": "project",
                              "project": project});
        edit[key] = value;
        let answer = command(&mut doc, edit);
        assert_eq!(answer["version"], 2, "{key}: {answer}");
        assert!(answer.get("warnings").is_none(), "{key}: {answer}");
        let saved = read(&file);
        let name = saved["material"]["name"].as_str().unwrap().to_owned();
        carries_own_colour(&saved, &format!("{name} color"));
        match key {
            "name" => assert_eq!(name, "Shop TPU"),
            "density" => assert_eq!(saved["material"]["density"], 1.25),
            _ => assert_eq!(saved["material"]["filament"]["temperature"], 230),
        }
    }
    // The library lists it with the colour it carries.
    let listed = query(
        &doc,
        json!({"query": "library_items", "kind": "material", "project": project}),
    );
    let row = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["place"] == "project" && r["id"] == ID)
        .cloned()
        .unwrap();
    assert_eq!(row["appearances"], json!([OWN_COLOUR]));
    assert_eq!(row["carried_appearances"][0]["id"], OWN_COLOUR);
}

#[test]
fn another_missing_default_is_kept_with_a_warning() {
    let Setup {
        mut doc,
        project,
        folder,
        file,
    } = setup("own-colour-other");
    write_item(&folder, &lost_look(1, "lost_look"));
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_material", "id": ID, "library": "project", "project": project,
               "name": "Shop TPU"}),
    );
    assert_eq!(answer["version"], 2);
    let warnings = answer["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{answer}");
    assert!(
        warnings[0]
            .as_str()
            .unwrap()
            .contains("there is no appearance lost_look"),
        "{answer}"
    );
    // The edit is kept, the default too; nothing is carried for it.
    let saved = read(&file);
    assert_eq!(saved["material"]["name"], "Shop TPU");
    assert_eq!(saved["material"]["appearance"], "lost_look");
    assert!(saved.get("appearance").is_none(), "{saved}");
    // A change that names a missing appearance is refused as before.
    let error = doc
        .command(
            &json!({"cmd": "edit_material", "id": ID, "library": "project",
                    "project": project, "appearance": "no_such_look"})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("there is no appearance no_such_look"),
        "{error}"
    );
}

#[test]
fn a_duplicate_of_an_item_without_its_own_colour_is_made() {
    let Setup {
        mut doc,
        project,
        file,
        ..
    } = setup("own-colour-duplicate");
    let answer = command(
        &mut doc,
        json!({"cmd": "create_material", "library": "project", "project": project,
               "based_on": {"library": "project", "id": ID}}),
    );
    let id = answer["id"].as_str().unwrap().to_owned();
    assert_eq!(id, "generic-tpu-x-copy");
    assert!(answer.get("warnings").is_none(), "{answer}");
    let copy = read(&file.with_file_name(format!("{id}.json")));
    assert_eq!(copy["material"]["name"], "Generic TPU X (copy)");
    // The source's own colour, made again under its id.
    carries_own_colour(&copy, "Generic TPU X (copy) color");
    // The source's file did not change.
    assert!(read(&file).get("appearance").is_none());
}

#[test]
fn the_design_s_copy_takes_the_edit_of_an_item_without_its_own_colour() {
    let Setup {
        mut doc,
        project,
        folder,
        file,
    } = setup("own-colour-copy");
    // Carried once (version 2), chosen for the root: the design's copy has
    // the colour.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": ID, "library": "project", "project": project,
               "density": 1.23}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": ID,
               "library": "project", "project": project}),
    );
    assert!(doc.appearance(OWN_COLOUR).is_some());
    // The file loses the colour again; a rename reaches the copy.
    write_item(&folder, &lost_look(2, OWN_COLOUR));
    let answer = command(
        &mut doc,
        json!({"cmd": "edit_material", "id": ID, "library": "project", "project": project,
               "name": "Shop TPU"}),
    );
    assert_eq!(answer["version"], 3);
    // The design has the colour (made under the name of the time): the
    // file carries the design's again.
    carries_own_colour(&read(&file), "Generic TPU X color");
    let copy = doc.material(ID).unwrap();
    assert_eq!(copy.name, "Shop TPU");
    assert_eq!(copy.appearance.as_deref(), Some(OWN_COLOUR));
    assert_eq!(copy.origin.as_ref().unwrap().version, Some(3));
}
