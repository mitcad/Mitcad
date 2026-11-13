// SPDX-License-Identifier: MIT
//! Modeling-review findings from concrete synthetic designs. These exercise
//! the public JSON query, including its read-only and print-admission contract.

use std::collections::BTreeSet;

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::kernel::BoundingBox;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const EDGE: &str = "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}";

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let answer = doc
        .command(&value.to_string())
        .unwrap_or_else(|error| panic!("{value}: {error}"));
    serde_json::from_str(&answer).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    let answer = doc
        .query(&value.to_string())
        .unwrap_or_else(|error| panic!("{value}: {error}"));
    serde_json::from_str(&answer).unwrap()
}

fn document() -> Document<MockKernel> {
    let doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    doc.kernel().box_faces.set(true);
    doc
}

fn stock(doc: &mut Document<MockKernel>, name: &str, size: [f64; 3]) -> String {
    command(
        doc,
        json!({"cmd":"add_feature", "name":name, "def":{
            "type":"box", "plane":"xy", "length":size[0],
            "width":size[1], "height":size[2], "operation":"new_body"}}),
    )["uid"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn block() -> Document<MockKernel> {
    let mut doc = document();
    command(&mut doc, json!({"cmd":"sketch.create"}));
    command(
        &mut doc,
        json!({"cmd":"sketch.add_rectangle", "sketch":"F1", "corner":[0,0],
            "width":60, "height":40}),
    );
    command(
        &mut doc,
        json!({"cmd":"add_feature", "def":{"type":"extrude",
            "profiles":[{"sketch":"F1", "region":RECT}],
            "extent":{"type":"distance", "distance":20}, "operation":"new_body"}}),
    );
    doc
}

fn review(doc: &Document<MockKernel>, checks: &[&str]) -> Value {
    query(
        doc,
        json!({"query":"design_review", "checks":checks, "print":false}),
    )
}

fn findings(report: &Value) -> &[Value] {
    report["findings"].as_array().unwrap()
}

fn finding<'a>(report: &'a Value, rule: &str, target: &str) -> &'a Value {
    findings(report)
        .iter()
        .find(|row| row["rule"] == rule && row["target"] == target)
        .unwrap_or_else(|| panic!("no {rule} for {target}: {report}"))
}

fn check_finding(row: &Value, level: &str, topic: &str, anchor: &str) {
    assert_eq!(row["level"], level, "{row}");
    assert!(!row["message"].as_str().unwrap().is_empty(), "{row}");
    assert!(!row["advice"].as_str().unwrap().is_empty(), "{row}");
    assert_eq!(row["guide"], json!({"topic":topic, "anchor":anchor}));
}

fn mesh_calls(doc: &Document<MockKernel>) -> usize {
    let calls = doc.kernel().calls.borrow();
    ["triangle_mesh", "face_meshes"]
        .into_iter()
        .map(|name| calls.get(name).copied().unwrap_or(0))
        .sum()
}

#[test]
fn a_named_grouped_origin_based_stock_has_no_findings_and_review_is_read_only() {
    let mut doc = document();
    let base = stock(&mut doc, "Base stock", [31.0, 23.0, 17.0]);
    command(
        &mut doc,
        json!({"cmd":"group_features", "features":[base], "name":"Core geometry"}),
    );
    doc.mark_saved();
    let before = doc.to_json();
    let state = (doc.revision(), doc.undo_depth(), doc.is_modified());
    let answer = query(&doc, json!({"query":"design_review"}));
    assert!(findings(&answer).is_empty(), "{answer}");
    assert_eq!(answer["errors"], json!([]));
    assert_eq!(answer["print"]["enabled"], false);
    assert_eq!(mesh_calls(&doc), 0);
    assert_eq!(doc.to_json(), before);
    assert_eq!((doc.revision(), doc.undo_depth(), doc.is_modified()), state);
}

#[test]
fn a_failed_fillet_is_an_error_and_suppression_removes_the_failure() {
    let mut doc = block();
    doc.kernel().fail.borrow_mut().insert("fillet");
    command(
        &mut doc,
        json!({"cmd":"add_feature", "name":"Edge finishing", "def":{
            "type":"fillet", "body":"F2.b0", "edges":[EDGE], "radius":3}}),
    );
    let answer = review(&doc, &["failed-feature"]);
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "failed-feature", "F3"),
        "error",
        "verification",
        "health",
    );
    command(&mut doc, json!({"cmd":"suppress_feature", "uid":"F3"}));
    assert!(findings(&review(&doc, &["failed-feature"])).is_empty());
}

#[test]
fn a_region_recovered_after_a_sketch_edit_is_reported_until_auto_fix() {
    let mut doc = block();
    let dividing = command(
        &mut doc,
        json!({"cmd":"sketch.add_line", "sketch":"F1", "start":[30,0], "end":[30,40]}),
    );
    let regions = query(&doc, json!({"query":"profiles"}));
    let left = regions
        .as_array()
        .unwrap()
        .iter()
        .find(|region| region["region"].as_str().unwrap().contains("c4["))
        .expect("the left half of the divided rectangle")["region"]
        .clone();
    command(
        &mut doc,
        json!({"cmd":"edit_feature", "uid":"F2", "def":{"type":"extrude",
            "profiles":[{"sketch":"F1", "region":left}],
            "extent":{"type":"distance", "distance":20}, "operation":"new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd":"sketch.remove", "sketch":"F1", "items":[dividing["made"][0]]}),
    );
    let answer = review(&doc, &["auto-fixable"]);
    check_finding(
        finding(&answer, "auto-fixable", "F2"),
        "warning",
        "verification",
        "health",
    );
    command(&mut doc, json!({"cmd":"auto_fix", "features":["F2"]}));
    assert!(findings(&review(&doc, &["auto-fixable"])).is_empty());
}

#[test]
fn a_free_sketch_point_and_a_fixed_point_have_different_findings() {
    let mut doc = document();
    command(
        &mut doc,
        json!({"cmd":"sketch.create", "name":"Mounting layout"}),
    );
    let point = command(
        &mut doc,
        json!({"cmd":"sketch.add_point", "sketch":"F1", "at":[7,11]}),
    );
    let checks = ["sketch-underconstrained", "sketch-fixed-heavy"];
    let free = review(&doc, &checks);
    check_finding(
        finding(&free, checks[0], "F1"),
        "warning",
        "sketches",
        "constraints",
    );
    assert_eq!(findings(&free).len(), 1, "{free}");
    command(
        &mut doc,
        json!({"cmd":"sketch.set_fixed", "sketch":"F1", "entities":point["entities"], "fixed":true}),
    );
    let fixed = review(&doc, &checks);
    check_finding(
        finding(&fixed, checks[1], "F1"),
        "warning",
        "sketches",
        "constraints",
    );
    assert_eq!(findings(&fixed).len(), 1, "{fixed}");
}

#[test]
fn a_fixed_primitive_plane_is_reported_but_an_origin_plane_is_not() {
    let mut doc = document();
    let base = stock(&mut doc, "Reference stock", [31.0, 23.0, 17.0]);
    assert!(findings(&review(&doc, &["fixed-placement"])).is_empty());
    command(
        &mut doc,
        json!({"cmd":"edit_feature", "uid":base, "def":{"type":"box",
            "plane":{"origin":[0,0,0], "normal":[0,0,1]},
            "length":31, "width":23, "height":17, "operation":"new_body"}}),
    );
    let answer = review(&doc, &["fixed-placement"]);
    check_finding(
        finding(&answer, "fixed-placement", &base),
        "warning",
        "references",
        "placement",
    );
}

#[test]
fn a_finishing_face_used_as_a_parent_is_distinct_from_modifying_the_same_body() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd":"add_feature", "name":"Round edge", "def":{
            "type":"fillet", "body":"F2.b0", "edges":[EDGE], "radius":3}}),
    );
    let rounded = doc.bodies()[0]
        .shape
        .faces
        .iter()
        .map(ToString::to_string)
        .find(|face| face.starts_with("F3:fillet("))
        .expect("the rounded face made by the fillet");
    // A direct selection of the finishing face is the fragile dependency.
    command(
        &mut doc,
        json!({"cmd":"add_feature", "name":"Opening", "def":{
            "type":"shell", "body":"F2.b0", "faces":[rounded], "inside":1}}),
    );
    let answer = review(&doc, &["volatile-parent"]);
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "volatile-parent", "F4"),
        "warning",
        "references",
        "stable-parents",
    );
    assert!(findings(&answer).iter().all(|row| row["target"] != "F3"));
}

#[test]
fn three_literal_length_slots_suggest_a_parameter_but_shared_expressions_do_not() {
    let mut doc = document();
    let cube = stock(&mut doc, "Spacer", [12.0, 12.0, 12.0]);
    let literal = review(&doc, &["repeated-literal"]);
    assert_eq!(findings(&literal).len(), 3, "{literal}");
    check_finding(
        finding(&literal, "repeated-literal", &cube),
        "info",
        "principles",
        "parameters",
    );
    command(
        &mut doc,
        json!({"cmd":"add_parameter", "name":"Side", "value":12}),
    );
    command(
        &mut doc,
        json!({"cmd":"edit_feature", "uid":cube, "def":{"type":"box", "plane":"xy",
            "length":"Side", "width":"Side", "height":"Side", "operation":"new_body"}}),
    );
    assert!(findings(&review(&doc, &["repeated-literal", "unused-parameter"])).is_empty());
}

#[test]
fn unused_user_parameters_are_global_and_do_not_leak_into_feature_scopes() {
    let mut doc = document();
    command(
        &mut doc,
        json!({"cmd":"add_parameter", "name":"UnusedClearance", "value":0.3}),
    );
    let base = stock(&mut doc, "Stock", [31.0, 23.0, 17.0]);
    let answer = review(&doc, &["unused-parameter"]);
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "unused-parameter", "UnusedClearance"),
        "info",
        "principles",
        "parameters",
    );
    for scope in [json!([base]), json!("C0"), json!([])] {
        let scoped = query(
            &doc,
            json!({"query":"design_review", "scope":scope,
            "checks":["unused-parameter"], "print":false}),
        );
        assert!(findings(&scoped).is_empty(), "{scoped}");
    }
}

#[test]
fn default_names_and_ungrouped_stages_disappear_after_organisation() {
    let mut doc = document();
    command(
        &mut doc,
        json!({"cmd":"add_feature", "def":{"type":"box", "plane":"xy",
        "length":31, "width":23, "height":17, "operation":"new_body"}}),
    );
    let checks = ["default-names", "ungrouped-timeline"];
    let answer = review(&doc, &checks);
    assert_eq!(findings(&answer).len(), 2, "{answer}");
    for rule in checks {
        check_finding(
            finding(&answer, rule, "F1"),
            "info",
            "principles",
            "organisation",
        );
    }
    command(
        &mut doc,
        json!({"cmd":"rename_feature", "uid":"F1", "name":"Mounting plate"}),
    );
    command(
        &mut doc,
        json!({"cmd":"group_features", "features":["F1"], "name":"Base shape"}),
    );
    assert!(findings(&review(&doc, &checks)).is_empty());
}

#[test]
fn component_and_feature_scopes_never_report_other_components() {
    let mut doc = document();
    let root = stock(&mut doc, "Root stock", [31.0, 23.0, 17.0]);
    let component =
        command(&mut doc, json!({"cmd":"create_component", "name":"Arm"}))["component"].clone();
    let arm = stock(&mut doc, "Arm stock", [41.0, 29.0, 19.0]);
    for scope in [component, json!("Arm"), json!([arm.clone(), arm.clone()])] {
        let answer = query(
            &doc,
            json!({"query":"design_review", "scope":scope,
            "checks":["ungrouped-timeline"], "print":false}),
        );
        assert_eq!(findings(&answer).len(), 1, "{answer}");
        assert_eq!(findings(&answer)[0]["target"], arm, "{answer}");
    }
    let answer = review(&doc, &["ungrouped-timeline"]);
    let targets: BTreeSet<_> = findings(&answer)
        .iter()
        .map(|row| row["target"].as_str().unwrap())
        .collect();
    assert_eq!(targets, BTreeSet::from([root.as_str(), arm.as_str()]));
}

#[test]
fn invalid_filters_and_scopes_are_refused_without_changing_the_design() {
    let mut doc = document();
    stock(&mut doc, "Stock", [31.0, 23.0, 17.0]);
    let before = doc.to_json();
    for options in [
        json!({"scope":"Missing component"}),
        json!({"scope":["F999"]}),
        json!({"scope":42}),
        json!({"checks":["not-a-review-rule"]}),
        json!({"print":"yes"}),
        json!({"print":42}),
        json!({"unknown":true}),
    ] {
        let mut request = options;
        request["query"] = json!("design_review");
        assert!(doc.query(&request.to_string()).is_err(), "{request}");
        assert_eq!(doc.to_json(), before);
    }
}

fn bed(doc: &mut Document<MockKernel>, uid: &str, face: &str) {
    command(
        doc,
        json!({"cmd":"set_print_bed_face", "uid":uid,
        "face":face}),
    );
}

fn printer(doc: &mut Document<MockKernel>) {
    command(
        doc,
        json!({"cmd":"create_print_profile", "kind":"printer", "name":"Bench",
        "choose":true, "values":{"bed_shape":[[-50,-50],[50,-50],[50,50],[-50,50]],
        "max_print_height":100, "nozzle_diameter":0.4}}),
    );
}

#[test]
fn print_auto_requires_context_and_false_always_avoids_meshing() {
    let mut doc = document();
    stock(&mut doc, "Print stock", [31.0, 23.0, 17.0]);
    doc.kernel().surfaces.borrow_mut().insert(
        "F1:bottom".to_owned(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    let ordinary = query(
        &doc,
        json!({"query":"design_review", "checks":["overhang", "thin-wall"]}),
    );
    assert_eq!(ordinary["print"]["enabled"], false);
    assert_eq!(mesh_calls(&doc), 0);
    bed(&mut doc, "F1.b0", "F1:bottom");
    let disabled = query(&doc, json!({"query":"design_review", "print":false}));
    assert_eq!(disabled["print"]["enabled"], false);
    assert_eq!(mesh_calls(&doc), 0);
    let admitted = query(
        &doc,
        json!({"query":"design_review", "checks":["overhang", "thin-wall"]}),
    );
    assert_eq!(admitted["print"]["enabled"], true);
    assert_eq!(admitted["print"]["bodies"], 1);
    assert!(mesh_calls(&doc) > 0);
    assert!(findings(&admitted).is_empty(), "{admitted}");
}

#[test]
fn empty_scope_or_empty_checks_never_fall_back_to_printing_all_bodies() {
    let mut doc = document();
    stock(&mut doc, "Thin stock", [31.0, 23.0, 0.4]);
    printer(&mut doc);
    for options in [json!({"scope":[]}), json!({"checks":[]})] {
        let mut request = options;
        request["query"] = json!("design_review");
        request["print"] = json!(true);
        let answer = query(&doc, request);
        assert!(findings(&answer).is_empty(), "{answer}");
        assert_eq!(answer["print"]["bodies"], 0);
        assert_eq!(mesh_calls(&doc), 0);
    }
}

#[test]
fn thin_walls_use_the_default_nozzle_and_an_explicit_request_needs_no_profile() {
    let mut doc = document();
    stock(&mut doc, "Thin cover", [31.0, 23.0, 0.4]);
    let answer = query(
        &doc,
        json!({"query":"design_review", "print":true, "checks":["thin-wall"]}),
    );
    check_finding(
        finding(&answer, "thin-wall", "F1.b0"),
        "warning",
        "3d-printing",
        "checks",
    );
    assert_eq!(answer["print"]["default_nozzle_diameter"], 0.4);
    assert_eq!(answer["print"]["default_layer_height"], 0.2);
    assert_eq!(answer["errors"], json!([]));
}

#[test]
fn print_findings_reuse_real_analysis_and_report_a_missing_bed_separately() {
    let mut doc = document();
    stock(&mut doc, "Tilted stock", [31.0, 23.0, 17.0]);
    // The synthetic kernel exposes a tilted support plane; the ordinary
    // print analysis sees the bottom resting only along an edge.
    doc.kernel().surfaces.borrow_mut().insert(
        "F1:bottom".to_owned(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [0.6, 0.0, -0.8],
        },
    );
    bed(&mut doc, "F1.b0", "F1:bottom");
    let direct = query(
        &doc,
        json!({"query":"print_overhangs", "bodies":["F1.b0"],
        "bed_face":{"body":"F1.b0", "face":"F1:bottom"}}),
    );
    assert!(direct["overhang_area"].as_f64().unwrap() > 1.0, "{direct}");
    let answer = query(
        &doc,
        json!({"query":"design_review", "checks":["overhang", "outside-build-volume"]}),
    );
    check_finding(
        finding(&answer, "overhang", "F1.b0"),
        "warning",
        "3d-printing",
        "checks",
    );
    assert!(
        findings(&answer)
            .iter()
            .all(|row| row["rule"] != "outside-build-volume")
    );
    assert!(
        answer["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["rule"] == "outside-build-volume"),
        "{answer}"
    );
}

#[test]
fn printer_context_admits_print_review_and_an_outside_body_is_named() {
    let mut doc = document();
    command(
        &mut doc,
        json!({"cmd":"add_feature", "name":"Outside stock", "def":{
        "type":"box", "plane":"xy", "corner":[120,0], "length":31, "width":23,
        "height":17, "operation":"new_body"}}),
    );
    printer(&mut doc);
    let direct = query(&doc, json!({"query":"build_volume"}));
    assert_eq!(direct["outside"][0]["body"], "F1.b0", "{direct}");
    let answer = query(
        &doc,
        json!({"query":"design_review", "checks":["outside-build-volume"]}),
    );
    check_finding(
        finding(&answer, "outside-build-volume", "F1.b0"),
        "warning",
        "3d-printing",
        "checks",
    );
    assert_eq!(answer["print"]["enabled"], true);
    assert_eq!(answer["errors"], json!([]));
    assert_eq!(mesh_calls(&doc), 0, "build-volume review needs no mesh");
}

// The checks an agent ran by hand after modelling print kits (mitcad#1731).

fn bodies(doc: &Document<MockKernel>) -> Vec<String> {
    doc.bodies()
        .iter()
        .map(|body| body.uid.to_string())
        .collect()
}

#[test]
fn an_unnamed_piece_of_a_split_beside_a_named_body_is_a_stray_body_until_named() {
    let mut doc = block();
    // The cut leaves the block in two pieces, left and right of the slot.
    *doc.kernel().piece_boxes.borrow_mut() = Some(vec![
        BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [25.0, 40.0, 20.0],
        },
        BoundingBox {
            min: [35.0, 0.0, 0.0],
            max: [60.0, 40.0, 20.0],
        },
    ]);
    command(
        &mut doc,
        json!({"cmd":"add_feature", "name":"Slot", "def":{"type":"box", "plane":"xy",
            "corner":[25,0], "length":10, "width":40, "height":20, "operation":"cut"}}),
    );
    let pieces = bodies(&doc);
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    let extra = pieces
        .iter()
        .find(|uid| *uid != "F2.b0")
        .expect("the piece the cut split off")
        .clone();
    // Nothing named yet: default names are not this rule's.
    assert!(findings(&review(&doc, &["stray-bodies"])).is_empty());
    command(
        &mut doc,
        json!({"cmd":"rename_body", "uid":"F2.b0", "name":"Base"}),
    );
    let answer = review(&doc, &["stray-bodies"]);
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "stray-bodies", &extra),
        "warning",
        "verification",
        "measure-the-result",
    );
    command(
        &mut doc,
        json!({"cmd":"rename_body", "uid":extra, "name":"Right half"}),
    );
    assert!(findings(&review(&doc, &["stray-bodies"])).is_empty());
}

#[test]
fn a_body_on_a_plate_without_a_bed_face_is_reported_until_one_is_assigned() {
    let mut doc = document();
    stock(&mut doc, "Print stock", [31.0, 23.0, 17.0]);
    doc.kernel().surfaces.borrow_mut().insert(
        "F1:bottom".to_owned(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    printer(&mut doc);
    let checks = json!({"query":"design_review", "checks":["no-bed-face"]});
    // Without plates the export lays out nothing: no finding.
    let unplated = query(&doc, checks.clone());
    assert_eq!(unplated["print"]["enabled"], true, "{unplated}");
    assert!(findings(&unplated).is_empty(), "{unplated}");
    command(
        &mut doc,
        json!({"cmd":"add_print_plate", "name":"Bench plate"}),
    );
    let answer = query(&doc, checks.clone());
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "no-bed-face", "F1.b0"),
        "warning",
        "3d-printing",
        "orientation-and-strength",
    );
    bed(&mut doc, "F1.b0", "F1:bottom");
    let answer = query(&doc, checks);
    assert!(findings(&answer).is_empty(), "{answer}");
    assert_eq!(mesh_calls(&doc), 0, "the plate checks need no mesh");
    // print false runs no print checks.
    let disabled = query(
        &doc,
        json!({"query":"design_review", "checks":["no-bed-face"], "print":false}),
    );
    assert_eq!(disabled["print"]["enabled"], false);
}

#[test]
fn a_plate_without_a_printer_profile_is_reported_until_one_is_chosen() {
    let mut doc = document();
    stock(&mut doc, "Print stock", [31.0, 23.0, 17.0]);
    let checks = json!({"query":"design_review", "checks":["no-print-profile"]});
    // No plate, no printer: nothing is prepared for printing.
    assert!(findings(&query(&doc, checks.clone())).is_empty());
    let plate = command(
        &mut doc,
        json!({"cmd":"add_print_plate", "name":"Bench plate"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let answer = query(&doc, checks.clone());
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "no-print-profile", &plate),
        "warning",
        "3d-printing",
        "checks",
    );
    printer(&mut doc);
    let answer = query(&doc, checks);
    assert!(findings(&answer).is_empty(), "{answer}");
    assert_eq!(mesh_calls(&doc), 0, "the plate checks need no mesh");
}

#[test]
fn a_profile_region_split_by_a_sketch_change_is_reported_until_auto_fix() {
    let mut doc = block();
    assert!(findings(&review(&doc, &["split-profile"])).is_empty());
    // A line across the rectangle splits the extruded region in two.
    command(
        &mut doc,
        json!({"cmd":"sketch.add_line", "sketch":"F1", "start":[30,0], "end":[30,40]}),
    );
    let answer = review(&doc, &["split-profile"]);
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "split-profile", "F2"),
        "warning",
        "sketches",
        "profiles-and-the-sketch-palette",
    );
    // Auto Fix selects the pieces found: the reference is exact again.
    command(&mut doc, json!({"cmd":"auto_fix", "features":["F2"]}));
    assert!(findings(&review(&doc, &["split-profile"])).is_empty());
}

#[test]
fn two_selected_regions_merged_by_a_sketch_change_are_a_split_profile() {
    let mut doc = document();
    command(&mut doc, json!({"cmd":"sketch.create"}));
    command(
        &mut doc,
        json!({"cmd":"sketch.add_rectangle", "sketch":"F1", "corner":[0,0],
            "width":60, "height":40}),
    );
    let dividing = command(
        &mut doc,
        json!({"cmd":"sketch.add_line", "sketch":"F1", "start":[30,0], "end":[30,40]}),
    );
    let halves: Vec<Value> = query(&doc, json!({"query":"profiles"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|region| json!({"sketch":"F1", "region":region["region"]}))
        .collect();
    assert_eq!(halves.len(), 2, "{halves:?}");
    command(
        &mut doc,
        json!({"cmd":"add_feature", "def":{"type":"extrude", "profiles":halves,
            "extent":{"type":"distance", "distance":20}, "operation":"new_body"}}),
    );
    assert!(findings(&review(&doc, &["split-profile"])).is_empty());
    command(
        &mut doc,
        json!({"cmd":"sketch.remove", "sketch":"F1", "items":[dividing["made"][0]]}),
    );
    let answer = review(&doc, &["split-profile"]);
    assert_eq!(findings(&answer).len(), 1, "{answer}");
    check_finding(
        finding(&answer, "split-profile", "F2"),
        "warning",
        "sketches",
        "profiles-and-the-sketch-palette",
    );
}
