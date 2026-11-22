// SPDX-License-Identifier: MIT
//! A loft's Tangent Edges (mitcad#2162) in documents with the mock kernel:
//! `merge` reaches the kernel, a loft without the field (a design saved
//! before the choice) builds as before, and the field survives saving and
//! loading. The geometry of a merged side is checked in
//! `geometry.loft_tangent_edges` and `cli.f3_loft_tangent_edges`.

use serde_json::{Value, json};

use mitcad_model::test_support::{block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

/// The specs of the lofts the kernel was asked for.
fn loft_specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|spec| spec.starts_with("loft ") || spec.starts_with("loft_sheet("))
        .cloned()
        .collect()
}

/// The block's top face to a circle on a sketch of its own (F3), the
/// loft's own fields added.
fn loft(extra: Value) -> Value {
    let mut value = json!({"type": "loft", "sections": [
        {"type": "profile", "sketch": "F3", "region": "r{c1}"},
        {"type": "profile", "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
        "operation": "new_body"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

fn with_circle() -> Document<MockKernel> {
    let mut b = block();
    command(&mut b.doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    command(
        &mut b.doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": 10}),
    );
    b.doc
}

#[test]
fn merge_reaches_the_kernel_and_keep_is_the_field_left_out() {
    let mut doc = with_circle();
    let merged = doc
        .add_feature(&def(loft(json!({"tangent_edges": "merge"}))), None)
        .unwrap()
        .uid;
    assert_eq!(doc.status(merged), Some(&FeatureStatus::Ok));
    let specs = loft_specs(&doc);
    assert!(
        specs.last().unwrap().ends_with(" tangent_edges merge"),
        "{specs:?}"
    );
    // Without the field, and with `keep`: as before, the field left out of
    // the definition.
    for extra in [json!({}), json!({"tangent_edges": "keep"})] {
        let uid = doc.add_feature(&def(loft(extra)), None).unwrap().uid;
        assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
        let specs = loft_specs(&doc);
        assert!(
            !specs.last().unwrap().contains("tangent_edges"),
            "{specs:?}"
        );
        let answer = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
        assert!(answer["def"].get("tangent_edges").is_none(), "{answer}");
    }
    let answer = query(&doc, json!({"query": "feature", "uid": merged.to_string()}));
    assert_eq!(answer["def"]["tangent_edges"], "merge", "{answer}");
    // Anything else is refused.
    let refused = json!({"cmd": "add_feature", "def": loft(json!({"tangent_edges": "smooth"}))});
    assert!(doc.command(&refused.to_string()).is_err());
}

#[test]
fn the_field_survives_saving_and_loading_and_a_design_without_it_builds_as_before() {
    let mut doc = with_circle();
    let merged = doc
        .add_feature(&def(loft(json!({"tangent_edges": "merge"}))), None)
        .unwrap()
        .uid;
    let kept = doc.add_feature(&def(loft(json!({}))), None).unwrap().uid;
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let features = file["features"].as_array().unwrap();
    let find = |uid: &str| features.iter().find(|f| f["uid"] == uid).unwrap();
    assert_eq!(find(&merged.to_string())["tangent_edges"], "merge");
    assert!(find(&kept.to_string()).get("tangent_edges").is_none());

    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
    command(&mut opened, json!({"cmd": "recompute"}));
    for (uid, merge) in [(merged, true), (kept, false)] {
        assert_eq!(opened.status(uid), Some(&FeatureStatus::Ok));
        let answer = query(&opened, json!({"query": "feature", "uid": uid.to_string()}));
        assert_eq!(
            answer["def"].get("tangent_edges") == Some(&json!("merge")),
            merge,
            "{answer}"
        );
    }
    // The kernel is asked as before: merged only where the file says so.
    let specs = loft_specs(&opened);
    assert_eq!(specs.len(), 2, "{specs:?}");
    assert!(specs[0].ends_with(" tangent_edges merge"), "{specs:?}");
    assert!(!specs[1].contains("tangent_edges"), "{specs:?}");
}

#[test]
fn a_surface_loft_of_curves_takes_it_too() {
    let mut doc = with_circle();
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F4", "center": [0, 5], "diameter": 10}),
    );
    let uid = doc
        .add_feature(
            &def(json!({"type": "loft", "sections": [
                {"type": "curves", "sketch": "F3", "curves": ["c1"]},
                {"type": "curves", "sketch": "F4", "curves": ["c1"]}],
                "surface": true, "tangent_edges": "merge", "operation": "new_body"})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    let specs = loft_specs(&doc);
    assert!(
        specs.last().unwrap().ends_with(" tangent_edges merge)"),
        "{specs:?}"
    );
}
