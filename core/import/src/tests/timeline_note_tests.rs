// SPDX-License-Identifier: MIT
//! The import's outcomes on the timeline (mitcad#170) on synthetic dumps
//! with the mock kernel: fallbacks' base features, partial items and
//! placeholders of items suppressed or failed in the file, with the
//! report's texts.

use super::*;
use mitcad_model_api::DocumentApi;

/// The timeline query's entry of a feature.
fn timeline_item(doc: &Document<TestKernel>, name: &str) -> Value {
    let timeline: Value =
        serde_json::from_str(&doc.query(r#"{"query": "timeline"}"#).unwrap()).unwrap();
    timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no {name} in {timeline}"))
}

#[test]
fn a_fallback_stands_in_for_its_items_with_their_reasons() {
    wall_clock();
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let options = Options {
        time_limit: Some(0.0),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let text = report.text();
    let fallbacks: Vec<&ItemReport> = report
        .items
        .iter()
        .filter(|i| i.outcome == Outcome::Fallback)
        .collect();
    assert_eq!(fallbacks.len(), 2, "{text}");
    // Each fallback item is named, with its note, by a base feature that
    // is yellow with that message; the report's note is the reason.
    let bases: Vec<Value> = doc
        .features()
        .filter(|f| f.def.type_name() == "base")
        .map(|f| timeline_item(&doc, &f.name))
        .collect();
    assert!(!bases.is_empty(), "{text}");
    // The note is information on the feature (mitcad#1040): it stays ok;
    // the import's own notices carry the fallback as a warning.
    for base in &bases {
        assert_eq!(base["status"], "ok", "{base}");
        assert_eq!(base["level"], "info", "{base}");
        assert!(
            base["import"]
                .as_str()
                .unwrap()
                .starts_with("Stands in for "),
            "{base}"
        );
        assert_eq!(base["notices"][0]["message"], base["import"], "{base}");
        assert_eq!(base["error"], Value::Null, "{base}");
    }
    for item in fallbacks {
        let note = item.note.as_deref().unwrap();
        let reason = note.split("; in the fallback of").next().unwrap();
        let named = format!("{} ({reason}", item.name);
        assert!(
            bases
                .iter()
                .any(|b| b["import"].as_str().unwrap().contains(&named)),
            "{named} in {bases:?}\n{text}"
        );
    }
    let design_notices = report.notices();
    assert!(
        design_notices
            .iter()
            .any(|n| n.level == mitcad_model::NoticeLevel::Warning
                && n.message == "2 items replaced by the file's bodies"),
        "{design_notices:?}"
    );
    // The partial sketch says what was left out, as the report does.
    let sketch = timeline_item(&doc, "Sketch1");
    assert_eq!(sketch["level"], "info", "{sketch}");
    assert_eq!(
        sketch["import"],
        format!(
            "Partly imported: {}",
            report.items[0].note.as_deref().unwrap()
        ),
        "{text}"
    );
}

#[test]
fn a_parametric_item_has_no_note_and_an_edit_clears_one() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_dump());
    let text = report.text();
    assert_eq!(report.items[1].outcome, Outcome::Parametric, "{text}");
    let extrude = &report.items[1].name;
    let item = |doc: &Document<MockKernel>, name: &str| -> Value {
        let timeline: Value =
            serde_json::from_str(&doc.query(r#"{"query": "timeline"}"#).unwrap()).unwrap();
        timeline["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .cloned()
            .unwrap()
    };
    assert_eq!(item(&doc, extrude)["status"], "ok", "{text}");
    assert!(item(&doc, extrude).get("import").is_none());
    // The partial sketch: its dimension edited, it builds, and its note
    // goes.
    let sketch = item(&doc, "Sketch1");
    assert_eq!(sketch["level"], "info", "{sketch}");
    doc.set_parameter("d1", 45.0).unwrap();
    assert_eq!(item(&doc, "Sketch1")["status"], "ok", "{text}");
    assert!(item(&doc, "Sketch1").get("import").is_none());
}

#[test]
fn items_suppressed_or_failed_in_the_file_become_placeholders() {
    // (An item whose change is in other components; a suppressed one
    // whose definition does not come in: those that do are in
    // `suppressed_tests`, mitcad#180.)
    let mut fillet = fillet_item(2);
    fillet["_f3d"] = json!({"result_no": -1, "flags": "000001"});
    let mut suppressed = extrude(3, 0, "JoinFeatureOperation", "d2");
    suppressed["isSuppressed"] = json!(true);
    suppressed["detail"] = Value::Null;
    // An item without geometry gets none.
    let mut group = json!({"index": 4, "name": "Group1", "objectType": "Group"});
    group["isSuppressed"] = json!(true);
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet,
        suppressed,
        group,
        extrude(5, 0, "JoinFeatureOperation", "d2"),
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let text = report.text();
    let names: Vec<String> = doc.features().map(|f| f.name.clone()).collect();
    // At their places in the timeline, suppressed, building nothing.
    assert_eq!(
        names,
        ["Sketch1", "Extrude1", "Fillet1", "Extrude3", "Extrude5"],
        "{text}"
    );
    let fillet = timeline_item(&doc, "Fillet1");
    assert_eq!(fillet["type"], "placeholder", "{fillet}");
    assert_eq!(fillet["status"], "suppressed", "{fillet}");
    // A state is no severity: the note is information (mitcad#1040).
    assert_eq!(fillet["level"], "info", "{fillet}");
    assert_eq!(
        fillet["import"],
        mitcad_import_replay::internal::timeline_notes::sentence(
            report.items[2].note.as_deref().unwrap()
        ),
        "{text}"
    );
    assert!(
        fillet["import"]
            .as_str()
            .unwrap()
            .starts_with("The file keeps no result for it"),
        "{fillet}"
    );
    let extrude3 = timeline_item(&doc, "Extrude3");
    assert_eq!(extrude3["type"], "placeholder");
    assert_eq!(extrude3["suppressed"], true);
    assert_eq!(
        extrude3["import"],
        "Suppressed in the file; its definition did not come in: ExtrudeFeature: no details \
         decoded"
    );
    assert_eq!(timeline_item(&doc, "Extrude5")["status"], "ok", "{text}");
    // The report is as before: skipped, with the notes the timeline shows.
    assert_eq!(report.items[3].outcome, Outcome::Skipped);
    assert_eq!(
        report.items[3].note.as_deref(),
        Some(
            "suppressed in the file; its definition did not come in: ExtrudeFeature: no details decoded"
        )
    );
    assert!(report.items[3].features.is_empty());
    assert_eq!(doc.bodies().len(), 1, "{text}");
    // Saved and opened again, the timeline says the same.
    let reopened_text = doc.to_json();
    let mut reopened = Document::from_json(&reopened_text, TestKernel::default()).unwrap();
    reopened.recompute();
    for name in ["Sketch1", "Fillet1", "Extrude3"] {
        assert_eq!(
            timeline_item(&reopened, name),
            timeline_item(&doc, name),
            "{name}"
        );
    }
}
