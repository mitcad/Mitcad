// SPDX-License-Identifier: MIT
//! Items suppressed in the file (mitcad#180) on synthetic dumps with the
//! mock kernel: suppressed features with their definitions that build when
//! unsuppressed, runs of them, and placeholders where a definition does not
//! come in.

use super::*;
use mitcad_import_replay::internal::suppressed::{
    ROLLED_BACK_UNCHECKED, SUPPRESSED_UNCHECKED, SuppressedReport,
};
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

/// The volume of the bodies at the marker.
fn volume(doc: &Document<TestKernel>) -> f64 {
    bodies_of(doc).iter().map(|(_, s)| s.volume).sum()
}

/// The volume (the mock kernel's measure) of the bodies `items` make,
/// none of them suppressed.
fn volume_of(items: Vec<Value>) -> f64 {
    let mut doc = Document::new(TestKernel::default());
    import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    volume(&doc)
}

/// An extrusion of sketch `sketch`, `mm` long, suppressed in the file by
/// its state bytes (`flags`, as the stream decoder gives them).
fn suppressed_extrude(index: i64, sketch: i64, operation: &str, mm: f64, flags: &str) -> Value {
    let mut item = extrude(index, sketch, operation, &format!("d{}", 10 + index));
    item["detail"]["extentOne"]["distance"] = param(
        &format!("d{}", 10 + index),
        &format!("{mm} mm"),
        mm / 10.0,
        "mm",
    );
    item["_f3d"]["result_no"] = json!(-1);
    item["_f3d"]["flags"] = json!(flags);
    item
}

#[test]
fn a_suppressed_item_comes_in_as_a_suppressed_feature_that_builds_when_unsuppressed() {
    // The block, a join 30 mm long suppressed in the file, a fillet
    // suppressed in the file whose edges are not decoded, then a join.
    let mut fillet = fillet_item(3);
    fillet["_f3d"] = json!({"result_no": -1, "flags": "010000"});
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        suppressed_extrude(2, 0, "JoinFeatureOperation", 30.0, "000100"),
        fillet,
        extrude(4, 0, "JoinFeatureOperation", "d2"),
    ];
    let mut doc = Document::new(TestKernel::default());
    let dir = mitcad_model::testing::unique_temp_path("learn-suppressed");
    let options = Options {
        learn: Some(crate::learn::LearnOptions {
            dir: dir.clone(),
            path: None,
        }),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut NoGeometry, &options);
    let lines: Vec<Value> = std::fs::read_dir(dir.join("unsettled"))
        .unwrap()
        .flat_map(|e| {
            std::fs::read_to_string(e.unwrap().path())
                .unwrap()
                .lines()
                .map(|s| serde_json::from_str::<Value>(s).unwrap())
                .collect::<Vec<_>>()
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    let text = report.text();
    // The join: parametric, not checked, suppressed.
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{text}");
    assert_eq!(item.verified, None, "{text}");
    assert_eq!(item.input_provenance, Some(InputProvenance::Unknown));
    let learned = lines.iter().find(|line| line["item"] == 2).unwrap();
    assert_eq!(learned["status"], "unsettled");
    assert_eq!(learned["answer"]["input_provenance"], "unknown");
    assert_eq!(learned["answer"]["defs"][0]["type"], "extrude");
    assert_eq!(item.note.as_deref(), Some(SUPPRESSED_UNCHECKED), "{text}");
    assert_eq!(item.features.len(), 1, "{text}");
    let uid: FeatureUid = item.features[0].parse().unwrap();
    assert!(doc.feature(uid).unwrap().suppressed, "{text}");
    let entry = timeline_item(&doc, "Extrude2");
    assert_eq!(entry["type"], "extrude", "{entry}");
    assert_eq!(entry["status"], "suppressed", "{entry}");
    // The fillet: a placeholder with the reason.
    let item = &report.items[3];
    assert_eq!(item.outcome, Outcome::Skipped, "{text}");
    assert_eq!(item.input_provenance, None);
    assert!(
        item.note.as_deref().is_some_and(
            |n| n.starts_with("suppressed in the file; its definition did not come in")
        ),
        "{text}"
    );
    assert_eq!(timeline_item(&doc, "Fillet1")["type"], "placeholder");
    assert_eq!(
        report.suppressed,
        SuppressedReport {
            suppressed_as_features: 1,
            suppressed_as_placeholders: 1,
            // The fillet (`010000`).
            suppressed_indirectly: 1,
            ..SuppressedReport::default()
        }
    );
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.ends_with("(suppressed through an item it depends on)")),
        "{text}"
    );
    assert!(
        text.contains("(1 of them through another suppressed item)"),
        "{text}"
    );
    assert!(
        text.contains("suppressed in the file: 1 modeling items as suppressed features, 1 as"),
        "{text}"
    );
    // The bodies are the file's: the join after it is the last item.
    assert_eq!(report.items[4].outcome, Outcome::Parametric, "{text}");
    assert_eq!(doc.bodies().len(), 1, "{text}");
    let block = volume(&doc);
    assert!(
        (block
            - volume_of(vec![
                rectangle_sketch(0),
                extrude(1, 0, "NewBodyFeatureOperation", "d2"),
                extrude(4, 0, "JoinFeatureOperation", "d2")
            ]))
        .abs()
            < 1e-9,
        "{block}: {text}"
    );
    // Unsuppressed, it builds: the block 30 mm high, its note still there
    // (not checked) as information (mitcad#1040).
    doc.set_suppressed(uid, false).unwrap();
    let entry = timeline_item(&doc, "Extrude2");
    assert_eq!(entry["status"], "ok", "{entry}");
    assert_eq!(entry["level"], "info", "{entry}");
    assert_eq!(
        entry["import"], "Suppressed in the file; not checked against the file's history",
        "{entry}"
    );
    let unsuppressed = volume(&doc);
    assert!(
        (unsuppressed
            - volume_of(vec![
                rectangle_sketch(0),
                extrude(1, 0, "NewBodyFeatureOperation", "d2"),
                suppressed_extrude(2, 0, "JoinFeatureOperation", 30.0, "000000"),
                extrude(4, 0, "JoinFeatureOperation", "d2")
            ]))
        .abs()
            < 1e-9,
        "{unsuppressed}: {text}"
    );
}

#[test]
fn items_suppressed_in_a_row_are_built_on_each_other_and_suppressed_together() {
    // A sketch and its extrusion, both suppressed in the file (the
    // extrusion needs the sketch built), a second suppressed join, then a
    // join that is not suppressed.
    let mut sketch = rectangle_sketch(2);
    sketch["_f3d"] = json!({"result_no": -1, "flags": "000100"});
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        sketch,
        suppressed_extrude(3, 2, "JoinFeatureOperation", 30.0, "010000"),
        suppressed_extrude(4, 0, "JoinFeatureOperation", 50.0, "010000"),
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
    for i in [2, 3, 4] {
        let uid: FeatureUid = report.items[i].features[0].parse().unwrap();
        assert!(doc.feature(uid).unwrap().suppressed, "{i}: {text}");
    }
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.ends_with("suppressed in the file")),
        "{text}"
    );
    // (The sketch came in partly: its note says both.)
    let sketch = timeline_item(&doc, "Sketch3");
    assert_eq!(sketch["status"], "suppressed", "{sketch}");
    assert!(
        sketch["import"]
            .as_str()
            .is_some_and(|e| e.ends_with("; suppressed in the file")),
        "{sketch}"
    );
    assert_eq!(report.suppressed.suppressed_as_features, 2, "{text}");
    assert_eq!(report.items[5].outcome, Outcome::Parametric, "{text}");
    let block = volume(&doc);
    assert!(
        (block
            - volume_of(vec![
                rectangle_sketch(0),
                extrude(1, 0, "NewBodyFeatureOperation", "d2"),
                extrude(5, 0, "JoinFeatureOperation", "d2")
            ]))
        .abs()
            < 1e-9,
        "{block}: {text}"
    );
    assert!(report.warnings.is_empty(), "{text}");
    // The sketch and the first extrusion unsuppressed: it builds.
    for i in [2, 3] {
        let uid: FeatureUid = report.items[i].features[0].parse().unwrap();
        doc.set_suppressed(uid, false).unwrap();
    }
    assert_eq!(timeline_item(&doc, "Extrude3")["level"], "info");
    let unsuppressed = volume(&doc);
    assert!(
        (unsuppressed
            - volume_of(vec![
                rectangle_sketch(0),
                extrude(1, 0, "NewBodyFeatureOperation", "d2"),
                rectangle_sketch(2),
                suppressed_extrude(3, 2, "JoinFeatureOperation", 30.0, "000000"),
                extrude(5, 0, "JoinFeatureOperation", "d2")
            ]))
        .abs()
            < 1e-9,
        "{unsuppressed}: {text}"
    );
}

#[test]
fn a_suppressed_item_takes_no_guess() {
    // A fillet with only guessed edges stays a placeholder: nothing could
    // check a guess.
    let mut fillet = fillet_item(2);
    fillet["isSuppressed"] = json!(true);
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet,
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let text = report.text();
    assert_eq!(report.items[2].outcome, Outcome::Skipped, "{text}");
    assert!(report.items[2].features.is_empty(), "{text}");
    assert_eq!(report.suppressed.suppressed_as_placeholders, 1, "{text}");
    assert_eq!(timeline_item(&doc, "Fillet1")["status"], "suppressed");
}

/// An extrusion after the timeline marker in the file.
fn rolled_back_extrude(index: i64, sketch: i64, operation: &str, mm: f64) -> Value {
    let mut item = suppressed_extrude(index, sketch, operation, mm, "000000");
    item["isRolledBack"] = json!(true);
    item
}

#[test]
fn items_after_the_file_marker_come_in_as_features_after_the_marker() {
    // The block, a join 30 mm long and a fillet whose edges are not
    // decoded, both after the file's marker.
    let mut fillet = fillet_item(3);
    fillet["isRolledBack"] = json!(true);
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        rolled_back_extrude(2, 0, "JoinFeatureOperation", 30.0),
        fillet,
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let text = report.text();
    // The join: a feature, not checked, after the marker.
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{text}");
    assert_eq!(item.verified, None, "{text}");
    assert_eq!(item.note.as_deref(), Some(ROLLED_BACK_UNCHECKED), "{text}");
    let entry = timeline_item(&doc, "Extrude2");
    assert_eq!(entry["type"], "extrude", "{entry}");
    assert_eq!(entry["status"], "rolled_back", "{entry}");
    // The fillet: a placeholder with the reason.
    let item = &report.items[3];
    assert_eq!(item.outcome, Outcome::Skipped, "{text}");
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.starts_with("rolled back in the file")
                && n.contains("its definition did not come in")),
        "{text}"
    );
    assert_eq!(
        report.suppressed,
        SuppressedReport {
            rolled_back_as_features: 1,
            rolled_back_as_placeholders: 1,
            ..SuppressedReport::default()
        }
    );
    assert!(
        text.contains("rolled back in the file: 1 modeling items as features after the marker"),
        "{text}"
    );
    // The marker is before the join: the sketch and the block.
    assert_eq!(doc.marker(), 2, "{text}");
    assert_eq!(doc.bodies().len(), 1, "{text}");
    let block = volume(&doc);
    assert!(
        (block
            - volume_of(vec![
                rectangle_sketch(0),
                extrude(1, 0, "NewBodyFeatureOperation", "d2"),
            ]))
        .abs()
            < 1e-9,
        "{block}: {text}"
    );
    // Moved past it, the join builds: the block 30 mm high.
    let features = doc.features().count();
    doc.set_marker(features).unwrap();
    let joined = volume(&doc);
    assert!(
        (joined
            - volume_of(vec![
                rectangle_sketch(0),
                extrude(1, 0, "NewBodyFeatureOperation", "d2"),
                suppressed_extrude(2, 0, "JoinFeatureOperation", 30.0, "000000"),
            ]))
        .abs()
            < 1e-9,
        "{joined}: {text}"
    );
}

#[test]
fn a_rolled_back_item_before_an_item_that_is_not_comes_in_as_before() {
    // (A component inserted from another design comes after the root's last
    // item, which is not rolled back: no marker.)
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        rolled_back_extrude(2, 0, "JoinFeatureOperation", 30.0),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let text = report.text();
    assert_eq!(report.suppressed, SuppressedReport::default(), "{text}");
    assert_eq!(doc.marker(), doc.features().count(), "{text}");
}
