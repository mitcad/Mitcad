// SPDX-License-Identifier: MIT
//! Materials' mechanical and thermal properties (mitcad#1213) through the
//! JSON api with the mock kernel: set, saved, read back and undone; shown
//! in the design's units; materials made from the bundled dataset; and
//! materials exported and imported as JSON and CSV, with invalid rows
//! reported and name conflicts decided.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

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
    serde_json::from_str(&doc.query(&query.to_string()).unwrap())
        .unwrap_or_else(|e| panic!("{query}: {e}"))
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
        .unwrap_or(Value::Null)
}

fn named(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "materials"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == name)
        .cloned()
        .unwrap_or(Value::Null)
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

fn close(a: &Value, b: f64) -> bool {
    a.as_f64()
        .is_some_and(|a| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0))
}

#[test]
fn properties_are_set_saved_read_back_and_undone() {
    let mut doc = block();
    let id = command(
        &mut doc,
        json!({"cmd": "create_material", "name": "Shop steel", "density": 7.85, "class": "metal",
               "properties": {"youngs_modulus": 210000, "poissons_ratio": 0.3},
               "reference": {"source": "Mill sheet", "grade": "S235"}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shown = material(&doc, &id);
    assert_eq!(
        shown["properties"],
        json!({"youngs_modulus": 210000.0, "poissons_ratio": 0.3})
    );
    // A value not given is left out, never 0.
    assert!(shown["properties"].get("yield_strength").is_none());
    assert_eq!(
        shown["reference"],
        json!({"source": "Mill sheet", "grade": "S235"})
    );

    // Given and taken away in one edit, one undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": id,
               "properties": {"yield_strength": 235, "poissons_ratio": null}}),
    );
    assert_eq!(
        material(&doc, &id)["properties"],
        json!({"youngs_modulus": 210000.0, "yield_strength": 235.0})
    );
    assert_eq!(undo_label(&doc), "Edit Material Shop steel");

    // The design file keeps them; a design read back has them.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["materials"][0]["properties"],
        json!({"youngs_modulus": 210000.0, "yield_strength": 235.0})
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(
        material(&loaded, &id)["properties"],
        material(&doc, &id)["properties"]
    );
    assert_eq!(loaded.to_json(), saved);

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        material(&doc, &id)["properties"],
        json!({"youngs_modulus": 210000.0, "poissons_ratio": 0.3})
    );

    // Out of range, 0 and unknown keys are refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_material", "id": id, "properties": {"youngs_modulus": 0}}),
    );
    assert!(
        error.contains("Young's modulus must be greater than zero"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_material", "id": id, "properties": {"poissons_ratio": 0.7}}),
    );
    assert!(error.contains("Poisson's ratio"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_material", "id": id, "properties": {"hardness": 3}}),
    );
    assert!(error.contains("hardness"), "{error}");
}

#[test]
fn values_are_shown_in_the_designs_units() {
    let mut doc = block();
    let metric = query(&doc, json!({"query": "material_units"}));
    assert_eq!(metric["units"], "metric");
    assert_eq!(metric["quantities"]["stress"]["symbol"], "MPa");
    assert_eq!(metric["quantities"]["density"]["symbol"], "g/cm³");
    assert_eq!(metric["quantities"]["expansion"]["symbol"], "µm/(m·°C)");
    assert_eq!(metric["quantities"]["temperature"]["symbol"], "°C");
    // The properties in their groups, then the print settings' two.
    let keys: Vec<&str> = metric["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "youngs_modulus",
            "poissons_ratio",
            "yield_strength",
            "max_service_temperature",
            "thermal_expansion"
        ]
    );
    assert_eq!(metric["properties"][3]["group"], "thermal");
    assert_eq!(metric["printing"][0]["key"], "temperature");
    assert_eq!(metric["printing"][1]["key"], "bed_temperature");

    // An inch design: psi, lb/in³, µin/(in·°F); temperatures as preferred.
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    let inch = query(&doc, json!({"query": "material_units", "temperature": "F"}));
    assert_eq!(inch["units"], "imperial");
    let stress = &inch["quantities"]["stress"];
    assert_eq!(stress["symbol"], "psi");
    // 1 MPa is 145.04 psi.
    assert!(close(&stress["scale"], 145.037_737_730_209));
    assert_eq!(inch["quantities"]["density"]["symbol"], "lb/in³");
    assert_eq!(inch["quantities"]["expansion"]["symbol"], "µin/(in·°F)");
    let temperature = &inch["quantities"]["temperature"];
    assert_eq!(
        (
            &temperature["symbol"],
            &temperature["scale"],
            &temperature["offset"]
        ),
        (&json!("°F"), &json!(1.8), &json!(32.0))
    );
    let celsius = query(&doc, json!({"query": "material_units", "temperature": "C"}));
    assert_eq!(celsius["quantities"]["temperature"]["symbol"], "°C");
    assert_eq!(celsius["quantities"]["stress"]["symbol"], "psi");

    // Physical Properties writes the body's material's values in them.
    command(
        &mut doc,
        json!({"cmd": "create_material", "id": "shop_steel", "name": "Shop steel", "density": 7.85,
               "properties": {"youngs_modulus": 200000, "max_service_temperature": 100}}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "shop_steel"}),
    );
    // The mock kernel measures the box only when asked to.
    doc.kernel().measures.set(true);
    doc.kernel().physical.set(true);
    let properties = query(&doc, json!({"query": "properties", "temperature": "F"}));
    let rows = &properties["bodies"][0]["material_properties"];
    assert_eq!(
        rows,
        &json!([
            {"key": "youngs_modulus", "name": "Young's modulus", "group": "mechanical",
             "text": "29007500 psi"},
            {"key": "max_service_temperature", "name": "Maximum service temperature",
             "group": "thermal", "text": "212 °F"},
        ])
    );
}

#[test]
fn a_dataset_material_is_chosen_and_saved_as_the_users_own() {
    let mut doc = block();
    let found = query(
        &doc,
        json!({"query": "material_dataset", "search": "6061 t6"}),
    );
    assert_eq!(found["license"], "Apache-2.0");
    assert!(found["caution"].as_str().unwrap().contains("supplier"));
    let entries = found["materials"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], "aluminum/6061/t6");
    // In the model's units: 0.098 lb/in³ as g/cm³.
    assert!(close(&entries[0]["density"], 2.712_630_661_599_9));

    let id = command(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "aluminum/6061/t6"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let made = material(&doc, &id);
    assert_eq!(made["name"], "Aluminum 6061, T6");
    assert_eq!(made["class"], "metal");
    assert_eq!(made["appearance"], "aluminum_anodized");
    assert_eq!(
        (&made["reference"]["material"], &made["reference"]["grade"]),
        (&json!("6061"), &json!("T6"))
    );
    assert!(close(
        &made["properties"]["youngs_modulus"],
        71_016.000_119_634
    ));
    assert_eq!(undo_label(&doc), "Create Material Aluminum 6061, T6");
    // The user edits it as their own; a second one is numbered.
    command(
        &mut doc,
        json!({"cmd": "edit_material", "id": id, "properties": {"yield_strength": 250}}),
    );
    assert_eq!(material(&doc, &id)["properties"]["yield_strength"], 250.0);
    let second = command(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "aluminum/6061/t6"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(material(&doc, &second)["name"], "Aluminum 6061, T6 (2)");
    // Reading the dataset again never changes the user's material.
    query(&doc, json!({"query": "material_dataset"}));
    assert_eq!(material(&doc, &id)["properties"]["yield_strength"], 250.0);
    // A printing material's temperatures are its print settings.
    let pla = command(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "3dprinting/pla/default"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let pla = material(&doc, &pla);
    assert_eq!(pla["class"], "fdm_plastic");
    assert_eq!(pla["filament"]["temperature"], 205.0);
    assert_eq!(pla["filament"]["nozzle_temperature_range_low"], 180);

    let error = refused(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "nothing/at/all"}),
    );
    assert!(error.contains("no material nothing/at/all"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "aluminum/6061/t6", "based_on": "steel"}),
    );
    assert!(error.contains("not both"), "{error}");
}

/// mitcad#1552: the dataset's woods and panels, read through the
/// `material_dataset` query: 17 of the class Wood, each with its density
/// and, for the solid woods, its Young's modulus (none for the panels);
/// `create_material` from `wood/walnut` makes a Wood material with its look
/// and its source; a new Wood material starts with the class's look.
#[test]
fn the_woods_and_panels_of_the_dataset_are_wood_materials() {
    let mut doc = block();
    let all = query(&doc, json!({"query": "material_dataset"}));
    assert_eq!(
        all["sources"],
        json!([
            {"name": "material-properties dataset", "license": "Apache-2.0"},
            {"name": "MitCad wood and panel materials", "license": "MIT"}
        ])
    );
    // (id, kg/m³, GPa)
    let expected: [(&str, f64, Option<f64>); 17] = [
        ("panel/chipboard", 611.0, None),
        ("panel/mdf_raw", 736.0, None),
        ("panel/osb3", 610.0, None),
        ("panel/plywood_birch", 680.0, None),
        ("panel/plywood_eucalyptus", 600.0, None),
        ("panel/plywood_pine", 500.0, None),
        ("panel/plywood_spruce", 460.0, None),
        ("wood/birch", 690.0, Some(13.86)),
        ("wood/cedar", 370.0, Some(7.66)),
        ("wood/cherry", 560.0, Some(10.3)),
        ("wood/douglas_fir", 510.0, Some(12.17)),
        ("wood/eucalyptus", 820.0, Some(18.76)),
        ("wood/maple", 705.0, Some(12.62)),
        ("wood/oak", 755.0, Some(12.15)),
        ("wood/pine", 550.0, Some(10.08)),
        ("wood/teak", 655.0, Some(12.28)),
        ("wood/walnut", 610.0, Some(11.59)),
    ];
    let materials = all["materials"].as_array().unwrap();
    let woods: Vec<&Value> = materials.iter().filter(|m| m["class"] == "wood").collect();
    assert_eq!(woods.len(), expected.len());
    for (id, density, modulus) in expected {
        let entry = materials
            .iter()
            .find(|m| m["id"] == id)
            .unwrap_or_else(|| panic!("{id} is missing"));
        assert_eq!(entry["class"], "wood", "{id}");
        // g/cm³ and MPa, the model's units.
        assert!(close(&entry["density"], density / 1000.0), "{id}: {entry}");
        match modulus {
            Some(gpa) => assert!(
                close(&entry["properties"]["youngs_modulus"], gpa * 1000.0),
                "{id}: {entry}"
            ),
            None => assert!(
                entry["properties"].get("youngs_modulus").is_none(),
                "{id}: {entry}"
            ),
        }
        assert!(entry["reference"]["source"].is_string(), "{id}");
    }
    // A search finds the oak alone.
    let found = query(&doc, json!({"query": "material_dataset", "search": "oak"}));
    let found = found["materials"].as_array().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["name"], "Wood - Oak");

    // A wood that is not built in (the oak is since mitcad#1706).
    let id = command(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "wood/walnut"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let walnut = material(&doc, &id);
    assert_eq!(walnut["name"], "Wood - Walnut");
    assert_eq!(walnut["class"], "wood");
    assert_eq!(walnut["appearance"], "wood_walnut");
    assert!(close(&walnut["density"], 0.61));
    assert!(close(&walnut["properties"]["youngs_modulus"], 11_590.0));
    assert_eq!(
        walnut["reference"],
        json!({"source": "The Wood Database", "material": "Black walnut (Juglans nigra)"})
    );
    assert_eq!(undo_label(&doc), "Create Material Wood - Walnut");
    // A panel: its own look, no modulus.
    let osb = command(
        &mut doc,
        json!({"cmd": "create_material", "dataset": "panel/osb3"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let osb = material(&doc, &osb);
    assert_eq!(osb["class"], "wood");
    assert_eq!(osb["appearance"], "panel_osb");
    assert_eq!(osb["reference"]["grade"], "15 mm");
    // A new Wood material starts with the class's look; one that names
    // its appearance keeps it.
    let plain = command(
        &mut doc,
        json!({"cmd": "create_material", "class": "wood", "density": 0.5}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(material(&doc, &plain)["appearance"], "wood_oak");
    let painted = command(
        &mut doc,
        json!({"cmd": "create_material", "class": "wood", "density": 0.5,
               "appearance": "paint_white"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(material(&doc, &painted)["appearance"], "paint_white");
    let metal = command(
        &mut doc,
        json!({"cmd": "create_material", "class": "metal", "density": 7.85}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        material(&doc, &metal)
            .get("appearance")
            .is_none_or(Value::is_null)
    );
}

#[test]
fn materials_round_trip_through_json_and_csv() {
    let mut doc = block();
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    command(
        &mut doc,
        json!({"cmd": "create_material", "name": "Shop steel", "density": 7.85, "class": "metal",
               "properties": {"youngs_modulus": 210000, "poissons_ratio": 0.3, "yield_strength": 235,
                              "max_service_temperature": 400, "thermal_expansion": 12},
               "reference": {"source": "Mill sheet", "material": "S235JR", "grade": "JR"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_material", "name": "Shop PLA", "density": 1.24,
               "filament": {"filament_type": "PLA", "temperature": 210, "bed_temperature": 60}}),
    );
    for format in ["json", "csv"] {
        // In the design's units (an inch design: psi), temperatures in °F.
        let exported = query(
            &doc,
            json!({"query": "export_materials", "format": format, "temperature": "F"}),
        );
        assert_eq!(exported["count"], 2);
        assert_eq!(exported["units"], "imperial");
        let text = exported["text"].as_str().unwrap();
        if format == "csv" {
            assert!(text.contains("youngs_modulus [psi]"), "{text}");
            assert!(text.contains("extruder_temperature [F]"), "{text}");
        } else {
            assert!(text.contains("\"stress\": \"psi\""), "{text}");
        }
        let mut other = block();
        let imported = command(
            &mut other,
            json!({"cmd": "import_materials", "text": text, "format": format}),
        );
        assert_eq!(imported["errors"], json!([]), "{text}");
        assert_eq!(imported["added"].as_array().unwrap().len(), 2);
        assert_eq!(undo_label(&other), "Import Materials");
        let steel = named(&other, "Shop steel");
        let mine = named(&doc, "Shop steel");
        for key in [
            "youngs_modulus",
            "poissons_ratio",
            "yield_strength",
            "max_service_temperature",
            "thermal_expansion",
        ] {
            assert!(
                close(
                    &steel["properties"][key],
                    mine["properties"][key].as_f64().unwrap()
                ),
                "{format} {key}: {} {}",
                steel["properties"][key],
                mine["properties"][key]
            );
        }
        assert!(close(&steel["density"], 7.85));
        assert_eq!(steel["reference"], mine["reference"]);
        let pla = named(&other, "Shop PLA");
        assert!(close(&pla["filament"]["temperature"], 210.0), "{pla}");
        assert!(close(&pla["filament"]["bed_temperature"], 60.0), "{pla}");
        // One undo step takes the whole import back.
        command(&mut other, json!({"cmd": "undo"}));
        assert!(named(&other, "Shop steel").is_null());
        assert!(named(&other, "Shop PLA").is_null());
    }
}

#[test]
fn an_import_reports_invalid_rows_and_decides_name_conflicts() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "create_material", "name": "Shop steel", "density": 7.85,
               "properties": {"youngs_modulus": 210000, "poissons_ratio": 0.3}}),
    );
    let csv = "name,density [g/cm3],yield_strength [MPa],poissons_ratio\r\n\
               Shop steel,,250,\r\n\
               Brass CZ121,8.5,300,0.34\r\n\
               Broken,8.5,-3,\r\n";
    // Ask: nothing is imported while a name is there; the names come back.
    let asked = command(
        &mut doc,
        json!({"cmd": "import_materials", "text": csv, "format": "csv", "conflict": "ask"}),
    );
    assert_eq!(asked["conflicts"], json!(["Shop steel"]));
    assert_eq!(asked["added"], json!([]));
    assert!(named(&doc, "Brass CZ121").is_null());
    // Keep (the default): the design's Shop steel stays as it was; the
    // invalid row is reported with its line and reason.
    let kept = command(
        &mut doc,
        json!({"cmd": "import_materials", "text": csv, "format": "csv"}),
    );
    assert_eq!(kept["kept"], json!(["Shop steel"]));
    assert_eq!(kept["added"].as_array().unwrap().len(), 1);
    assert_eq!(kept["errors"].as_array().unwrap().len(), 1);
    assert_eq!(kept["errors"][0]["row"], 4);
    assert_eq!(kept["errors"][0]["name"], "Broken");
    let message = kept["errors"][0]["message"].as_str().unwrap();
    assert!(
        message.contains("Yield strength must be greater than zero"),
        "{message}"
    );
    assert!(
        named(&doc, "Shop steel")["properties"]
            .get("yield_strength")
            .is_none()
    );
    assert!(close(
        &named(&doc, "Brass CZ121")["properties"]["poissons_ratio"],
        0.34
    ));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(named(&doc, "Brass CZ121").is_null());

    // Replace: the file's values over the design's, the others kept.
    let replaced = command(
        &mut doc,
        json!({"cmd": "import_materials", "text": csv, "format": "csv", "conflict": "replace"}),
    );
    assert_eq!(replaced["replaced"].as_array().unwrap().len(), 1);
    let steel = named(&doc, "Shop steel");
    assert_eq!(steel["properties"]["yield_strength"], 250.0);
    assert_eq!(steel["properties"]["youngs_modulus"], 210000.0);
    assert_eq!(steel["properties"]["poissons_ratio"], 0.3);
    assert_eq!(steel["density"], 7.85);
    command(&mut doc, json!({"cmd": "undo"}));

    // Keep both: the file's beside the design's, numbered.
    let both = command(
        &mut doc,
        json!({"cmd": "import_materials", "text": csv.replace("Shop steel,,", "Shop steel,7.9,"),
               "format": "csv", "conflict": "keep_both"}),
    );
    assert_eq!(both["added"].as_array().unwrap().len(), 2);
    assert_eq!(
        named(&doc, "Shop steel (2)")["properties"]["yield_strength"],
        250.0
    );
    assert!(
        named(&doc, "Shop steel")["properties"]
            .get("yield_strength")
            .is_none()
    );
    command(&mut doc, json!({"cmd": "undo"}));

    // A built-in's name replaced: the design's own in its place.
    let steel_csv = "name,yield_strength [psi]\r\nSteel,36000\r\n";
    command(
        &mut doc,
        json!({"cmd": "import_materials", "text": steel_csv, "format": "csv", "conflict": "replace"}),
    );
    let steel = material(&doc, "steel");
    assert_eq!(steel["built_in"], false);
    assert_eq!(steel["name"], "Steel");
    assert!(close(
        &steel["properties"]["yield_strength"],
        248.211_262_554_061
    ));

    // A file that cannot be read is refused whole; a conflict choice
    // must be one of the three.
    let error = refused(
        &mut doc,
        json!({"cmd": "import_materials", "text": "name,density\r\nX,1\r\n", "format": "csv"}),
    );
    assert!(error.contains("states no unit"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "import_materials", "text": csv, "format": "csv", "conflict": "merge"}),
    );
    assert!(error.contains("keep_both or ask"), "{error}");
}

/// Keep Both numbers a name that differs from the design's only in case,
/// as the clash is found (mitcad#1315): `steel` beside the built-in
/// `Steel` is `steel (2)`.
#[test]
fn keep_both_numbers_a_name_that_differs_only_in_case() {
    let mut doc = block();
    let both = command(
        &mut doc,
        json!({"cmd": "import_materials", "text": "name,density [g/cm3]\r\nsteel,7.9\r\n",
               "format": "csv", "conflict": "keep_both"}),
    );
    assert_eq!(both["added"].as_array().unwrap().len(), 1, "{both}");
    let added = named(&doc, "steel (2)");
    assert!(close(&added["density"], 7.9), "{added}");
    assert!(named(&doc, "steel").is_null());
    assert_eq!(material(&doc, "steel")["name"], "Steel");
}

/// mitcad#1706: common woods and panels are built in, of the class Wood,
/// with the dataset's values and looks; a body of the oak gets its mass
/// from the oak's density; a design's own material of a built-in wood's
/// name (made from the dataset before) still opens and changes.
#[test]
fn common_woods_and_panels_are_built_in() {
    let mut doc = block();
    // (id, g/cm³, MPa)
    let expected: [(&str, f64, Option<f64>); 5] = [
        ("wood_pine", 0.55, Some(10_080.0)),
        ("wood_oak", 0.755, Some(12_150.0)),
        ("wood_birch", 0.69, Some(13_860.0)),
        ("plywood_birch", 0.68, None),
        ("mdf_raw", 0.736, None),
    ];
    let names = [
        "Wood - Pine",
        "Wood - Oak",
        "Wood - Birch",
        "Plywood - birch",
        "MDF - raw",
    ];
    let looks = [
        "wood_pine",
        "wood_oak",
        "wood_birch",
        "plywood_birch",
        "panel_mdf",
    ];
    let all = query(&doc, json!({"query": "materials"}));
    let woods: Vec<&Value> = all
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["class"] == "wood")
        .collect();
    assert_eq!(woods.len(), expected.len(), "{all}");
    for (i, wood) in woods.into_iter().enumerate() {
        let (id, density, modulus) = expected[i];
        assert_eq!(wood["id"], id, "{wood}");
        assert_eq!(wood["name"], names[i], "{wood}");
        assert_eq!(wood["built_in"], true, "{wood}");
        assert!(close(&wood["density"], density), "{wood}");
        // A panel's is not given (no key).
        let given = &wood["properties"]["youngs_modulus"];
        match modulus {
            Some(mpa) => assert!(close(given, mpa), "{wood}"),
            None => assert!(given.is_null(), "{wood}"),
        }
        assert_eq!(wood["appearance"], looks[i], "{wood}");
        assert!(wood["reference"]["source"].is_string(), "{wood}");
    }

    // A body of the oak: the oak's density, its mass from it (the mock
    // kernel measures the box).
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "wood_oak"}),
    );
    doc.kernel().measures.set(true);
    doc.kernel().physical.set(true);
    let properties = query(&doc, json!({"query": "properties"}));
    let body = &properties["bodies"][0];
    assert_eq!(body["material"], "wood_oak", "{properties}");
    assert!(close(&body["density"], 0.755), "{properties}");
    let volume = body["volume"].as_f64().unwrap();
    assert!(volume > 0.0, "{properties}");
    assert!(close(&body["mass"], volume / 1e6 * 0.755), "{properties}");
    // Saved and opened again, it is still the oak.
    let saved = doc.to_json();
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);

    // A design's own `Wood - Oak` from before (as a design file of an
    // earlier version holds it) opens unchanged and still changes; a new
    // one of a built-in's name is refused as before.
    let mut own = block();
    let id = command(
        &mut own,
        json!({"cmd": "create_material", "name": "Oak of before", "density": 0.75,
               "class": "wood"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = own.to_json().replace("Oak of before", "Wood - Oak");
    let mut opened = Document::from_json(&before, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), before);
    assert_eq!(material(&opened, &id)["name"], "Wood - Oak");
    assert_eq!(material(&opened, "wood_oak")["name"], "Wood - Oak");
    command(
        &mut opened,
        json!({"cmd": "edit_material", "id": id, "density": 0.76}),
    );
    assert!(close(&material(&opened, &id)["density"], 0.76));
    let error = refused(
        &mut opened,
        json!({"cmd": "create_material", "name": "Wood - Pine", "density": 0.5}),
    );
    assert!(error.contains("a material is named Wood - Pine"), "{error}");
}

/// mitcad#1706: several materials of the dataset made at once, as one undo
/// step; a name there already (a built-in wood's, a design's own, one
/// named twice) is skipped and said; an entry the dataset lacks refuses
/// the whole; into a library likewise, one undo step for its files.
#[test]
fn dataset_materials_are_made_at_once_and_names_there_skipped() {
    let mut doc = block();
    let depth = doc.undo_depth();
    let made = command(
        &mut doc,
        json!({"cmd": "create_dataset_materials",
               "entries": ["panel/plywood_eucalyptus", "panel/plywood_pine",
                           "panel/plywood_spruce"]}),
    );
    assert_eq!(made["skipped"], json!([]), "{made}");
    let added: Vec<String> = made["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(added.len(), 3, "{made}");
    for (id, name, look) in [
        (&added[0], "Plywood - eucalyptus", "plywood_eucalyptus"),
        (&added[1], "Plywood - pine", "plywood_pine"),
        (&added[2], "Plywood - spruce", "plywood_spruce"),
    ] {
        let wood = material(&doc, id);
        assert_eq!(wood["name"], name, "{wood}");
        assert_eq!(wood["class"], "wood", "{wood}");
        assert_eq!(wood["appearance"], look, "{wood}");
        assert_eq!(wood["built_in"], false, "{wood}");
        // Each with its own source.
        assert!(wood["reference"]["source"].is_string(), "{wood}");
    }
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(undo_label(&doc), "Create Materials");

    // The built-in birch plywood and oak, a pine plywood made above and a
    // walnut named twice: only the first walnut is made.
    let again = command(
        &mut doc,
        json!({"cmd": "create_dataset_materials",
               "entries": ["panel/plywood_birch", "wood/oak", "panel/plywood_pine",
                           "wood/walnut", "wood/walnut"]}),
    );
    assert_eq!(again["added"].as_array().unwrap().len(), 1, "{again}");
    assert_eq!(
        again["skipped"],
        json!([
            "Plywood - birch",
            "Wood - Oak",
            "Plywood - pine",
            "Wood - Walnut"
        ])
    );
    assert_eq!(undo_label(&doc), "Create Material Wood - Walnut");
    // All of them there already: nothing made, no undo step.
    let none = command(
        &mut doc,
        json!({"cmd": "create_dataset_materials", "entries": ["wood/walnut"]}),
    );
    assert_eq!(none["added"], json!([]));
    assert_eq!(none["skipped"], json!(["Wood - Walnut"]));
    assert_eq!(doc.undo_depth(), depth + 2);
    // One undo takes the three plywoods away together.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(doc.undo_depth(), depth);
    assert!(named(&doc, "Plywood - pine").is_null());

    let error = refused(
        &mut doc,
        json!({"cmd": "create_dataset_materials",
               "entries": ["wood/walnut", "nothing/at/all"]}),
    );
    assert!(error.contains("no material nothing/at/all"), "{error}");
    assert!(named(&doc, "Wood - Walnut").is_null());
    let error = refused(
        &mut doc,
        json!({"cmd": "create_dataset_materials", "entries": []}),
    );
    assert!(error.contains("at least one"), "{error}");

    // Into the project's library: a file each, one undo step, the built-in
    // name skipped there too.
    let root = mitcad_model::testing::unique_temp_path("dataset-materials");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let folder = mitcad_model::print_library::project_library(&project);
    let path = project.to_string_lossy().to_string();
    let depth = doc.undo_depth();
    let made = command(
        &mut doc,
        json!({"cmd": "create_dataset_materials", "library": "project", "project": path,
               "entries": ["panel/plywood_birch", "wood/cedar", "wood/teak"]}),
    );
    assert_eq!(made["skipped"], json!(["Plywood - birch"]), "{made}");
    let files: Vec<std::path::PathBuf> = made["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| folder.join(format!("materials/{}.json", id.as_str().unwrap())))
        .collect();
    assert_eq!(files.len(), 2, "{made}");
    assert!(files.iter().all(|file| file.exists()), "{files:?}");
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(undo_label(&doc), "Create Materials");
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(files.iter().all(|file| !file.exists()), "{files:?}");
}
