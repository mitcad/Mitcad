// SPDX-License-Identifier: MIT
//! Library sources with the mock kernel: kept once in the design (inside a
//! single file, in a project's store as a reference), removed with their
//! last derived part, moved to another version in one undo step, listed
//! with the library parts, and their supplier columns in the parts list;
//! mount references in files and copies; the classes of the commands.

use serde_json::json;

use mitcad_model::features::GeomRef;
use mitcad_model::file::MemoryStore;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::classes::{ApiClass, command_class_with};
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

#[test]
fn a_project_keeps_the_source_s_design_in_its_store() {
    let mut doc = design();
    first(&mut doc, "p30", [0.0; 3], 1000.0);
    let store = MemoryStore::new();
    let v3 = doc.to_project_json(&store).unwrap();
    assert!(v3.contains("\"version\": 3"), "{v3}");
    assert!(!v3.contains("\"text\""), "{v3}");
    assert_eq!(store.len(), 1);
    let references = mitcad_model::file::brep_references(&v3).unwrap();
    assert_eq!(references.len(), 1);
    let mut reopened =
        mitcad_model::Document::from_json_in(&v3, &store, MockKernel::default()).unwrap();
    reopened.recompute();
    all_ok(&reopened);
    // Without the store the derived part fails, saying why.
    let mut lost = mitcad_model::Document::from_json(&v3, MockKernel::default()).unwrap();
    lost.recompute();
    let part = derived(&lost, lost.assembly().components[0].uid);
    let error = lost
        .status(part)
        .and_then(|s| s.error())
        .unwrap_or_default()
        .to_owned();
    assert!(error.contains("missing from the project store"), "{error}");
    // A hand-written file may give the design as an object.
    let text = doc.to_json();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let design_text = value["library_sources"][0]["design"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let object: serde_json::Value = serde_json::from_str(&design_text).unwrap();
    value["library_sources"][0]["design"] = json!({"text": object});
    let mut written =
        mitcad_model::Document::from_json(&value.to_string(), MockKernel::default()).unwrap();
    written.recompute();
    all_ok(&written);
}

#[test]
fn a_source_goes_with_its_last_part() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    place(
        &mut doc,
        json!({"part": part("cap30"), "at": {"point": [100, 0, 0]}}),
    );
    assert_eq!(doc.state().library_sources.0.len(), 2);
    command(
        &mut doc,
        json!({"cmd": "build_remove", "component": t["component"]}),
    )
    .unwrap();
    assert_eq!(doc.state().library_sources.0.len(), 1);
    assert_eq!(doc.state().library_sources.0[0].library.component, "cap30");
}

#[test]
fn a_source_no_part_has_used_stays() {
    let mut doc = design();
    first(&mut doc, "p30", [0.0; 3], 1000.0);
    // A file that brings a second source no part uses yet.
    let mut value: serde_json::Value = serde_json::from_str(&doc.to_json()).unwrap();
    let mut unused = value["library_sources"][0].clone();
    unused["id"] = json!("S2");
    value["library_sources"]
        .as_array_mut()
        .unwrap()
        .push(unused);
    let mut opened =
        mitcad_model::Document::from_json(&value.to_string(), MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(opened.state().library_sources.0.len(), 2);
    // An edit that does not use it keeps it.
    command(
        &mut opened,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"point": [0, 200, 0]}, "length": 500}),
    )
    .unwrap();
    assert_eq!(opened.state().library_sources.0.len(), 2);
    // Used and then removed, it goes with its last part.
    let answer = command(
        &mut opened,
        json!({"cmd": "build_place", "part": {"source": "S2"},
               "at": {"point": [0, 400, 0]}, "length": 300}),
    )
    .unwrap();
    assert_eq!(opened.state().library_sources.0.len(), 2);
    command(
        &mut opened,
        json!({"cmd": "build_remove", "component": answer["component"]}),
    )
    .unwrap();
    let ids: Vec<String> = opened
        .state()
        .library_sources
        .0
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    assert_eq!(ids, ["S1"]);
}

#[test]
fn a_source_moves_to_another_version_in_one_step() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let width = |doc: &mitcad_model::Document<MockKernel>| {
        let b = doc.part_bounds(component(&t)).unwrap();
        b.max[0] - b.min[0]
    };
    assert!((width(&doc) - 30.0).abs() < 1e-9);
    let answer = command(
        &mut doc,
        json!({"cmd": "update_library_parts", "changes": [{"source": "S1", "rev": "v1.1.0"}]}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(
        answer["changes"],
        json!(["Profile 30x30 B8: version v1.0.0 (1111111) -> v1.1.0 (2222222)"])
    );
    assert_eq!(doc.undo_label(), Some("Update Profile 30x30 B8 to v1.1.0"));
    assert!((width(&doc) - 32.0).abs() < 1e-9, "{}", width(&doc));
    assert_eq!(doc.state().library_sources.0[0].part_number, "P30_N8_B_V2");
    doc.undo();
    assert!((width(&doc) - 30.0).abs() < 1e-9);
    // Listed with the library parts: the derived parts it makes.
    let parts = query(&doc, json!({"query": "library_parts"}));
    let source = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["source"] == "S1")
        .unwrap();
    assert_eq!(source["uses"], json!([tp.to_string()]));
    assert_eq!(source["library"]["component"], "p30");
    assert_eq!(source["supplier"], "Test supplier");
    assert_eq!(source["version"], "v1.0.0 (1111111)");
    // The parts list's supplier columns.
    let list = query(&doc, json!({"query": "parts_list"}));
    let row = &list["rows"][0];
    assert_eq!(row["supplier"], "Test supplier");
    assert_eq!(row["part_number"], "P30_N8_B");
    assert_eq!(row["designation"], "Profile 30x30 B8 L=1000");
    // A source and a component in one change are refused.
    let error = command(
        &mut doc,
        json!({"cmd": "update_library_parts", "changes": [{"source": "S1", "component": "C1"}]}),
    )
    .unwrap_err();
    assert!(error.contains("names a source or a component"), "{error}");
}

#[test]
fn mount_references_read_write_and_follow_copies() {
    let reference: GeomRef =
        serde_json::from_value(json!({"mount": "F7:mount(slot_x+_1)"})).unwrap();
    assert_eq!(
        reference,
        GeomRef::Mount {
            feature: mitcad_model::ids::FeatureUid(7),
            name: "slot_x+_1".to_owned()
        }
    );
    assert_eq!(
        serde_json::to_value(&reference).unwrap(),
        json!({"mount": "F7:mount(slot_x+_1)"})
    );
    assert!(serde_json::from_value::<GeomRef>(json!({"mount": "F7:mount()"})).is_err());
    // A derived part's faces name its source's faces in the source's ids:
    // they are no features of the design.
    let face: mitcad_model::topo::FaceName = "F7:part(F2:side(c1))".parse().unwrap();
    assert_eq!(
        face.features(),
        std::collections::BTreeSet::from([mitcad_model::ids::FeatureUid(7)])
    );
    // A copy of a placed part (Paste New) keeps its derived part.
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    command(
        &mut doc,
        json!({"cmd": "add_feature", "component": t["component"],
               "def": {"type": "hole", "placement": {"type": "plane", "plane": {"mount": format!("{tp}:mount(end_b)")},
                                                      "points": [[0, 0, 1000]]},
                       "diameter": 5, "extent": {"type": "distance", "depth": 10}}}),
    )
    .unwrap();
    all_ok(&doc);
    let pasted = command(
        &mut doc,
        json!({"cmd": "paste_new", "occurrence": t["occurrence"]}),
    )
    .unwrap();
    all_ok(&doc);
    let text = doc.to_json();
    assert!(text.matches("\"derived_part\"").count() == 2, "{pasted}");
}

#[test]
fn placing_from_a_source_of_the_design_is_the_model_s_own() {
    assert_eq!(
        command_class_with(&json!({"cmd": "build_place", "part": {"source": "S1"}})),
        Some(ApiClass::Model)
    );
    assert_eq!(
        command_class_with(
            &json!({"cmd": "build_place", "part": {"library": "x", "component": "y"}})
        ),
        Some(ApiClass::Remote)
    );
    assert_eq!(
        command_class_with(&json!({"cmd": "build_edit", "component": "C1"})),
        Some(ApiClass::Model)
    );
    assert_eq!(
        mitcad_model_api::classes::query_class("build_cut_list"),
        Some(ApiClass::Model)
    );
}

#[test]
fn the_library_usage_counts_build_parts_by_their_source() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"point": [0, 200, 0]}, "length": 500}),
    )
    .unwrap();
    place(
        &mut doc,
        json!({"part": part("cap30"), "at": {"point": [100, 0, 0]}}),
    );
    let usage = |doc: &mitcad_model::Document<MockKernel>| -> Vec<(String, String, u64)> {
        query(doc, json!({"query": "library_usage"}))["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                assert_eq!(p["library"], LIBRARY);
                assert_eq!(p["url"], URL);
                (
                    p["library"].as_str().unwrap().to_owned(),
                    p["component"].as_str().unwrap().to_owned(),
                    p["count"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let entry = |component: &str, count: u64| (LIBRARY.to_owned(), component.to_owned(), count);
    assert_eq!(usage(&doc), [entry("p30", 2), entry("cap30", 1)]);
    // A part removed is counted no more.
    command(
        &mut doc,
        json!({"cmd": "build_remove", "component": t["component"]}),
    )
    .unwrap();
    assert_eq!(usage(&doc), [entry("p30", 1), entry("cap30", 1)]);
}

#[test]
fn removing_an_excluded_part_drops_the_cut_s_exclusion_with_a_notice() {
    use mitcad_model::features::SketchPlane;
    use mitcad_model::test_support::num;
    // mitcad#1236: a cut in Holder whose automatic scope excludes a BUILD
    // part's occurrence.
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let holder = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Holder"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 10, "width": 10, "height": 10, "operation": "new_body"}}),
    )
    .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 60}, "operation": "cut",
               "cut_scope": {"context": {"component": holder["component"],
                                         "occurrence": holder["occurrence"]},
                   "targets": {"type": "automatic", "excluded": [
                       {"component": t["component"], "occurrence": t["occurrence"]}]}}}}),
    )
    .unwrap();
    let cut = added["uid"].clone();
    all_ok(&doc);
    let reply = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": t["component"]}),
    )
    .unwrap();
    let notice = reply["notices"]
        .as_array()
        .unwrap_or_else(|| panic!("{reply}"))
        .iter()
        .find(|n| n["target"] == cut)
        .unwrap_or_else(|| panic!("{reply}"));
    assert_eq!(notice["level"], "info");
    assert!(
        notice["message"].as_str().unwrap().contains("dropped"),
        "{notice}"
    );
    // The cut stays and keeps working.
    let deleted = reply["removed"]["features"].as_array().unwrap();
    assert!(!deleted.contains(&cut), "{reply}");
    all_ok(&doc);
}
