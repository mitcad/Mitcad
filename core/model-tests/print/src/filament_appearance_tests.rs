// SPDX-License-Identifier: MIT
//! A filament's colour is its appearance (mitcad#1055): a material's list
//! of appearances, a body's material and one of its appearances, the
//! export's filament slot per material and appearance (read back from the
//! 3MF), a material's own `filament_colour` of a design from before loaded
//! as an appearance of its list, the Appearance command and
//! `appearance_id` growing the list, and a component's appearance.

use serde_json::{Map, Value, json};

use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

use super::print_files_tests::{command, export_to, small_block};

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn material(doc: &Document<MockKernel>, id: &str) -> Value {
    query(doc, json!({"query": "materials"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no material {id}"))
}

fn body(doc: &Document<MockKernel>, uid: BodyUid) -> Value {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == uid.to_string())
        .cloned()
        .unwrap()
}

/// `#RRGGBB` of an appearance as the `appearances` query gives its colour.
fn hex(doc: &Document<MockKernel>, id: &str) -> String {
    let look = query(doc, json!({"query": "appearances"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no appearance {id}"));
    let c: Vec<u8> = look["display_color"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v.as_f64().unwrap().clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// Two blocks of the root, a bench printer of four slots and a standard
/// process, two appearances of the design's own and PLA Basic offering
/// three: Bright Green (its default), Plastic - Black and Translucent Blue.
fn design() -> (Document<MockKernel>, BodyUid, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let first = small_block(&mut doc);
    let second = small_block(&mut doc);
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
             "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]],
                        "max_print_height": 250, "nozzle_diameter": 0.4, "filament_slots": 4}},
            {"cmd": "create_print_profile", "kind": "process", "name": "Standard",
             "values": {"layer_height": 0.2}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
             "value": "bench"},
            {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile",
             "value": "standard"},
            {"cmd": "create_appearance", "id": "bright_green", "name": "Bright Green",
             "base_color": [0.0, 0.8, 0.2]},
            {"cmd": "create_appearance", "id": "translucent_blue", "name": "Translucent Blue",
             "base_color": [0.2, 0.4, 1.0], "transmission": 0.6},
            {"cmd": "create_material", "id": "pla_basic", "name": "PLA Basic", "density": 1.24,
             "class": "plastic",
             "appearances": ["bright_green", "plastic_black", "translucent_blue"],
             "filament": {"filament_type": "PLA", "temperature": 215}},
        ]),
    );
    (doc, first, second)
}

#[test]
fn two_colours_of_one_material_are_two_slots_of_the_same_values() {
    let (mut doc, first, second) = design();
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["appearance"], "bright_green");
    assert_eq!(
        pla["appearances"],
        json!(["bright_green", "plastic_black", "translucent_blue"])
    );
    // The design file keeps the others besides the default.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(
        file["materials"][0]["appearances"],
        json!(["plastic_black", "translucent_blue"])
    );

    // Material ▸ PLA Basic ▸ Plastic - Black and ▸ Translucent Blue.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "pla_basic",
               "appearance_id": "plastic_black"}),
    );
    assert_eq!(undo_label(&doc), "Set Material of Body1");
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": second.to_string(), "material": "pla_basic",
               "appearance_id": "translucent_blue"}),
    );
    assert_eq!(body(&doc, first)["appearance"], "plastic_black");
    assert_eq!(body(&doc, second)["appearance"], "translucent_blue");
    // Of the material's list: not set by hand.
    assert_eq!(body(&doc, first).get("appearance_set_by_hand"), None);
    let black = hex(&doc, "plastic_black");
    let blue = hex(&doc, "translucent_blue");
    assert_eq!(black, "#34363A");
    assert_eq!(blue, "#3366FF");
    let settings = query(
        &doc,
        json!({"query": "print_settings", "body": second.to_string()}),
    );
    assert_eq!(settings["filament"]["appearance"], "translucent_blue");

    // Two filament slots, the same values, each its appearance's colour.
    let (result, data) = export_to(&mut doc, "colours.3mf", "bambu_studio");
    assert_eq!(result["filaments"], json!([1, 2]), "{result}");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let written: Map<String, Value> =
        serde_json::from_slice(package.file("Metadata/project_settings.config").unwrap()).unwrap();
    assert_eq!(written["filament_colour"], json!([black, blue]));
    assert_eq!(written["filament_type"], json!(["PLA", "PLA"]));
    assert_eq!(written["filament_density"], json!(["1.24", "1.24"]));
    assert_eq!(written["nozzle_temperature"], json!(["215", "215"]));
    let model = std::str::from_utf8(package.file("Metadata/model_settings.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        model.contains("<metadata key=\"extruder\" value=\"1\"/>"),
        "{model}"
    );
    assert!(model.contains("<metadata key=\"extruder\" value=\"2\"/>"));

    // PrusaSlicer's list too.
    let (_, data) = export_to(&mut doc, "colours-prusa.3mf", "prusaslicer");
    let package = mitcad_3mf::read_package(&data).unwrap();
    let config = std::str::from_utf8(package.file("Metadata/Slic3r_PE.config").unwrap())
        .unwrap()
        .to_owned();
    assert!(
        config.contains(&format!("; filament_colour = {black};{blue}\n")),
        "{config}"
    );

    // The purge table has the same slots, named with their colours; the
    // volume between them is their colours'.
    let purge = query(&doc, json!({"query": "purge_volumes"}));
    let names: Vec<&str> = purge["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "PLA Basic (Plastic - Black)",
            "PLA Basic (Translucent Blue)"
        ]
    );
    assert!(purge["volumes"][0][1]["computed"].as_f64().unwrap() > 0.0);
    assert_eq!(purge["volumes"][0][1]["edited"], false);

    // One colour for both: one slot.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": second.to_string(), "material": "pla_basic",
               "appearance_id": "plastic_black"}),
    );
    let (result, _) = export_to(&mut doc, "one.3mf", "bambu_studio");
    assert_eq!(result["filaments"], json!([1, 1]), "{result}");
}

#[test]
fn an_old_design_s_filament_colour_loads_as_an_appearance_of_its_list() {
    let mut doc = Document::new(MockKernel::default());
    let uid = small_block(&mut doc);
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    // A design from before: materials with a colour of their own.
    file["materials"] = json!([
        {"id": "old_pla", "name": "Old PLA", "density": 1.24, "appearance": "plastic_white",
         "filament": {"filament_type": "PLA", "filament_colour": "#FF8000"}, "class": "plastic"},
        {"id": "plain_petg", "name": "Plain PETG", "density": 1.27,
         "filament": {"filament_type": "PETG", "filament_colour": "#00FF00"}, "class": "plastic"},
        {"id": "white_abs", "name": "White ABS", "density": 1.06, "appearance": "plastic_white",
         "filament": {"filament_type": "ABS", "filament_colour": "#eeeeea"}, "class": "plastic"},
    ]);
    let mut loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    command(&mut loaded, json!({"cmd": "recompute"}));

    // Its colour is an appearance of its list, the default staying.
    let old = material(&loaded, "old_pla");
    assert_eq!(old["appearance"], "plastic_white");
    assert_eq!(
        old["appearances"],
        json!(["plastic_white", "old_pla_colour"])
    );
    assert_eq!(old["filament"].get("filament_colour"), None);
    assert_eq!(hex(&loaded, "old_pla_colour"), "#FF8000");
    // Without an appearance: the colour's is the default.
    let plain = material(&loaded, "plain_petg");
    assert_eq!(plain["appearance"], "plain_petg_colour");
    assert_eq!(plain["appearances"], json!(["plain_petg_colour"]));
    assert_eq!(hex(&loaded, "plain_petg_colour"), "#00FF00");
    // A colour its appearance has already: nothing new.
    let white = material(&loaded, "white_abs");
    assert_eq!(white["appearances"], json!(["plastic_white"]));
    assert_eq!(white["filament"].get("filament_colour"), None);

    // A body made of it prints with its appearance; Old PLA ▸ its colour
    // gives the old colour.
    command(
        &mut loaded,
        json!({"cmd": "set_body_material", "uid": uid.to_string(), "material": "old_pla"}),
    );
    let settings = query(
        &loaded,
        json!({"query": "print_settings", "body": uid.to_string()}),
    );
    assert_eq!(settings["filament"]["appearance"], "plastic_white");
    command(
        &mut loaded,
        json!({"cmd": "set_body_material", "uid": uid.to_string(), "material": "old_pla",
               "appearance_id": "old_pla_colour"}),
    );
    let effective = loaded.effective_print(&mitcad_model::PrintTarget::Body(uid, None));
    assert_eq!(effective.filament["filament_colour"].0, "#FF8000");

    // Saved without a colour of its own.
    let saved: Value = serde_json::from_str(&loaded.to_json()).unwrap();
    assert!(
        saved["materials"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["filament"].get("filament_colour").is_none()),
        "{}",
        saved["materials"]
    );
}

#[test]
fn a_material_s_list_is_edited_and_a_colour_set_joins_it() {
    let (mut doc, _, _) = design();
    // Another default of its list: the old one stays in the list.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic", "appearance": "plastic_black"}),
    );
    assert_eq!(undo_label(&doc), "Edit Material PLA Basic");
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["appearance"], "plastic_black");
    assert_eq!(
        pla["appearances"],
        json!(["plastic_black", "bright_green", "translucent_blue"])
    );
    // The list in its place: the default kept while it is there.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic",
               "appearances": ["translucent_blue", "plastic_black", "plastic_white"]}),
    );
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["appearance"], "plastic_black");
    assert_eq!(
        pla["appearances"],
        json!(["plastic_black", "translucent_blue", "plastic_white"])
    );
    // The default removed: the first is the default.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic",
               "appearances": ["plastic_white", "translucent_blue"]}),
    );
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["appearance"], "plastic_white");
    assert_eq!(
        pla["appearances"],
        json!(["plastic_white", "translucent_blue"])
    );
    // Another appearance with the list: the default, the list keeping all.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic", "appearance": "bright_green",
               "appearances": ["plastic_white", "translucent_blue"]}),
    );
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["appearance"], "bright_green");
    assert_eq!(
        pla["appearances"],
        json!(["bright_green", "plastic_white", "translucent_blue"])
    );
    // Refused: an appearance that does not exist.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic", "appearances": ["nowhere"]}),
    );
    assert!(error.contains("there is no appearance nowhere"), "{error}");
    // A colour set by a command is the default: what the user wants
    // printed; the old default stays in the list.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic",
               "filament": {"filament_colour": "#FF0000"}}),
    );
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["filament"].get("filament_colour"), None);
    assert_eq!(pla["appearance"], "pla_basic_colour");
    assert_eq!(
        pla["appearances"],
        json!([
            "pla_basic_colour",
            "plastic_white",
            "translucent_blue",
            "bright_green"
        ])
    );
    assert_eq!(hex(&doc, "pla_basic_colour"), "#FF0000");
    // The list empty: no appearance.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla_basic", "appearances": []}),
    );
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla.get("appearance"), None);
    assert_eq!(pla["appearances"], json!([]));
}

#[test]
fn an_appearance_outside_the_list_joins_it_for_the_design() {
    let (mut doc, first, second) = design();
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "pla_basic"}),
    );
    assert_eq!(body(&doc, first)["appearance"], "bright_green");
    // The Appearance command: Paint - Red joins PLA Basic's list in the
    // same undo step, named after the body.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": first.to_string(), "appearance": "paint_red"}),
    );
    assert_eq!(undo_label(&doc), "Set Appearance of Body1");
    assert_eq!(
        material(&doc, "pla_basic")["appearances"],
        json!([
            "bright_green",
            "plastic_black",
            "translucent_blue",
            "paint_red"
        ])
    );
    assert_eq!(body(&doc, first).get("appearance_set_by_hand"), None);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(body(&doc, first)["appearance"], "bright_green");
    assert_eq!(
        material(&doc, "pla_basic")["appearances"],
        json!(["bright_green", "plastic_black", "translucent_blue"])
    );
    // appearance_id outside the list: the same.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "pla_basic",
               "appearance_id": "plastic_orange"}),
    );
    assert!(
        material(&doc, "pla_basic")["appearances"]
            .as_array()
            .unwrap()
            .contains(&json!("plastic_orange"))
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "pla_basic",
               "appearance_id": "plastic_black", "appearance": "keep"}),
    );
    assert!(error.contains("appearance_id"), "{error}");

    // A built-in material cannot change: its list stays, the appearance is
    // set by hand.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": second.to_string(), "material": "pla"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": second.to_string(),
               "appearance": "paint_red"}),
    );
    assert_eq!(
        material(&doc, "pla")["appearances"],
        json!(["plastic_white"])
    );
    assert_eq!(body(&doc, second)["appearance_set_by_hand"], true);

    // A new material that offers the body's appearance keeps it; one that
    // does not gives its default.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "pla_basic",
               "appearance_id": "plastic_black"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "petg_basic", "name": "PETG Basic",
               "density": 1.27, "class": "plastic",
               "appearances": ["plastic_white", "plastic_black"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "petg_basic",
               "appearance": "replace"}),
    );
    assert_eq!(body(&doc, first)["appearance"], "plastic_black");
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "abs"}),
    );
    assert_eq!(body(&doc, first)["appearance"], "plastic_black");
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": first.to_string(), "material": "pla",
               "appearance": "replace"}),
    );
    assert_eq!(body(&doc, first)["appearance"], "plastic_white");
}

#[test]
fn a_component_s_appearance_is_what_its_bodies_print_with() {
    let (mut doc, first, _) = design();
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": "pla_basic",
               "appearance_id": "translucent_blue"}),
    );
    assert_eq!(undo_label(&doc), "Set Default Material of Root");
    let pla = material(&doc, "pla_basic");
    assert_eq!(pla["components"], json!(["C0"]));
    assert_eq!(
        pla["component_appearances"],
        json!({"C0": "translucent_blue"})
    );
    // A body without an appearance of its own prints with it.
    let settings = query(
        &doc,
        json!({"query": "print_settings", "body": first.to_string()}),
    );
    assert_eq!(settings["filament"]["appearance"], "translucent_blue");
    let effective = doc.effective_print(&mitcad_model::PrintTarget::Body(first, None));
    assert_eq!(effective.filament["filament_colour"].0, "#3366FF");
    // The design file keeps it with the component's material.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["component_materials"],
        json!([{"component": "C0", "material": "pla_basic", "appearance": "translucent_blue"}])
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(
        loaded.component_appearance(mitcad_model::ids::ComponentUid::ROOT),
        Some("translucent_blue")
    );
    // The material again without one: its default.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": "pla_basic"}),
    );
    let settings = query(
        &doc,
        json!({"query": "print_settings", "body": first.to_string()}),
    );
    assert_eq!(settings["filament"]["appearance"], "bright_green");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": null,
               "appearance_id": "plastic_black"}),
    );
    assert!(error.contains("without a material"), "{error}");
}

/// A colour given to `create_material` is the material's default
/// appearance unless the command names one (the purge table's white and
/// black filaments, `tools/ui-purge-volumes-test.sh`): its bodies print
/// with it, and the purge volumes are those colours'.
#[test]
fn a_colour_given_at_create_material_is_the_default_appearance() {
    let (mut doc, first, second) = design();
    command(
        &mut doc,
        json!([
            {"cmd": "create_material", "id": "white-pla", "name": "White PLA", "based_on": "pla",
             "filament": {"filament_colour": "#FFFFFF"}},
            {"cmd": "create_material", "id": "black-petg", "name": "Black PETG", "based_on": "pla",
             "filament": {"filament_type": "PETG", "filament_colour": "#000000"}},
            {"cmd": "create_material", "id": "named", "name": "Named", "based_on": "pla",
             "appearance": "plastic_orange", "filament": {"filament_colour": "#0000FF"}},
            {"cmd": "set_component_material", "component": "C0", "material": "white-pla"},
            {"cmd": "set_body_material", "uid": second.to_string(), "material": "black-petg"},
        ]),
    );
    let white = material(&doc, "white-pla");
    assert_eq!(white["appearance"], "white_pla_colour");
    assert_eq!(
        white["appearances"],
        json!(["white_pla_colour", "plastic_white"])
    );
    assert_eq!(white["filament"].get("filament_colour"), None);
    assert_eq!(
        material(&doc, "black-petg")["appearance"],
        "black_petg_colour"
    );
    // An appearance the command names stays the default.
    let named = material(&doc, "named");
    assert_eq!(named["appearance"], "plastic_orange");
    assert_eq!(
        named["appearances"],
        json!(["plastic_orange", "named_colour"])
    );
    assert_eq!(body(&doc, second)["appearance"], "black_petg_colour");
    let purge = query(&doc, json!({"query": "purge_volumes"}));
    let colors: Vec<&str> = purge["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["color"].as_str().unwrap())
        .collect();
    assert_eq!(colors, ["#FFFFFF", "#000000"]);
    let white_out = mitcad_model::print_purge::colour_defaults(Some([1.0, 1.0, 1.0])).out;
    let black_in = mitcad_model::print_purge::colour_defaults(Some([0.0, 0.0, 0.0])).into;
    assert_eq!(
        purge["volumes"][0][1]["computed"],
        json!(white_out + black_in)
    );
    let effective = doc.effective_print(&mitcad_model::PrintTarget::Body(first, None));
    assert_eq!(effective.filament["filament_colour"].0, "#FFFFFF");
}

/// A library material keeps its whole list of appearances (mitcad#1067):
/// PLA Basic saved into a library carries its two appearances of the
/// design (Plastic - Black is the library's), and a new design it is
/// inserted into offers the same three, the default first; one there of a
/// carried appearance's id but another look keeps its own, and the
/// carried one comes under a free id in its place of the list.
#[test]
fn a_library_material_brings_its_whole_list_of_appearances() {
    use mitcad_model::print_library::{ItemData, ItemKind, Libraries, LibraryKind};

    let (mut doc, _, _) = design();
    let user = mitcad_model::testing::unique_temp_path("library-appearances");
    let libraries = Libraries::load(Vec::new(), Some(&user), None);
    let (path, version) = doc
        .save_to_print_library(
            &libraries,
            LibraryKind::User,
            ItemKind::Material,
            "pla_basic",
        )
        .unwrap();
    assert_eq!(version, 1);
    // The default's as before, the other of the design's in a list.
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(file["appearance"]["id"], "bright_green");
    assert_eq!(file["appearances"].as_array().unwrap().len(), 1);
    assert_eq!(file["appearances"][0]["id"], "translucent_blue");

    let libraries = Libraries::load(Vec::new(), Some(&user), None);
    assert!(libraries.problems.is_empty(), "{:?}", libraries.problems);
    let item = libraries
        .find(Some(LibraryKind::User), ItemKind::Material, "pla_basic")
        .unwrap();
    let ItemData::Material { appearances, .. } = &item.data else {
        unreachable!()
    };
    let carried: Vec<&str> = appearances.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(carried, ["bright_green", "translucent_blue"]);
    // The lists' row names and colours the carried ones before a design
    // has them.
    let mut fresh = Document::new(MockKernel::default());
    let rows = fresh.library_items_json(&libraries, ItemKind::Material);
    let row = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["place"] == "user" && r["id"] == "pla_basic")
        .cloned()
        .unwrap();
    assert_eq!(
        row["appearances"],
        json!(["bright_green", "plastic_black", "translucent_blue"])
    );
    assert_eq!(row["carried_appearances"][1]["name"], "Translucent Blue");

    // Inserted into a new design: the same three, the default first, with
    // their looks.
    let id = fresh.copy_from_print_library(item).unwrap();
    assert_eq!(id, "pla_basic");
    let pla = material(&fresh, "pla_basic");
    assert_eq!(pla["appearance"], "bright_green");
    assert_eq!(
        pla["appearances"],
        json!(["bright_green", "plastic_black", "translucent_blue"])
    );
    assert_eq!(hex(&fresh, "bright_green"), hex(&doc, "bright_green"));
    assert_eq!(hex(&fresh, "translucent_blue"), "#3366FF");
    assert_eq!(undo_label(&fresh), "Copy PLA Basic from Library");

    // A design with a Translucent Blue of its own keeps it; the carried one
    // comes as translucent_blue_1, in its place of the list.
    let mut other = Document::new(MockKernel::default());
    command(
        &mut other,
        json!({"cmd": "create_appearance", "id": "translucent_blue", "name": "Translucent Blue",
               "base_color": [0.0, 0.0, 0.5]}),
    );
    other.copy_from_print_library(item).unwrap();
    assert_eq!(
        material(&other, "pla_basic")["appearances"],
        json!(["bright_green", "plastic_black", "translucent_blue_1"])
    );
    assert_eq!(hex(&other, "translucent_blue_1"), "#3366FF");
    assert_ne!(hex(&other, "translucent_blue"), "#3366FF");
    // A newer version of the same values in the library is no change to
    // offer to either design (the renamed one counts as the same).
    let mut newer = item.data.clone();
    newer.set_version(2);
    std::fs::write(&path, newer.text()).unwrap();
    let libraries = Libraries::load(Vec::new(), Some(&user), None);
    assert!(fresh.print_library_updates(&libraries).is_empty());
    assert!(other.print_library_updates(&libraries).is_empty());
}

/// A component's appearance that neither the design nor the library has,
/// in a file edited by hand (mitcad#1150): the file opens with a warning,
/// the component without that appearance, its material kept.
#[test]
fn an_unknown_component_appearance_is_dropped_with_a_warning() {
    let (mut doc, _, _) = design();
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": "pla_basic",
               "appearance_id": "translucent_blue"}),
    );
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    file["component_materials"][0]["appearance"] = json!("no_such_look");
    let loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    let root = mitcad_model::ids::ComponentUid::ROOT;
    assert_eq!(loaded.component_appearance(root), None);
    assert_eq!(material(&loaded, "pla_basic")["components"], json!(["C0"]));
    assert!(
        loaded
            .load_warnings()
            .iter()
            .any(|w| w.contains("there is no appearance no_such_look")),
        "{:?}",
        loaded.load_warnings()
    );
    // Saved again, the file no longer names it.
    let saved: Value = serde_json::from_str(&loaded.to_json()).unwrap();
    assert_eq!(
        saved["component_materials"],
        json!([{"component": "C0", "material": "pla_basic"}])
    );
}
