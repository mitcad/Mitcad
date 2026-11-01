// SPDX-License-Identifier: MIT
//! Materials as objects (mitcad#640, part 2) with the mock kernel: the
//! built-in ids kept, the document's own materials made, edited and
//! deleted as undo steps, a body's material setting its appearance (or
//! keeping a hand-set one), components' default materials up the
//! occurrence path, the design file and the comparison of versions.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, MaterialSource};
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

fn body(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "bodies"}))[0].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn material(doc: &Document<MockKernel>, id: &str) -> Option<Value> {
    query(doc, json!({"query": "materials"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
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

#[test]
fn built_in_ids_stay_and_bring_their_looks() {
    let mut doc = block();
    let pla = material(&doc, "pla").unwrap();
    assert_eq!(pla["built_in"], true);
    assert_eq!(pla["density"], 1.24);
    assert_eq!(pla["appearance"], "plastic_white");
    assert_eq!(pla["filament"], json!({"filament_type": "PLA"}));
    assert_eq!(pla["bodies"], json!([]));
    // Every built-in id is listed once, in the old order.
    let ids: Vec<String> = query(&doc, json!({"query": "materials"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids[0], "steel");
    assert!(ids.contains(&"water".to_owned()));

    // Setting the material gives the body the material's appearance.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "pla"}),
    );
    assert_eq!(undo_label(&doc), "Set Material of Body1");
    assert_eq!(
        body(&doc),
        json!({"uid": "F2.b0", "name": "Body1", "material": "pla", "appearance": "plastic_white"})
    );
    assert_eq!(material(&doc, "pla").unwrap()["bodies"], json!(["F2.b0"]));
    // Its density gives the mass (the mock kernel measures the box).
    doc.kernel().measures.set(true);
    doc.kernel().physical.set(true);
    let properties = query(&doc, json!({"query": "properties"}));
    assert_eq!(properties["bodies"][0]["material"], "pla");
    assert_eq!(properties["bodies"][0]["density"], 1.24);
    // One undo step takes both away.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(body(&doc), json!({"uid": "F2.b0", "name": "Body1"}));
}

#[test]
fn a_hand_set_appearance_is_kept_unless_replaced() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"}),
    );
    assert_eq!(body(&doc)["appearance_set_by_hand"], true);
    // What the application asks about before replacing it.
    let conflicts = query(
        &doc,
        json!({"query": "material_conflicts", "bodies": ["F2.b0"], "material": "abs"}),
    );
    assert_eq!(
        conflicts["bodies"],
        json!([{"uid": "F2.b0", "name": "Body1", "appearance": "paint_red"}])
    );
    // Left out: a hand-set appearance stays.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs"}),
    );
    assert_eq!(body(&doc)["appearance"], "paint_red");
    assert_eq!(body(&doc)["material"], "abs");
    // replace: the material's.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs",
               "appearance": "replace"}),
    );
    assert_eq!(body(&doc)["appearance"], "plastic_black");
    assert_eq!(body(&doc).get("appearance_set_by_hand"), None);
    let conflicts = query(
        &doc,
        json!({"query": "material_conflicts", "bodies": ["F2.b0"], "material": "pla"}),
    );
    assert_eq!(conflicts["bodies"], json!([]));
    // An appearance that is its material's follows the next material.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "pla"}),
    );
    assert_eq!(body(&doc)["appearance"], "plastic_white");
    // keep: the body's stays.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum",
               "appearance": "keep"}),
    );
    assert_eq!(body(&doc)["appearance"], "plastic_white");
    assert_eq!(body(&doc)["appearance_set_by_hand"], true);
    // Back to the default material: steel's look.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": null,
               "appearance": "replace"}),
    );
    assert_eq!(body(&doc).get("material"), None);
    assert_eq!(body(&doc)["appearance"], "steel_satin");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs",
               "appearance": "maybe"}),
    );
    assert!(error.contains("\"replace\" or \"keep\""), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "cheese"}),
    );
    assert!(
        error.contains("unknown material cheese (known: steel,"),
        "{error}"
    );
    // The model's own call keeps the appearance (imports set both).
    doc.set_body_material(BodyUid::new(mitcad_model::FeatureUid(2), 0), Some("glass"))
        .unwrap();
    assert_eq!(body(&doc)["appearance"], "steel_satin");
}

#[test]
fn document_materials_are_made_edited_and_deleted() {
    let mut doc = block();
    let id = command(
        &mut doc,
        json!({"cmd": "create_material", "id": "acme/petg", "name": "Acme PETG",
               "density": 1.27, "appearance": "plastic_orange",
               "filament": {"filament_type": "PETG", "temperature": 240,
                            "filament_colour": "#ff6600",
                            "filament_settings_id": {"prusaslicer": "Acme PETG"}}}),
    )["id"]
        .clone();
    assert_eq!(id, "acme/petg");
    assert_eq!(undo_label(&doc), "Create Material Acme PETG");
    let petg = material(&doc, "acme/petg").unwrap();
    assert_eq!(petg["built_in"], false);
    assert_eq!(petg["filament"]["temperature"], 240);
    assert!(petg["display_color"].is_array());

    // Refused: other sets' keys, the density as a filament value, an
    // appearance that does not exist, a name or id taken, no density.
    for (bad, expected) in [
        (
            json!({"cmd": "create_material", "filament": {"layer_height": 0.2}}),
            "no key of the Print Filament set",
        ),
        (
            json!({"cmd": "create_material", "filament": {"filament_density": 1.2}}),
            "the material's density",
        ),
        (
            json!({"cmd": "create_material", "appearance": "nowhere"}),
            "there is no appearance nowhere",
        ),
        (
            json!({"cmd": "create_material", "name": "Acme PETG"}),
            "a material is named Acme PETG",
        ),
        (
            json!({"cmd": "create_material", "name": "Nylon"}),
            "a material is named Nylon",
        ),
        (
            json!({"cmd": "create_material", "id": "acme/petg"}),
            "a material acme/petg exists",
        ),
        (
            json!({"cmd": "create_material", "density": 0}),
            "greater than zero",
        ),
        (
            json!({"cmd": "create_material", "id": "a b"}),
            "is no material id",
        ),
        (
            json!({"cmd": "create_material", "filament": {"temperature": "hot"}}),
            "temperature",
        ),
    ] {
        let error = refused(&mut doc, bad);
        assert!(error.contains(expected), "{error}");
    }

    // A body made of it; editing it changes the material, not the body.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "acme/petg"}),
    );
    assert_eq!(body(&doc)["appearance"], "plastic_orange");
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "acme/petg", "density": 1.28,
               "filament": {"temperature": 245, "filament_colour": null}}),
    );
    assert_eq!(undo_label(&doc), "Edit Material Acme PETG");
    let petg = material(&doc, "acme/petg").unwrap();
    assert_eq!(petg["density"], 1.28);
    assert_eq!(petg["filament"]["temperature"], 245);
    assert_eq!(petg["filament"].get("filament_colour"), None);
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": "acme/petg", "density": 1.28}),
    );
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo_depth"],
        depth
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_material", "id": "pla", "density": 2}),
    );
    assert!(error.contains("built-in material"), "{error}");

    // A copy of a built-in starts from its fields.
    let copy = command(
        &mut doc,
        json!({"cmd": "create_material", "based_on": "pla", "name": "My PLA"}),
    )["id"]
        .clone();
    let mine = material(&doc, copy.as_str().unwrap()).unwrap();
    assert_eq!(mine["density"], 1.24);
    assert_eq!(mine["based_on"], "pla");
    assert_eq!(mine["appearance"], "plastic_white");

    // Deleting it: the body has no material of its own any more.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_material", "id": "acme/petg"}),
    );
    assert_eq!(deleted["bodies"], json!(["F2.b0"]));
    assert_eq!(undo_label(&doc), "Delete Material Acme PETG");
    assert_eq!(body(&doc).get("material"), None);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(body(&doc)["material"], "acme/petg");
}

#[test]
fn a_document_material_takes_a_built_in_ones_place() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "pla", "name": "PLA Plastic",
               "density": 1.25, "filament": {"filament_type": "PLA", "temperature": 210},
               "origin": {"library": "base", "id": "pla", "version": 3}, "version": 3}),
    );
    let all = query(&doc, json!({"query": "materials"}));
    let plas: Vec<&Value> = all
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["id"] == "pla")
        .collect();
    assert_eq!(plas.len(), 1);
    assert_eq!(plas[0]["built_in"], false);
    assert_eq!(plas[0]["replaces_built_in"], true);
    assert_eq!(plas[0]["density"], 1.25);
    assert_eq!(
        plas[0]["origin"],
        json!({"library": "base", "id": "pla", "version": 3})
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "pla"}),
    );
    // Deleting it brings the built-in back for the body.
    command(&mut doc, json!({"cmd": "delete_material", "id": "pla"}));
    assert_eq!(body(&doc)["material"], "pla");
    assert_eq!(material(&doc, "pla").unwrap()["density"], 1.24);
}

#[test]
fn components_give_their_bodies_a_default_material() {
    let mut doc = Document::new(MockKernel::default());
    let (arm, arm1) = doc
        .create_component(Some("Arm"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    let (pin, pin1) = doc
        .create_component(Some("Pin"), Transform::IDENTITY, true)
        .unwrap();
    let pin_body = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let arm2 = doc.copy_occurrence(arm1, None).unwrap();
    let pin_in_arm2 = doc.assembly().find_path("Arm:2/Pin:1").unwrap();
    assert_eq!(pin_in_arm2[0], arm2);

    // Nothing set: steel.
    assert_eq!(doc.body_material(pin_body, None).1, MaterialSource::Default);
    assert_eq!(doc.body_material(pin_body, None).0.id, "steel");

    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": "pla"}),
    );
    assert_eq!(undo_label(&doc), "Set Default Material of Root");
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Arm", "material": "abs"}),
    );
    assert_eq!(undo_label(&doc), "Set Default Material of Arm");
    // Up the occurrence path: the arm's; without a path: the pin's own
    // component, then the root's.
    let path = [arm1, pin1];
    assert_eq!(
        doc.body_material(pin_body, Some(&path)),
        (doc.material("abs").unwrap(), MaterialSource::Component(arm))
    );
    assert_eq!(doc.body_material(pin_body, None).0.id, "pla");
    let answer = query(
        &doc,
        json!({"query": "body_material", "body": pin_body.to_string(),
               "occurrence": "Arm:1/Pin:1"}),
    );
    assert_eq!(answer["id"], "abs");
    assert_eq!(answer["source"], "component");
    assert_eq!(answer["component"], arm.to_string());
    // The pin's own default wins.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": pin.to_string(),
               "material": "nylon"}),
    );
    assert_eq!(doc.body_material(pin_body, Some(&path)).0.id, "nylon");
    // The body's own wins over all.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": pin_body.to_string(), "material": "brass"}),
    );
    let answer = query(
        &doc,
        json!({"query": "body_material", "body": pin_body.to_string()}),
    );
    assert_eq!(answer["id"], "brass");
    assert_eq!(answer["source"], "body");
    // The default of the body's component gives it its appearance when
    // the body's own material goes.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": pin_body.to_string(), "material": null,
               "appearance": "replace"}),
    );
    let bodies = query(&doc, json!({"query": "bodies"}));
    let pin_row = bodies
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == pin_body.to_string())
        .unwrap()
        .clone();
    assert_eq!(pin_row["appearance"], "plastic_white");
    // The materials query lists the components.
    assert_eq!(material(&doc, "abs").unwrap()["components"], json!([arm]));
    // Refused: unknown materials and components.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Arm", "material": "cheese"}),
    );
    assert!(error.contains("unknown material cheese"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Nothing", "material": "abs"}),
    );
    assert!(error.contains("no component Nothing"), "{error}");

    // The design file keeps them; a copy into another design brings them.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["component_materials"],
        json!([{"component": "C0", "material": "pla"},
               {"component": arm, "material": "abs"},
               {"component": pin, "material": "nylon"}])
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.component_material(arm), Some("abs"));
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn materials_survive_the_design_file_and_are_compared() {
    let mut doc = block();
    let plain = doc.to_json();
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "acme/tpu", "name": "Acme TPU",
               "density": 1.21, "filament": {"filament_type": "TPU"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "acme/tpu"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["materials"],
        json!([{"id": "acme/tpu", "name": "Acme TPU", "density": 1.21,
                "filament": {"filament_type": "TPU"}, "class": "plastic"}])
    );
    // A material from before classes is saved as it was: no class is
    // added to it (mitcad#655).
    let mut old_file = file.clone();
    old_file["materials"][0]
        .as_object_mut()
        .unwrap()
        .remove("class");
    let old_text = old_file.to_string();
    let reloaded = Document::from_json(&old_text, MockKernel::default()).unwrap();
    let resaved: Value = serde_json::from_str(&reloaded.to_json()).unwrap();
    assert_eq!(resaved["materials"], old_file["materials"]);
    assert_eq!(file["bodies"][0]["material"], "acme/tpu");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(body(&loaded)["material"], "acme/tpu");
    // Older files have none.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(old.custom_materials().is_empty());

    // The comparison of versions lists them.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.materials.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["Acme TPU (acme/tpu): added"]);
    assert!(diff.summary.contains("+1 material"), "{}", diff.summary);

    // Broken entries are refused with where they are.
    for (field, broken, expected) in [
        (
            "materials",
            json!([{"id": "a", "name": "A", "density": 1}, {"id": "a", "name": "B", "density": 1}]),
            "materials[1]: material a is listed more than once",
        ),
        (
            "materials",
            json!([{"id": "a", "name": "A", "density": -1}]),
            "materials[0]: material a: the density",
        ),
        (
            "component_materials",
            json!([{"component": "C0", "material": "cheese"}]),
            "component_materials[0]: unknown material cheese",
        ),
        (
            "component_materials",
            json!([{"component": "C9", "material": "pla"}]),
            "component_materials[0]: there is no component C9",
        ),
    ] {
        let mut file: Value = serde_json::from_str(&plain).unwrap();
        file[field] = broken;
        let error = Document::from_json(&file.to_string(), MockKernel::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn print_keys_are_listed_for_the_interface() {
    let doc = block();
    let answer = query(&doc, json!({"query": "print_keys", "set": "filament"}));
    let keys = answer["keys"].as_array().unwrap();
    let temperature = keys.iter().find(|k| k["key"] == "temperature").unwrap();
    assert_eq!(temperature["name"], "Nozzle temperature");
    assert_eq!(temperature["unit"], "°C");
    assert!(
        temperature["dialects"].as_array().unwrap().contains(
            &json!({"dialect": "bambu_studio", "key": "nozzle_temperature", "list": true})
        )
    );
    assert!(keys.iter().all(|k| k["set"] == "filament"));
    // A number-or-percentage key says per dialect whether it takes a
    // percentage (mitcad#655): Bambu Studio's line width is in mm only.
    let process = query(&doc, json!({"query": "print_keys", "set": "process"}));
    let width = process["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key"] == "extrusion_width")
        .unwrap();
    let percent = |dialect: &str| {
        width["dialects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["dialect"] == dialect)
            .map(|d| d["percent"].clone())
    };
    assert_eq!(percent("bambu_studio"), Some(json!(false)));
    assert_eq!(percent("orcaslicer"), Some(json!(true)));
    assert_eq!(percent("prusaslicer"), Some(json!(true)));
    assert!(temperature["dialects"][0].get("percent").is_none());
    // Its place in the settings form (mitcad#655).
    assert_eq!(temperature["category"], "filament");
    assert_eq!(temperature["group"], "nozzle_temperature");
    assert_eq!(temperature["advanced"], false);
    let layout = answer["layout"].as_array().unwrap();
    assert_eq!(layout.len(), 1);
    assert_eq!(layout[0]["set"], "filament");
    let categories: Vec<&Value> = layout[0]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["id"])
        .collect();
    assert_eq!(
        categories,
        [
            &json!("filament"),
            &json!("cooling"),
            &json!("setting_overrides"),
            &json!("advanced")
        ]
    );
    assert_eq!(
        layout[0]["categories"][0]["groups"][0],
        json!({"id": "filament_basics", "title": "Basic information"})
    );
    let all = query(&doc, json!({"query": "print_keys"}));
    assert!(all["keys"].as_array().unwrap().len() > keys.len());
    assert_eq!(all["layout"].as_array().unwrap().len(), 3);
    let order = all["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key"] == "wall_sequence")
        .unwrap();
    assert_eq!(order["category"], "quality");
    assert_eq!(order["group"], "order_and_flow");
    assert_eq!(
        order["choices"][0],
        json!({"value": "inner_outer", "name": "Inner, then outer"})
    );
    let error = doc
        .query(&json!({"query": "print_keys", "set": "plate"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("no set of print settings"), "{error}");
    // mitcad#746: a number's range as its slicers take it, each bound only
    // where there is one; a dialect's narrower range on its entry.
    let find = |name: &str| {
        all["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["key"] == name)
            .unwrap()
            .clone()
    };
    let density = find("fill_density");
    assert_eq!(
        (&density["min"], &density["max"]),
        (&json!(0.0), &json!(100.0))
    );
    let spacing = find("wipe_tower_extra_spacing");
    assert!(spacing["max"].as_f64().unwrap() >= 150.0);
    let height = find("layer_height");
    assert_eq!(height["min"], json!(0.0));
    assert!(height.get("max").is_none());
    assert!(find("xy_size_compensation").get("min").is_none());
    assert!(find("seam_position").get("min").is_none());
    let walls = find("tree_support_wall_count");
    assert_eq!(walls["min"], json!(-1.0));
    let dialect = |name: &str| {
        walls["dialects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["dialect"] == name)
            .unwrap()
            .clone()
    };
    assert!(dialect("bambu_studio").get("min").is_none());
    assert_eq!(
        (&dialect("orcaslicer")["min"], &dialect("orcaslicer")["max"]),
        (&json!(0.0), &json!(2.0))
    );
}
