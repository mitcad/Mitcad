// SPDX-License-Identifier: MIT
//! Clearance holes and the head recesses by screw size (mitcad#903): the
//! `hole_sizes` query's table (ISO 273, DIN 974-1 for ISO 4762 heads,
//! ISO 10642 and ISO 15065 countersinks) and holes that name their size,
//! with the mock kernel. The volumes of a clearance hole with a counterbore
//! and with a countersink on a block are in tools/cli/tests/f1_hole.json.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::test_support::{block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn size(doc: &Document<MockKernel>, size: &str) -> Value {
    let answer = query(doc, json!({"query": "hole_sizes", "size": size}));
    assert_eq!(answer["sizes"].as_array().unwrap().len(), 1, "{answer}");
    answer["sizes"][0].clone()
}

#[test]
fn the_table_gives_clearance_holes_and_head_recesses() {
    let b = block();
    let all = query(&b.doc, json!({"query": "hole_sizes"}));
    let names: Vec<&str> = all["sizes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["size"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "M1.6", "M2", "M2.5", "M3", "M4", "M5", "M6", "M8", "M10", "M12", "M14", "M16", "M20",
            "M24"
        ]
    );
    assert_eq!(
        all["fits"],
        json!([{"fit": "fine", "title": "Fine"}, {"fit": "normal", "title": "Normal"},
               {"fit": "coarse", "title": "Coarse"}])
    );
    assert_eq!(all["default_fit"], "normal");
    assert_eq!(all["default_size"], "M6");
    assert!(
        all["standards"]["clearance"]
            .as_str()
            .unwrap()
            .contains("ISO 273"),
        "{all}"
    );
    assert!(
        all["standards"]["counterbore"]
            .as_str()
            .unwrap()
            .contains("DIN 974-1"),
        "{all}"
    );
    assert!(all.get("tapped_countersink").is_none());

    // M2: ISO 273 2.2 / 2.4 / 2.6; the counterbore for its ISO 4762 head
    // 4.4, 2.2 deep; ISO 10642 has no M2, so the countersink is ISO 15065's.
    let m2 = size(&b.doc, "M2");
    assert_eq!(
        m2["clearance"],
        json!({"fine": 2.2, "normal": 2.4, "coarse": 2.6})
    );
    assert_eq!(m2["counterbore"], json!({"diameter": 4.4, "depth": 2.2}));
    assert_eq!(
        m2["counterdrill"],
        json!({"diameter": 4.4, "depth": 2.2, "angle": 90.0})
    );
    assert_eq!(m2["countersink"], json!({"diameter": 4.4, "angle": 90.0}));
    assert!(m2.get("iso_10642_head").is_none());

    // M6: 6.4 / 6.6 / 7.0; the counterbore 11, 6.4 deep; the countersink
    // the larger of ISO 10642's head (13.44) and ISO 15065's (12.6).
    let m6 = size(&b.doc, "m6");
    assert_eq!(m6["size"], "M6");
    assert_eq!(
        m6["clearance"],
        json!({"fine": 6.4, "normal": 6.6, "coarse": 7.0})
    );
    assert_eq!(m6["counterbore"], json!({"diameter": 11.0, "depth": 6.4}));
    assert_eq!(m6["countersink"], json!({"diameter": 13.44, "angle": 90.0}));
    assert_eq!(m6["iso_10642_head"], json!(13.44));
    assert_eq!(m6["iso_15065"], json!(12.6));

    // M24: 25 / 26 / 28; the counterbore 40, 24.8 deep; no countersink
    // (neither ISO 10642 nor ISO 15065 has M24).
    let m24 = size(&b.doc, "M24");
    assert_eq!(
        m24["clearance"],
        json!({"fine": 25.0, "normal": 26.0, "coarse": 28.0})
    );
    assert_eq!(m24["counterbore"], json!({"diameter": 40.0, "depth": 24.8}));
    assert!(m24.get("countersink").is_none(), "{m24}");

    // A tapped hole's countersink: just over the thread's major diameter.
    let tapped = query(
        &b.doc,
        json!({"query": "hole_sizes", "size": "M6", "major": 6}),
    );
    assert_eq!(
        tapped["tapped_countersink"],
        json!({"diameter": 6.6, "angle": 90.0})
    );

    // A tapped hole's thread (mitcad#980): its countersink from the size's
    // major diameter, the screw size of its nominal diameter (M6: DIN
    // 974-1's counterbore 11, 6.4 deep) and its bore D1. ISO 261's 5.5 has
    // no screw size, nor has another standard's thread.
    let m6x1 = query(
        &b.doc,
        json!({"query": "hole_sizes", "size": "M6",
               "thread": {"standard": "iso_metric", "size": "6", "designation": "M6x1"}}),
    );
    assert_eq!(
        m6x1["tapped_countersink"],
        json!({"diameter": 6.6, "angle": 90.0})
    );
    assert_eq!(m6x1["thread_size"]["size"], "M6");
    assert_eq!(
        m6x1["thread_size"]["counterbore"],
        json!({"diameter": 11.0, "depth": 6.4})
    );
    let bore = m6x1["thread_bore"].as_f64().unwrap();
    assert!((bore - 4.917468).abs() < 1e-4, "{m6x1}");
    for (standard, size) in [("iso_metric", "5.5"), ("unified", "1/4")] {
        let other = query(
            &b.doc,
            json!({"query": "hole_sizes", "size": "M6",
                   "thread": {"standard": standard, "size": size}}),
        );
        assert!(other.get("thread_size").is_none(), "{other}");
        assert!(other.get("tapped_countersink").is_some(), "{other}");
    }

    let error = b
        .doc
        .query(&json!({"query": "hole_sizes", "size": "M7"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no clearance hole for the screw size M7"),
        "{error}"
    );
    let error = b
        .doc
        .query(&json!({"query": "hole_sizes", "major": 0}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("greater than zero"), "{error}");
}

#[test]
fn a_clearance_hole_keeps_its_size_and_its_plain_values() {
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[30, 20, 20]]});
    // M6 Normal with the counterbore for its head: the values the panel
    // fills in, stored as plain values, the size and fit beside them.
    let hole = json!({"type": "hole", "placement": on_top, "diameter": 6.6,
        "kind": {"type": "counterbore", "diameter": 11, "depth": 6.4},
        "extent": {"type": "through_all"},
        "clearance": {"size": "M6", "fit": "normal"}});
    let uid = b.doc.add_feature(&def(hole.clone()), None).unwrap().uid;
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    let spec = b.doc.kernel().specs.borrow().last().cloned().unwrap();
    assert!(
        spec.contains("diameter 6.600000 Counterbore { diameter: 11.0, depth: 6.4 }"),
        "{spec}"
    );
    let stored = query(&b.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(
        stored["clearance"],
        json!({"size": "M6", "fit": "normal"}),
        "{stored}"
    );
    // A value typed instead of the table's is the hole's (12 for 11).
    let mut typed = hole.clone();
    typed["kind"]["diameter"] = json!(12);
    b.doc
        .edit_feature(uid, &def(typed))
        .expect("a typed counterbore");
    let spec = b.doc.kernel().specs.borrow().last().cloned().unwrap();
    assert!(
        spec.contains("Counterbore { diameter: 12.0, depth: 6.4 }"),
        "{spec}"
    );
    b.doc.undo();
    b.doc.undo();

    // Refused: a size the table lacks, and a clearance hole with a thread.
    for (clearance, thread, expected) in [
        (
            json!({"size": "M7"}),
            Value::Null,
            "no clearance hole for the screw size M7",
        ),
        (
            json!({"size": "M6"}),
            json!({"designation": "M6x1"}),
            "a clearance hole has no thread",
        ),
    ] {
        let mut wrong = hole.clone();
        wrong["clearance"] = clearance;
        if !thread.is_null() {
            wrong["thread"] = thread;
        }
        let error = b
            .doc
            .add_feature(&def(wrong), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
}
