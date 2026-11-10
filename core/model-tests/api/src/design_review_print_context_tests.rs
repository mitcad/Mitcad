// SPDX-License-Identifier: MIT
//! Print guidance follows the same shown placement as print analysis,
//! including occurrence settings and a printer different from the root's.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(
        &doc.command(&command.to_string())
            .unwrap_or_else(|error| panic!("{command}: {error}")),
    )
    .unwrap()
}

fn query(doc: &Document<MockKernel>, request: Value) -> Value {
    serde_json::from_str(
        &doc.query(&request.to_string())
            .unwrap_or_else(|error| panic!("{request}: {error}")),
    )
    .unwrap()
}

fn plate(doc: &mut Document<MockKernel>) -> BodyUid {
    command(
        doc,
        json!([
            {"cmd":"sketch.create"},
            {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[-5,-5],"width":10,"height":10},
            {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F1","region":REGION}],
                "extent":{"type":"distance","distance":1},"operation":"new_body"}}
        ]),
    );
    "F2.b0".parse().unwrap()
}

fn printer(doc: &mut Document<MockKernel>, name: &str, half_bed: f64, nozzle: f64) -> String {
    command(
        doc,
        json!({"cmd":"create_print_profile","kind":"printer","name":name,
        "values":{"bed_shape":[[-half_bed,-half_bed],[half_bed,-half_bed],
            [half_bed,half_bed],[-half_bed,half_bed]],"max_print_height":20,
            "nozzle_diameter":nozzle}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn set_printer(doc: &mut Document<MockKernel>, owner: impl ToString, id: &str) {
    command(
        doc,
        json!({"cmd":"set_property","owner":owner.to_string(),
        "set":"Print Printer","key":"profile","value":id}),
    );
}

fn review(doc: &Document<MockKernel>, check: &str) -> Value {
    query(doc, json!({"query":"design_review","checks":[check]}))
}

#[test]
fn occurrence_printer_admits_auto_and_its_nozzle_controls_wall_review() {
    let mut doc = Document::new(MockKernel::default());
    let (_, first) = doc
        .create_component(Some("Printed"), Transform::IDENTITY, true)
        .unwrap();
    let body = plate(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc.copy_occurrence(first, None).unwrap();
    let large_nozzle = printer(&mut doc, "Large nozzle", 20.0, 0.8);
    set_printer(&mut doc, first, &large_nozzle);

    // The root and component have no printer. The first occurrence does,
    // so the 1 mm plate is checked against its 1.6 mm wall minimum.
    let answer = review(&doc, "thin-wall");
    assert_eq!(answer["print"]["enabled"], true, "{answer}");
    assert_eq!(answer["findings"].as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(answer["findings"][0]["target"], body.to_string());
    assert!(
        answer["findings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("1.6 mm")
    );

    // Hiding that occurrence makes the next shown one the print-analysis
    // placement. It has no printer, plate or bed face, so auto stays off.
    doc.set_occurrence_visible(first, false).unwrap();
    let answer = review(&doc, "thin-wall");
    assert_eq!(answer["print"]["enabled"], false, "{answer}");
    assert_eq!(answer["findings"], json!([]));

    set_printer(&mut doc, second, &large_nozzle);
    command(
        &mut doc,
        json!({"cmd":"set_property","owner":second.to_string(),
        "set":"Print Printer","key":"nozzle_diameter","value":0.4}),
    );
    let answer = review(&doc, "thin-wall");
    assert_eq!(answer["print"]["enabled"], true);
    assert_eq!(answer["findings"], json!([]), "{answer}");

    // The body's own setting still overrides the occurrence's.
    command(
        &mut doc,
        json!({"cmd":"set_property","owner":body.to_string(),
        "set":"Print Printer","key":"nozzle_diameter","value":0.8}),
    );
    assert_eq!(
        review(&doc, "thin-wall")["findings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn body_and_occurrence_printers_supply_their_own_build_volume() {
    let mut doc = Document::new(MockKernel::default());
    let (_, occurrence) = doc
        .create_component(Some("Printed"), Transform::IDENTITY, true)
        .unwrap();
    let body = plate(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let large = printer(&mut doc, "Large bed", 100.0, 0.4);
    let small = printer(&mut doc, "Small bed", 2.0, 0.4);
    set_printer(&mut doc, ComponentUid::ROOT, &large);

    assert_eq!(
        query(&doc, json!({"query":"build_volume"}))["outside"],
        json!([])
    );
    set_printer(&mut doc, occurrence, &small);
    let answer = review(&doc, "outside-build-volume");
    assert_eq!(answer["errors"], json!([]), "{answer}");
    assert_eq!(answer["findings"].as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(answer["findings"][0]["target"], body.to_string());

    // A body override wins over that occurrence, and the original root
    // query's behavior is unchanged by the shared implementation.
    set_printer(&mut doc, body, &large);
    assert_eq!(review(&doc, "outside-build-volume")["findings"], json!([]));
    assert_eq!(
        query(&doc, json!({"query":"build_volume"}))["outside"],
        json!([])
    );

    set_printer(&mut doc, body, &small);
    assert_eq!(
        review(&doc, "outside-build-volume")["findings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn build_volume_uses_the_first_shown_path_and_reports_a_missing_body_bed() {
    let mut doc = Document::new(MockKernel::default());
    let (_, first) = doc
        .create_component(Some("Printed"), Transform::IDENTITY, true)
        .unwrap();
    let body = plate(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc.copy_occurrence(first, None).unwrap();
    let small = printer(&mut doc, "Small bed", 2.0, 0.4);
    let large = printer(&mut doc, "Large bed", 100.0, 0.4);
    set_printer(&mut doc, first, &small);
    set_printer(&mut doc, second, &large);
    assert_eq!(
        review(&doc, "outside-build-volume")["findings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    doc.set_occurrence_visible(first, false).unwrap();
    let answer = review(&doc, "outside-build-volume");
    assert_eq!(answer["findings"], json!([]), "{answer}");
    assert_eq!(answer["errors"], json!([]), "{answer}");

    command(
        &mut doc,
        json!({"cmd":"remove_property","owner":second.to_string(),
        "set":"Print Printer","key":"profile"}),
    );
    let answer = query(
        &doc,
        json!({"query":"design_review","checks":["outside-build-volume"],"print":true}),
    );
    assert_eq!(answer["findings"], json!([]));
    assert_eq!(answer["errors"].as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(answer["errors"][0]["target"], body.to_string());
}

#[test]
fn an_occurrence_assigned_plate_supplies_the_reported_process_context() {
    let mut doc = Document::new(MockKernel::default());
    let (_, occurrence) = doc
        .create_component(Some("Printed"), Transform::IDENTITY, true)
        .unwrap();
    let body = plate(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    command(&mut doc, json!({"cmd":"add_print_plate","name":"Default"}));
    command(&mut doc, json!({"cmd":"add_print_plate","name":"Coarse"}));
    command(
        &mut doc,
        json!({"cmd":"edit_print_plate","plate":"Coarse","values":{"layer_height":0.3}}),
    );
    command(
        &mut doc,
        json!({"cmd":"place_on_plate","plate":"Coarse", "items":[{"occurrence":occurrence.to_string()}]}),
    );

    let answer = review(&doc, "thin-wall");
    assert_eq!(answer["print"]["enabled"], true, "{answer}");
    let context = &answer["print"]["contexts"][0];
    assert_eq!(context["body"], body.to_string());
    assert_eq!(context["occurrence"], occurrence.to_string());
    assert_eq!(context["plate"], "plate2");
    assert_eq!(context["layer_height"], 0.3);
    assert_eq!(context["nozzle_diameter"], 0.4);

    // A body setting wins over the plate's process setting.
    command(
        &mut doc,
        json!({"cmd":"set_property","owner":body.to_string(),
        "set":"Print Process","key":"layer_height","value":0.16}),
    );
    assert_eq!(
        review(&doc, "thin-wall")["print"]["contexts"][0]["layer_height"],
        0.16
    );
}
