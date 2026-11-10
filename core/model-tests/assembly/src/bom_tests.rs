// SPDX-License-Identifier: MIT
//! BOM grouping is a report, with a measured local-frame key and no undo.

use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, PartSignature};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn block(doc: &mut Document<MockKernel>, length: f64) -> String {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {
            "type": "box", "plane": {"type": "origin", "plane": "xy"},
            "corner": [0, 0], "length": length, "width": 10, "height": 10,
            "operation": "new_body"
        }}),
    )["uid"]
        .as_str()
        .unwrap()
        .to_owned()
        + ".b0"
}

#[test]
fn geometry_key_matches_equal_and_mirrored_parts_but_not_length_or_material() {
    let key = PartSignature {
        volume: 5000.0,
        dimensions: [10.0, 10.0, 50.0],
        faces: 6,
        materials: vec!["steel".to_owned()],
    };
    assert!(key.matches(&key.clone()));
    // A mirror changes orientation, not these unsigned measured dimensions.
    let mirrored = PartSignature {
        volume: 5000.0,
        ..key.clone()
    };
    assert!(key.matches(&mirrored));
    // Partitioning the same all-steel geometry into several bodies does
    // not add a material or an extra grouping-key field.
    let partitioned = PartSignature {
        materials: vec!["steel".to_owned(), "steel".to_owned()],
        ..key.clone()
    };
    assert!(key.matches(&partitioned));
    let longer = PartSignature {
        volume: 5100.0,
        dimensions: [10.0, 10.0, 51.0],
        ..key.clone()
    };
    assert!(!key.matches(&longer));
    let other_material = PartSignature {
        materials: vec!["aluminum".to_owned()],
        ..key.clone()
    };
    assert!(!key.matches(&other_material));
    let tolerance = PartSignature {
        volume: key.volume + 0.5e-6,
        dimensions: [10.0 + 0.5e-6, 10.0, 50.0],
        ..key.clone()
    };
    assert!(key.matches(&tolerance));
    assert!(!key.matches(&PartSignature {
        faces: 7,
        ..key.clone()
    }));
}

#[test]
fn separate_components_group_without_mutating_the_design_and_plain_bodies_count_once() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    for (name, length) in [
        ("First", 10.0),
        ("Second", 10.0),
        ("Third", 10.0),
        ("Longer", 20.0),
    ] {
        command(&mut doc, json!({"cmd": "create_component", "name": name}));
        block(&mut doc, length);
        command(
            &mut doc,
            json!({"cmd": "activate_component", "component": "C0"}),
        );
    }
    let before = doc.to_json();
    let undo = query(&doc, json!({"query": "document"}))["undo"].clone();
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 4);
    let grouped = query(
        &doc,
        json!({"query": "parts_list", "group_identical": true}),
    );
    assert_eq!(grouped["rows"].as_array().unwrap().len(), 2, "{grouped}");
    assert_eq!(grouped["rows"][0]["quantity"], 3);
    assert_eq!(
        grouped["rows"][0]["group_names"],
        json!(["First", "Second", "Third"])
    );
    assert_eq!(grouped["rows"][1]["quantity"], 1);
    assert_eq!(doc.to_json(), before);
    assert_eq!(query(&doc, json!({"query": "document"}))["undo"], undo);

    // Two root bodies are rows too. No extra root/component row duplicates them.
    let root = block(&mut doc, 10.0);
    block(&mut doc, 10.0);
    let plain = query(&doc, json!({"query": "parts_list"}));
    assert_eq!(plain["rows"].as_array().unwrap().len(), 6);
    assert_eq!(
        query(
            &doc,
            json!({"query": "parts_list", "group_identical": true})
        )["rows"][0]["quantity"],
        5
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": root, "material": "aluminum"}),
    );
    assert_eq!(
        query(
            &doc,
            json!({"query": "parts_list", "group_identical": true})
        )["rows"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // The two bodies in one component replace its component row; each inherits
    // that component's placement quantity, including nested occurrences.
    let component = command(&mut doc, json!({"cmd": "create_component", "name": "Pair"}));
    block(&mut doc, 10.0);
    block(&mut doc, 20.0);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": component["occurrence"]}),
    );
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    let pair: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["component"] == component["component"])
        .collect();
    assert_eq!(pair.len(), 2, "{rows}");
    assert!(
        pair.iter()
            .all(|r| r["quantity"] == 2 && r["body"].is_string())
    );
}

#[test]
fn unsupported_measurements_never_group_unrelated_parts() {
    let mut doc = Document::new(MockKernel::default());
    block(&mut doc, 10.0);
    block(&mut doc, 10.0);
    let rows = query(
        &doc,
        json!({"query": "parts_list", "group_identical": true}),
    );
    assert_eq!(rows["rows"].as_array().unwrap().len(), 2);
}

#[test]
fn measured_mirrored_body_groups_with_the_original_in_its_own_frame() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    let body = block(&mut doc, 50.0);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {
            "type": "mirror", "objects": {"type": "bodies", "bodies": [body]},
            "plane": {"type": "origin", "plane": "yz"}
        }}),
    );
    let separate = query(&doc, json!({"query": "parts_list"}));
    assert_eq!(separate["rows"].as_array().unwrap().len(), 2);
    let grouped = query(
        &doc,
        json!({"query": "parts_list", "group_identical": true}),
    );
    assert_eq!(grouped["rows"].as_array().unwrap().len(), 1, "{grouped}");
    assert_eq!(grouped["rows"][0]["quantity"], 2);
}

#[test]
fn first_level_assembly_signature_includes_children_in_the_assemblys_own_frame() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    let mut parts = Vec::new();
    for (name, translation, child_length) in [("One", 100, 10.0), ("Two", 200, 20.0)] {
        let part = command(
            &mut doc,
            json!({"cmd": "create_component", "name": name,
            "transform": {"translation": [translation, 0, 0]}}),
        );
        block(&mut doc, 10.0);
        command(
            &mut doc,
            json!({"cmd": "create_component", "transform": {"translation": [20, 0, 0]}}),
        );
        block(&mut doc, child_length);
        command(
            &mut doc,
            json!({"cmd": "activate_component", "component": "C0"}),
        );
        parts.push(part);
    }
    let component = |n: usize| parts[n]["component"].as_str().unwrap().parse().unwrap();
    let path = |n: usize| vec![parts[n]["occurrence"].as_str().unwrap().parse().unwrap()];
    let first = component(0);
    let second = component(1);
    assert!(
        doc.part_signature(&doc.component_bodies(first))
            .unwrap()
            .matches(&doc.part_signature(&doc.component_bodies(second)).unwrap())
    );
    let root = mitcad_model::ids::ComponentUid::ROOT;
    let first_key = doc.component_part_signature(first, root, &path(0)).unwrap();
    let second_key = doc
        .component_part_signature(second, root, &path(1))
        .unwrap();
    assert!(!first_key.matches(&second_key));
    assert!((first_key.dimensions[0] - 30.0).abs() <= 1e-6);
    assert!((second_key.dimensions[0] - 40.0).abs() <= 1e-6);
}

#[test]
fn bom_material_columns_follow_placement_materials_without_changing_other_properties() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "SteelParent"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Shared"}),
    );
    block(&mut doc, 10.0);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "AluminumParent"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "AluminumParent", "material": "aluminum"}),
    );
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1/O2"}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "Shared", "set": "Identity", "key": "Mark", "value": "Keep this mark"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "Shared", "set": "Identity", "key": "Material", "value": "Catalogue label"}),
    );
    let before = doc.to_json();
    let undo = doc.undo_depth();
    let materials = vec![
        doc.material("steel").unwrap().name.clone(),
        doc.material("aluminum").unwrap().name.clone(),
    ];
    for grouped in [false, true] {
        let answer = query(
            &doc,
            json!({"query": "parts_list", "group_identical": grouped,
            "properties": ["Material", "Computed/Material", "Identity/Material", "Mark"]}),
        );
        let shared: Vec<_> = answer["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["name"] == "Shared")
            .collect();
        assert_eq!(shared.len(), 2, "{answer}");
        for (row, material) in shared.iter().zip(&materials) {
            assert_eq!(row["quantity"], 1);
            assert_eq!(row["properties"]["Material"], material.as_str());
            assert_eq!(row["properties"]["Computed/Material"], material.as_str());
            assert_eq!(row["properties"]["Identity/Material"], "Catalogue label");
            assert_eq!(row["properties"]["Mark"], "Keep this mark");
        }
    }
    assert_eq!(doc.to_json(), before);
    assert_eq!(doc.undo_depth(), undo);
    assert!(
        doc.bom_rows(false)
            .iter()
            .all(|row| row.signature.is_none())
    );
}

// A component's own `Part number` property is its parts list's part number
// when it is no library or BUILD part (mitcad#1579: Insert › Component from
// Web sets it from the downloaded file's name).
#[test]
fn a_components_own_part_number_is_its_parts_list_part_number() {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "6494K13_NO THREADS_Bearing"}),
    );
    let component = made["component"].as_str().unwrap().to_owned();
    block(&mut doc, 10.0);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let row = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "parts_list"}))["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["component"] == component.as_str())
            .cloned()
            .unwrap()
    };
    assert_eq!(row(&doc)["part_number"], "");
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Identity",
               "key": "Part number", "value": "6494K13"}),
    );
    assert_eq!(row(&doc)["part_number"], "6494K13");
    assert_eq!(row(&doc)["name"], "6494K13_NO THREADS_Bearing");
}
