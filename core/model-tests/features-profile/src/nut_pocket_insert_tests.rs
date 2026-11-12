// SPDX-License-Identifier: MIT
//! Nut pockets and heat-set insert holes for printed parts (mitcad#1518):
//! the `hole_sizes` query's ISO 4032 nuts with their pockets and its insert
//! series, and holes that take their values from them, with the mock
//! kernel. The geometry is `test_nut_pockets` in
//! geometry/tests/test_hole.cpp.

use serde_json::{Value, json};

use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, FeatureUid};
use mitcad_model_api::DocumentApi;

const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn size(doc: &Document<MockKernel>, size: &str, extra: Value) -> Value {
    let mut asked = json!({"query": "hole_sizes", "size": size});
    for (key, field) in extra.as_object().unwrap() {
        asked[key] = field.clone();
    }
    let answer = query(doc, asked);
    assert_eq!(answer["sizes"].as_array().unwrap().len(), 1, "{answer}");
    answer["sizes"][0].clone()
}

/// The last hole the kernel was asked for.
fn hole_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|spec| spec.starts_with("hole "))
        .cloned()
        .unwrap_or_default()
}

/// A hole into the block's top at (30, 20) with `fields`.
fn hole(fields: Value) -> Value {
    let mut value = json!({"type": "hole",
        "placement": {"type": "face", "body": "F2.b0", "face": TOP, "points": [[30, 20, 20]]}});
    for (key, field) in fields.as_object().unwrap() {
        value[key] = field.clone();
    }
    value
}

fn add(b: &mut Block, value: Value) -> FeatureUid {
    let uid = b.doc.add_feature(&def(value), None).unwrap().uid;
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    uid
}

/// A number the hole's stored definition gives at a JSON pointer, through
/// its parameter where it names one.
fn stored(doc: &Document<MockKernel>, uid: FeatureUid, pointer: &str) -> f64 {
    let def = query(doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    let field = def
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{pointer} in {def}"));
    match field.as_str() {
        Some(name) => {
            let id = doc.parameters().find(name).unwrap();
            doc.parameters().value(id).unwrap()
        }
        None => field.as_f64().unwrap(),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn an_m3_nut_pocket_is_the_iso_4032_nut_with_the_clearance() {
    let mut b = block();
    // The table: ISO 4032's M3 nut is 5.5 across the flats and 2.4 high;
    // its pocket has 0.3 mm more of each.
    let m3 = size(&b.doc, "M3", json!({}));
    assert_eq!(m3["nut"], json!({"across_flats": 5.5, "height": 2.4}));
    assert_eq!(m3["nut_pocket"], json!({"across_flats": 5.8, "depth": 2.7}));
    let all = query(&b.doc, json!({"query": "hole_sizes"}));
    assert_eq!(all["nut_clearance"], json!(0.3));
    assert!(
        all["standards"]["nut"]
            .as_str()
            .unwrap()
            .contains("ISO 4032"),
        "{all}"
    );
    // Another clearance, and one refused.
    let loose = size(&b.doc, "M3", json!({"nut_clearance": 0.5}));
    assert_eq!(
        loose["nut_pocket"],
        json!({"across_flats": 6.0, "depth": 2.9})
    );
    let error = b
        .doc
        .query(&json!({"query": "hole_sizes", "nut_clearance": -0.1}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("must not be negative"), "{error}");

    // An M3 clearance hole through the block with the pocket at its start,
    // the values the table gives.
    let pocket = &m3["nut_pocket"];
    let uid = add(
        &mut b,
        hole(json!({"diameter": m3["clearance"]["normal"],
            "kind": {"type": "nut_pocket", "across_flats": pocket["across_flats"],
                     "depth": pocket["depth"]},
            "extent": {"type": "through_all"},
            "clearance": {"size": "M3"}})),
    );
    let spec = hole_spec(&b.doc);
    assert!(
        spec.contains(
            "diameter 3.400000 NutPocket { across_flats: 5.8, depth: 2.7, angle: 0.0, \
             offset: 0.0, slot: None }"
        ),
        "{spec}"
    );
    // Stored as the hole's values: ISO 4032's nut and the clearance.
    assert!(close(stored(&b.doc, uid, "/kind/across_flats"), 5.5 + 0.3));
    assert!(close(stored(&b.doc, uid, "/kind/depth"), 2.4 + 0.3));
    let faces: Vec<String> = b
        .doc
        .body_shape(b.body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .filter(|f| f.contains("pocket"))
        .collect();
    assert_eq!(faces.len(), 7, "{faces:?}");
    assert!(faces.contains(&format!("{uid}:hole0.pocket_floor")));
}

#[test]
fn a_side_slot_pocket_lies_below_the_start() {
    let mut b = block();
    // An M4 nut 4 mm below the top, slid in from the side 30 mm away.
    let m4 = size(&b.doc, "M4", json!({}));
    let pocket = &m4["nut_pocket"];
    assert_eq!(pocket, &json!({"across_flats": 7.3, "depth": 3.5}));
    let uid = add(
        &mut b,
        hole(json!({"diameter": 4.5,
            "kind": {"type": "nut_pocket", "across_flats": pocket["across_flats"],
                     "depth": pocket["depth"], "offset": 4, "slot": 30, "angle": "90 deg"},
            "extent": {"type": "distance", "depth": 15}})),
    );
    let spec = hole_spec(&b.doc);
    assert!(
        spec.contains("NutPocket { across_flats: 7.3, depth: 3.5, angle: ")
            && spec.contains("offset: 4.0, slot: Some(30.0) }"),
        "{spec}"
    );
    let saved = query(&b.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(saved["kind"]["type"], "nut_pocket");
    assert!(close(stored(&b.doc, uid, "/kind/slot"), 30.0));
    assert!((stored(&b.doc, uid, "/kind/angle") - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    let shape = b.doc.body_shape(b.body).unwrap();
    let has = |part: &str| {
        shape
            .faces
            .iter()
            .any(|f| f.to_string() == format!("{uid}:hole0.{part}"))
    };
    assert!(has("slot_end") && has("pocket_roof") && has("pocket0") && !has("pocket1"));

    // Refused: a pocket no wider than the hole, a slot that stays inside it,
    // a pocket as deep as the hole, a negative offset, a thread.
    for (kind, extra, expected) in [
        (
            json!({"type": "nut_pocket", "across_flats": 4.5, "depth": 3.5}),
            json!({}),
            "wider than the hole",
        ),
        (
            json!({"type": "nut_pocket", "across_flats": 7.3, "depth": 3.5, "slot": 4}),
            json!({}),
            "reach out of the pocket",
        ),
        (
            json!({"type": "nut_pocket", "across_flats": 7.3, "depth": 3.5, "offset": 12}),
            json!({}),
            "shallower than the hole",
        ),
        (
            json!({"type": "nut_pocket", "across_flats": 7.3, "depth": 3.5, "offset": -1}),
            json!({}),
            "must not be negative",
        ),
        (
            json!({"type": "nut_pocket", "across_flats": 7.3, "depth": 3.5}),
            json!({"thread": {"designation": "M4x0.7", "class": "6H"}}),
            "the nut has it",
        ),
    ] {
        let mut fields = json!({"diameter": 4.5, "kind": kind,
            "extent": {"type": "distance", "depth": 15}});
        for (key, field) in extra.as_object().unwrap() {
            fields[key] = field.clone();
        }
        let error = b
            .doc
            .add_feature(&def(hole(fields.clone())), None)
            .err()
            .unwrap_or_else(|| panic!("{fields} was accepted"))
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
}

#[test]
fn an_insert_hole_takes_its_sizes_from_the_series() {
    let mut b = block();
    // The series: the published hole diameter and the insert's length, the
    // hole 1 mm deeper; the source named.
    let m3 = size(&b.doc, "M3", json!({}));
    assert_eq!(
        m3["insert"],
        json!({"diameter": 4.0, "depth": 6.7, "length": 5.7})
    );
    assert!(size(&b.doc, "M8", json!({})).get("insert").is_none());
    let all = query(&b.doc, json!({"query": "hole_sizes"}));
    assert!(
        all["standards"]["insert"]
            .as_str()
            .unwrap()
            .contains("ruthex"),
        "{all}"
    );

    // The hole the panel makes from it: the series' values, its size named.
    let insert = &m3["insert"];
    let uid = add(
        &mut b,
        hole(json!({"diameter": insert["diameter"], "flat": true,
            "extent": {"type": "distance", "depth": insert["depth"]},
            "insert": {"size": "M3"}})),
    );
    let spec = hole_spec(&b.doc);
    assert!(
        spec.contains("diameter 4.000000 Simple tip None depth 6.7"),
        "{spec}"
    );
    let saved = query(&b.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(saved["insert"], json!({"size": "M3"}), "{saved}");
    assert!(close(stored(&b.doc, uid, "/diameter"), 4.0));
    assert!(close(stored(&b.doc, uid, "/extent/depth"), 6.7));

    // Refused: a size the series lacks, and an insert hole with a thread or
    // a counterbore.
    for (fields, expected) in [
        (
            json!({"insert": {"size": "M8"}}),
            "no heat-set insert for the screw size M8",
        ),
        (
            json!({"insert": {"size": "M3"}, "thread": {"designation": "M3x0.5", "class": "6H"}}),
            "has no thread",
        ),
        (
            json!({"insert": {"size": "M3"},
                   "kind": {"type": "counterbore", "diameter": 6, "depth": 1}}),
            "is a simple hole",
        ),
    ] {
        let mut value = hole(json!({"diameter": 4, "extent": {"type": "distance", "depth": 6.7}}));
        for (key, field) in fields.as_object().unwrap() {
            value[key] = field.clone();
        }
        let error = b
            .doc
            .add_feature(&def(value.clone()), None)
            .err()
            .unwrap_or_else(|| panic!("{value} was accepted"))
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
}
