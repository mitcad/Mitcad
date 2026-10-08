// SPDX-License-Identifier: MIT
//! The memory cache's budget and the caches' diagnostics (P7d), with the
//! mock kernel (a kilobyte of memory a face).

use std::time::Duration;

use serde_json::{Value, json};

use crate::Document;
use crate::document_tests::{block, extrude, num};
use crate::features::SketchPlane;
use crate::ids::FeatureUid;
use crate::store::ResultStore;
use crate::testing::MockKernel;

/// Block (F1, F2 with d3) and a boss joined to it (F3, F4).
fn joined() -> (Document<MockKernel>, FeatureUid, FeatureUid) {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [10.0, 10.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let join = extrude(sketch, &[&region], 30.0, "join", &[b.body]);
    let boss = b.doc.add_feature(&join, None).unwrap().uid;
    (b.doc, b.extrude, boss)
}

fn cache(doc: &Document<MockKernel>, disk: bool) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "cache", "disk": disk}).to_string())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn a_budget_drops_the_results_used_longest_ago() {
    let (mut doc, extrude, boss) = joined();
    for value in 21..25 {
        doc.set_parameter("d3", f64::from(value)).unwrap();
    }
    let unlimited = doc.memory_cache_bytes();
    assert!(unlimited > 0);
    // Too small for anything: what the document's results hold stays.
    doc.set_memory_budget(Some(1));
    let held = doc.memory_cache_bytes();
    assert!(held > 0 && held < unlimited, "{held} of {unlimited}");
    let report = cache(&doc, false);
    assert_eq!(report["memory"]["budget"], 1);
    assert!(report["memory"]["evicted"].as_u64().unwrap() >= 8);
    // Each feature keeps its current result only.
    for row in report["memory"]["by_feature"].as_array().unwrap() {
        assert_eq!(row["results"], 1, "{row}");
    }
    // An earlier value is evaluated again: its results went.
    doc.set_parameter("d3", 21.0).unwrap();
    assert_eq!(doc.stats().evaluated, vec![extrude, boss]);
    // Undo to a state whose results went computes too, and drops more.
    doc.undo();
    assert_eq!(doc.stats().evaluated, vec![extrude, boss]);
    // Without a budget nothing goes.
    doc.set_memory_budget(None);
    let before = doc.memory_cache_bytes();
    doc.set_parameter("d3", 30.0).unwrap();
    assert!(doc.memory_cache_bytes() > before);
}

#[test]
fn dropped_results_come_back_from_the_store() {
    let dir = std::env::temp_dir().join(format!("mitcad-cache-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (mut doc, extrude, boss) = joined();
    doc.set_result_store(Some(ResultStore::new(&dir, "test")));
    doc.persist_results(Duration::ZERO);
    doc.set_memory_budget(Some(1));
    doc.set_parameter("d3", 25.0).unwrap();
    // d3 = 20 again: its results left the memory, the store has them.
    doc.set_parameter("d3", 20.0).unwrap();
    assert_eq!(doc.stats().restored, vec![extrude, boss]);
    assert!(doc.stats().evaluated.is_empty());
    let report = cache(&doc, true);
    assert_eq!(report["memory"]["restored"], 2);
    let store = &report["store"];
    assert_eq!(store["build_id"], "test");
    assert_eq!(store["disk"]["files"], 2);
    let types = store["disk"]["by_type"].as_array().unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0]["type"], "extrude");
    assert_eq!(types[0]["features"], 2);
    let features = store["disk"]["by_feature"].as_array().unwrap();
    assert!(features[0]["bytes"].as_u64() >= features[1]["bytes"].as_u64());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_cache_query_tells_where_results_came_from() {
    let (mut doc, extrude, boss) = joined();
    doc.set_parameter("d3", 25.0).unwrap();
    let report = cache(&doc, false);
    let last = &report["last_recompute"];
    assert_eq!(last["evaluated"], 2);
    assert_eq!(last["cached"], 2);
    assert_eq!(last["restored"], 0);
    let sources: Vec<(&str, &str)> = last["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["name"].as_str().unwrap(), f["source"].as_str().unwrap()))
        .collect();
    assert_eq!(
        sources,
        [
            ("Sketch1", "cache"),
            ("Extrude1", "evaluated"),
            ("Sketch2", "cache"),
            ("Extrude2", "evaluated")
        ]
    );
    let memory = &report["memory"];
    assert_eq!(memory["bytes"], doc.memory_cache_bytes());
    assert_eq!(memory["budget"], Value::Null);
    assert_eq!(memory["features"], 4);
    // Two results each of the extrusions: the largest first, by type too.
    let features = memory["by_feature"].as_array().unwrap();
    assert_eq!(features[0]["results"], 2);
    let first = features[0]["uid"].as_str().unwrap();
    assert!(first == extrude.to_string() || first == boss.to_string());
    let types = memory["by_type"].as_array().unwrap();
    assert_eq!(types[0]["type"], "extrude");
    assert_eq!(types[0]["features"], 2);
    assert_eq!(types[0]["results"], 4);
    assert!(memory["hits"].as_u64().unwrap() >= 2);
    assert_eq!(report["store"], Value::Null);
    // clear_cache drops all but the current results, and is no change of
    // the document.
    // (The sketches have their results before their rectangles too.)
    let (revision, depth) = (doc.revision(), doc.undo_depth());
    let results = memory["results"].as_u64().unwrap();
    let answer: Value =
        serde_json::from_str(&doc.command(r#"{"cmd": "clear_cache"}"#).unwrap()).unwrap();
    assert_eq!(answer["results"], results - 4);
    assert_eq!(cache(&doc, false)["memory"]["results"], 4);
    assert_eq!((doc.revision(), doc.undo_depth()), (revision, depth));
}

#[test]
fn a_failure_for_lack_of_memory_is_not_kept() {
    // An operation that ran out of memory (mitcad#80) may build once the
    // memory is free again: the failure is not cached, so the same inputs
    // evaluate the feature again.
    let (mut doc, extrude, _) = joined();
    doc.kernel().out_of_memory.borrow_mut().insert("extrude");
    doc.set_parameter("d3", 21.0).unwrap();
    let error = doc
        .status(extrude)
        .and_then(|s| s.error())
        .map(str::to_owned);
    assert!(
        error
            .as_deref()
            .is_some_and(|e| e.contains("out of memory")),
        "{error:?}"
    );
    doc.kernel().out_of_memory.borrow_mut().clear();
    doc.set_parameter("d3", 22.0).unwrap();
    doc.set_parameter("d3", 21.0).unwrap();
    assert!(doc.stats().evaluated.contains(&extrude));
    assert_eq!(doc.status(extrude).and_then(|s| s.error()), None);
}
