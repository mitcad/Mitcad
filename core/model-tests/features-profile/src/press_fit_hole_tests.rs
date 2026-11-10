// SPDX-License-Identifier: MIT
//! Press-fit holes for printed parts (mitcad#1517): hex and square holes
//! whose flats, and crush ribs whose tips, lie the press fit's interference
//! under the hole's diameter, the pin's or bearing's.

use serde_json::{Value, json};

use mitcad_model::features::FeatureDef;
use mitcad_model::test_support::{block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A hole of `diameter` into the block's top at (30, 20), 10 deep, with
/// `extra` fields.
fn press_fit_hole(diameter: f64, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "hole",
        "placement": {"type": "face", "body": "F2.b0", "face": TOP, "points": [[30, 20, 20]]},
        "diameter": diameter, "extent": {"type": "distance", "depth": 10}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// A number the mock kernel's spec writes after `label`.
fn spec_number(spec: &str, label: &str) -> f64 {
    let rest = spec
        .split_once(label)
        .unwrap_or_else(|| panic!("no '{label}' in '{spec}'"))
        .1;
    rest.split_whitespace()
        .next()
        .and_then(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("no number after '{label}' in '{spec}'"))
}

#[test]
fn hex_and_square_flats_lie_the_interference_under_the_pin() {
    let mut b = block();
    // A 5 mm pin in a hex hole at the printed fit (0.1 mm, created with
    // the hole): the flats' inscribed diameter is 4.9 mm.
    let hex = b
        .doc
        .add_feature(
            &press_fit_hole(
                5.0,
                json!({"press_fit": {"shape": "hex", "interference": "press_fit_interference"}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(b.doc.status(hex), Some(&FeatureStatus::Ok));
    let spec = last_spec(&b.doc);
    assert!(spec.contains("press fit 6 sides"), "{spec}");
    assert!(
        (spec_number(&spec, "inscribed") - (5.0 - 0.1)).abs() < 1e-6,
        "{spec}"
    );
    // A flat bottom: no drill point.
    assert!(spec.contains("tip None"), "{spec}");
    let interference = b.doc.parameters().find("press_fit_interference").unwrap();
    let value = b.doc.parameters().value(interference).unwrap();
    assert!((value - 0.1).abs() < 1e-9, "{value}");
    // The hex's faces: six flats and the bottom.
    let shape = b.doc.body_shape(b.body).unwrap();
    for k in 0..6 {
        let flat = format!("F3:hole0.flat{k}");
        assert!(
            shape.faces.iter().any(|f| f.to_string() == flat),
            "{flat} in {:?}",
            shape.faces
        );
    }
    // The fit follows its parameter.
    b.doc.set_parameter("press_fit_interference", 0.15).unwrap();
    let spec = last_spec(&b.doc);
    assert!(
        (spec_number(&spec, "inscribed") - (5.0 - 0.15)).abs() < 1e-6,
        "{spec}"
    );
    // The definition keeps the press fit.
    let stored = query(&b.doc, json!({"query": "feature", "uid": hex}))["def"].clone();
    assert_eq!(stored["press_fit"]["shape"], "hex");
    assert_eq!(
        stored["press_fit"]["interference"],
        "press_fit_interference"
    );

    // A square turned 45 degrees, a plain interference.
    let mut b = block();
    let square = b
        .doc
        .add_feature(
            &press_fit_hole(
                3.0,
                json!({"press_fit": {"shape": "square", "interference": 0.2, "angle": "45 deg"}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(b.doc.status(square), Some(&FeatureStatus::Ok));
    let spec = last_spec(&b.doc);
    assert!(spec.contains("press fit 4 sides"), "{spec}");
    assert!(
        (spec_number(&spec, "inscribed") - (3.0 - 0.2)).abs() < 1e-6,
        "{spec}"
    );
    assert!(
        (spec_number(&spec, "angle") - std::f64::consts::FRAC_PI_4).abs() < 1e-9,
        "{spec}"
    );
}

#[test]
fn crush_rib_tips_lie_the_interference_under_the_pin() {
    let mut b = block();
    // An 8 mm bearing in four ribs 0.4 mm high, 0.2 mm of interference:
    // the tips at 7.8 mm, the bore 8.6 mm.
    let ribs = b
        .doc
        .add_feature(
            &press_fit_hole(
                8.0,
                json!({"extent": {"type": "through_all"},
                       "press_fit": {"shape": "crush_ribs", "interference": 0.2,
                                     "ribs": 4, "rib_height": 0.4}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(b.doc.status(ribs), Some(&FeatureStatus::Ok));
    let spec = last_spec(&b.doc);
    assert!(spec.contains("press fit 4 ribs"), "{spec}");
    assert!(
        (spec_number(&spec, "tips") - (8.0 - 0.2)).abs() < 1e-6,
        "{spec}"
    );
    assert!(
        (spec_number(&spec, "bore") - (8.0 - 0.2 + 2.0 * 0.4)).abs() < 1e-6,
        "{spec}"
    );
    assert!(spec.contains("through 1 bodies"), "{spec}");
    let shape = b.doc.body_shape(b.body).unwrap();
    for part in ["wall3", "rib0.side0", "rib3.side1"] {
        let face = format!("F3:hole0.{part}");
        assert!(
            shape.faces.iter().any(|f| f.to_string() == face),
            "{face} in {:?}",
            shape.faces
        );
    }

    // Left out: three ribs 0.3 mm high.
    let mut b = block();
    b.doc
        .add_feature(
            &press_fit_hole(
                8.0,
                json!({"press_fit": {"shape": "crush_ribs", "interference": "press_fit_interference"}}),
            ),
            None,
        )
        .unwrap();
    let spec = last_spec(&b.doc);
    assert!(spec.contains("press fit 3 ribs"), "{spec}");
    assert!((spec_number(&spec, "tips") - 7.9).abs() < 1e-6, "{spec}");
    assert!((spec_number(&spec, "bore") - 8.5).abs() < 1e-6, "{spec}");
}

#[test]
fn press_fit_values_are_checked() {
    let mut b = block();
    for (extra, expected) in [
        (
            json!({"press_fit": {"shape": "hex", "interference": 5}}),
            "less than the hole's diameter",
        ),
        (
            json!({"press_fit": {"shape": "hex", "interference": 0.1, "ribs": 4}}),
            "only crush ribs have",
        ),
        (
            json!({"press_fit": {"shape": "crush_ribs", "interference": 0.1, "ribs": 2}}),
            "whole number from 3",
        ),
        (
            json!({"press_fit": {"shape": "crush_ribs", "interference": 0.4, "rib_height": 0.1}}),
            "more than half the interference",
        ),
        (
            json!({"press_fit": {"shape": "hex", "interference": 0.1},
                   "kind": {"type": "counterbore", "diameter": 10, "depth": 2}}),
            "a press-fit hole is a simple hole",
        ),
        (
            json!({"press_fit": {"shape": "hex", "interference": 0.1},
                   "thread": {"designation": "M6x1"}}),
            "a press-fit hole has no thread",
        ),
        (
            json!({"press_fit": {"shape": "hex", "interference": 0.1},
                   "clearance": {"size": "M6"}}),
            "a press-fit hole has no thread or clearance",
        ),
        (
            json!({"press_fit": {"shape": "square", "interference": 0.1}, "taper": 0.1}),
            "a press-fit hole has no taper",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&press_fit_hole(5.0, extra), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
}
