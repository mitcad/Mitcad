// SPDX-License-Identifier: MIT
//! Filaments imported from an installed slicer's presets with their colour
//! and every other value they inherit (mitcad#1053), with the mock kernel
//! and a synthetic Bambu Studio data folder the tests write (never a real
//! slicer's): a user preset that inherits a system preset, which inherits
//! two more levels, the colour and the temperatures spread over them; a
//! user preset whose base is missing; presets with no `filament_colour`
//! anywhere, one with the family's `default_filament_colour`. Every
//! imported filament gets an appearance, the neutral one when no colour is
//! found, and the material's colour is its appearance's.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::print_files_tests::{command, design, export_to};
use mitcad_model::Document;
use mitcad_model::print_files::NEUTRAL_FILAMENT_COLOR;
use mitcad_model::print_library::{ItemData, ItemKind, Libraries, LibraryKind};
use mitcad_model::testing::MockKernel;

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value.to_string()).unwrap();
}

/// `#RRGGBB` as the sRGB components an appearance has.
fn rgb(hex: &str) -> [f64; 3] {
    let byte = |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
    [byte(1), byte(3), byte(5)]
}

/// The test's folder and a Bambu Studio data folder in it. System presets
/// of a vendor Acme: `fdm_filament_common` (density, temperatures, a
/// plate temperature) ← `Acme PLA Basic @base` (type, colour `#FF8000`,
/// nozzle 220) ← `Acme PLA Basic @Acme X` (the plate 60), and `Acme PETG`
/// (no colour, an empty default colour). The user's: `My PLA` (inherits
/// the PLA, its own first layer), `My Lost PLA` (inherits a preset that is
/// not there), `My PETG` and `My Green PETG` (with a default colour).
fn slicer_folder(name: &str) -> (PathBuf, PathBuf) {
    let dir = mitcad_model::testing::unique_temp_dir(&format!("colour-{name}"));
    let slicer = dir.join("BambuStudio");
    let system = slicer.join("system").join("Acme").join("filament");
    write(
        &system.join("fdm_filament_common.json"),
        &json!({"type": "filament", "name": "fdm_filament_common", "instantiation": "false",
                "filament_density": ["1.24"], "nozzle_temperature": ["200"],
                "nozzle_temperature_initial_layer": ["205"], "hot_plate_temp": ["55"]}),
    );
    write(
        &system.join("Acme PLA Basic @base.json"),
        &json!({"type": "filament", "name": "Acme PLA Basic @base", "instantiation": "false",
                "inherits": "fdm_filament_common", "filament_type": ["PLA"],
                "filament_colour": ["#FF8000"], "nozzle_temperature": ["220"]}),
    );
    write(
        &system.join("Acme PLA Basic @Acme X.json"),
        &json!({"type": "filament", "name": "Acme PLA Basic @Acme X", "instantiation": "true",
                "inherits": "Acme PLA Basic @base", "hot_plate_temp": ["60"]}),
    );
    write(
        &system.join("Acme PETG.json"),
        &json!({"type": "filament", "name": "Acme PETG", "instantiation": "true",
                "inherits": "fdm_filament_common", "filament_type": ["PETG"],
                "default_filament_colour": [""], "nozzle_temperature": ["240"]}),
    );
    let user = slicer.join("user").join("default").join("filament");
    write(
        &user.join("My PLA.json"),
        &json!({"name": "My PLA", "inherits": "Acme PLA Basic @Acme X", "from": "User",
                "nozzle_temperature_initial_layer": ["215"]}),
    );
    write(
        &user.join("My Lost PLA.json"),
        &json!({"name": "My Lost PLA", "inherits": "Gone PLA", "from": "User",
                "filament_type": ["PLA"], "nozzle_temperature": ["230"]}),
    );
    write(
        &user.join("My PETG.json"),
        &json!({"name": "My PETG", "inherits": "Acme PETG", "from": "User",
                "nozzle_temperature": ["245"]}),
    );
    write(
        &user.join("My Green PETG.json"),
        &json!({"name": "My Green PETG", "inherits": "Acme PETG", "from": "User",
                "default_filament_colour": ["#00AE42"]}),
    );
    (dir, slicer)
}

/// A user filament preset's file.
fn preset(slicer: &Path, name: &str) -> PathBuf {
    slicer
        .join("user")
        .join("default")
        .join("filament")
        .join(format!("{name}.json"))
}

/// `import_slicer_presets` of the user's filament presets `names`.
fn import(
    doc: &mut Document<MockKernel>,
    slicer: &Path,
    into: &str,
    project: &Path,
    names: &[&str],
    same_name: &str,
) -> Value {
    let presets: Vec<Value> = names
        .iter()
        .map(|n| json!({"path": preset(slicer, n), "same_name": same_name}))
        .collect();
    command(
        doc,
        json!({"cmd": "import_slicer_presets", "dialect": "bambu_studio", "folder": slicer,
               "into": into, "project": project, "presets": presets}),
    )
}

/// The design's material of an import's answer's `materials[index]`, and
/// its appearance's colour.
fn material_and_colour(
    doc: &Document<MockKernel>,
    result: &Value,
    index: usize,
) -> (mitcad_model::material::Material, [f64; 3]) {
    let id = result["materials"][index]["id"].as_str().unwrap();
    let material = doc.material(id).unwrap().clone();
    let look = material
        .appearance
        .as_deref()
        .unwrap_or_else(|| panic!("{id} has no appearance"));
    let colour = doc.appearance(look).unwrap().base_color;
    (material, colour)
}

fn log(result: &Value) -> Vec<String> {
    result["report"]["log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_user_preset_takes_its_colour_and_temperatures_from_the_presets_it_inherits() {
    let (dir, slicer) = slicer_folder("chain");
    let project = dir.join("project");
    let mut doc = Document::new(MockKernel::default());
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        &["My PLA", "My Lost PLA"],
        "copy",
    );
    assert_eq!(result["materials"][0]["name"], "My PLA");
    let (pla, colour) = material_and_colour(&doc, &result, 0);
    // Every value of the chain, the nearest winning.
    assert_eq!(
        pla.filament["temperature"], 220.0,
        "from Acme PLA Basic @base"
    );
    assert_eq!(pla.filament["first_layer_temperature"], 215.0, "its own");
    assert_eq!(pla.filament["hot_plate_temp"], 60.0, "from @Acme X");
    assert_eq!(pla.density, 1.24, "from fdm_filament_common");
    assert_eq!(pla.based_on.as_deref(), Some("pla"));
    // Its colour is its appearance's: the system preset's colour.
    assert_eq!(colour, rgb("#FF8000"));
    assert!(
        !pla.filament.contains_key("filament_colour"),
        "the material keeps no colour of its own: {:?}",
        pla.filament
    );

    // A missing base: its own values, the neutral appearance, and the
    // values it inherits reported as not read.
    let (lost, colour) = material_and_colour(&doc, &result, 1);
    assert_eq!(lost.name, "My Lost PLA");
    assert_eq!(lost.filament["temperature"], 230.0);
    assert_eq!(colour, NEUTRAL_FILAMENT_COLOR);
    let left_out = result["report"]["left_out"].as_array().unwrap();
    assert!(
        left_out
            .iter()
            .any(|l| l.as_str().unwrap().contains("system preset Gone PLA")),
        "{left_out:?}"
    );
    assert_eq!(
        log(&result),
        vec![
            "Slicer import: My PLA inherits Acme PLA Basic @Acme X > Acme PLA Basic @base > \
             fdm_filament_common, colour #FF8000",
            "Slicer import: My Lost PLA inherits Gone PLA (missing), colour none",
        ]
    );
}

#[test]
fn a_filament_with_no_colour_anywhere_gets_the_neutral_appearance() {
    let (dir, slicer) = slicer_folder("neutral");
    let project = dir.join("project");
    let mut doc = Document::new(MockKernel::default());
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        &["My PETG", "My Green PETG"],
        "copy",
    );
    let (petg, colour) = material_and_colour(&doc, &result, 0);
    assert_eq!(petg.name, "My PETG");
    assert_eq!(petg.filament["temperature"], 245.0);
    assert_eq!(colour, NEUTRAL_FILAMENT_COLOR, "never no appearance");
    let look = doc.appearance(petg.appearance.as_deref().unwrap()).unwrap();
    assert_eq!(look.name, "My PETG color", "named for the material");
    // The family's default colour when the chain has no filament colour.
    let (green, colour) = material_and_colour(&doc, &result, 1);
    assert_eq!(green.name, "My Green PETG");
    assert_eq!(green.filament["temperature"], 240.0, "from Acme PETG");
    assert_eq!(colour, rgb("#00AE42"));
    assert_eq!(
        log(&result),
        vec![
            "Slicer import: My PETG inherits Acme PETG > fdm_filament_common, colour none",
            "Slicer import: My Green PETG inherits Acme PETG > fdm_filament_common, \
             colour #00AE42",
        ]
    );
}

#[test]
fn a_preset_file_in_a_slicer_folder_is_imported_with_its_chain() {
    // Import from Slicer... with the user preset's own file: the values it
    // inherits come from the data folder it lies in.
    let (_dir, slicer) = slicer_folder("file");
    let mut doc = Document::new(MockKernel::default());
    let result = command(
        &mut doc,
        json!({"cmd": "import_print_profile", "path": preset(&slicer, "My PLA")}),
    );
    let (pla, colour) = material_and_colour(&doc, &result, 0);
    assert_eq!(pla.filament["temperature"], 220.0);
    assert_eq!(pla.filament["hot_plate_temp"], 60.0);
    assert_eq!(colour, rgb("#FF8000"));
    let left_out = result["report"]["left_out"].as_array().unwrap();
    assert!(
        !left_out
            .iter()
            .any(|l| l.as_str().unwrap().contains("system preset")),
        "nothing left to the system presets: {left_out:?}"
    );
    assert_eq!(
        log(&result),
        vec![
            "Slicer import: My PLA inherits Acme PLA Basic @Acme X > Acme PLA Basic @base > \
             fdm_filament_common, colour #FF8000"
        ]
    );
}

#[test]
fn an_override_takes_the_new_colour_into_the_design_and_the_library() {
    let (dir, slicer) = slicer_folder("override");
    let project = dir.join("project");
    let base = slicer
        .join("system")
        .join("Acme")
        .join("filament")
        .join("Acme PLA Basic @base.json");

    // The design: imported, then the slicer's colour changes and the
    // preset overrides the design's material.
    let mut doc = Document::new(MockKernel::default());
    let first = import(&mut doc, &slicer, "design", &project, &["My PLA"], "copy");
    let id = first["materials"][0]["id"].as_str().unwrap().to_owned();
    write(
        &base,
        &json!({"type": "filament", "name": "Acme PLA Basic @base", "instantiation": "false",
                "inherits": "fdm_filament_common", "filament_type": ["PLA"],
                "filament_colour": ["#0000FF"], "nozzle_temperature": ["225"]}),
    );
    let result = import(
        &mut doc,
        &slicer,
        "design",
        &project,
        &["My PLA"],
        "override",
    );
    assert_eq!(result["overridden"][0]["id"], id.as_str());
    let material = doc.material(&id).unwrap();
    assert_eq!(material.filament["temperature"], 225.0);
    let look = doc
        .appearance(material.appearance.as_deref().unwrap())
        .unwrap();
    assert_eq!(look.base_color, rgb("#0000FF"));

    // The project's library: version 1 orange; overridden, version 2
    // carries the blue appearance.
    write(
        &base,
        &json!({"type": "filament", "name": "Acme PLA Basic @base", "instantiation": "false",
                "inherits": "fdm_filament_common", "filament_type": ["PLA"],
                "filament_colour": ["#FF8000"], "nozzle_temperature": ["220"]}),
    );
    let mut doc = Document::new(MockKernel::default());
    import(&mut doc, &slicer, "project", &project, &["My PLA"], "copy");
    write(
        &base,
        &json!({"type": "filament", "name": "Acme PLA Basic @base", "instantiation": "false",
                "inherits": "fdm_filament_common", "filament_type": ["PLA"],
                "filament_colour": ["#0000FF"], "nozzle_temperature": ["225"]}),
    );
    let result = import(
        &mut doc,
        &slicer,
        "project",
        &project,
        &["My PLA"],
        "override",
    );
    assert_eq!(result["overridden"][0]["version"], 2);
    let item = Libraries::load(Vec::new(), None, Some(project.as_path()))
        .items
        .into_iter()
        .find(|i| {
            i.library == LibraryKind::Project
                && i.data.kind() == ItemKind::Material
                && i.data.name() == "My PLA"
        })
        .unwrap()
        .data;
    let ItemData::Material {
        material,
        appearances,
    } = item
    else {
        panic!("a material");
    };
    assert_eq!(material.version, Some(2));
    assert_eq!(material.filament["temperature"], 225.0);
    let appearance =
        mitcad_model::print_library::carried(&appearances, material.appearance.as_deref())
            .expect("the file carries its appearance");
    assert_eq!(material.appearance.as_deref(), Some(appearance.id.as_str()));
    assert_eq!(appearance.base_color, rgb("#0000FF"));
}

#[test]
fn the_slicer_gets_the_imported_filament_s_appearance_colour() {
    // Bound together: the colour written for the slot is the appearance's.
    let (dir, slicer) = slicer_folder("export");
    let project = dir.join("project");
    let mut doc = design();
    let result = import(&mut doc, &slicer, "design", &project, &["My PLA"], "copy");
    let id = result["materials"][0]["id"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Arm", "material": id}),
    );
    let (_, data) = export_to(&mut doc, "colour.3mf", "bambu_studio");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let settings: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(settings["filament_settings_id"][1], "My PLA");
    assert_eq!(settings["filament_colour"][1], "#FF8000");
    assert_eq!(settings["nozzle_temperature"][1], "220");
}
