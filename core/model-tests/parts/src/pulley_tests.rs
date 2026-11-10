// SPDX-License-Identifier: MIT
//! Pulleys (mitcad#562) in documents with the mock kernel: parameters as
//! expressions, rebuilds, saved designs and edits, checks, flanges, hubs,
//! bores and set screws, and belts round two pulleys.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel().specs.borrow().clone()
}

/// The sweeps a feature asked for.
fn of_feature(doc: &Document<MockKernel>, feature: &str) -> Vec<String> {
    let prefix = format!("gear_sweep {feature} ");
    specs(doc)
        .into_iter()
        .filter(|s| s.starts_with(&prefix))
        .collect()
}

fn bounds(doc: &Document<MockKernel>, body: &str) -> ([f64; 3], [f64; 3]) {
    let uid: BodyUid = body.parse().unwrap();
    let shape = doc.body_shape(uid).expect("the body");
    let b = shape.bounds.expect("bounds");
    (b.min, b.max)
}

fn has_face(doc: &Document<MockKernel>, body: &str, face: &str) -> bool {
    let uid: BodyUid = body.parse().unwrap();
    let shape = doc.body_shape(uid).expect("the body");
    shape.faces.iter().any(|f| f.to_string() == face)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0e-6
}

fn rejected(doc: &mut Document<MockKernel>, value: Value) -> String {
    match add(doc, value) {
        Err(error) => error.to_string(),
        Ok(uid) => match status(doc, uid) {
            FeatureStatus::Failed(message) => message.agent_text().to_owned(),
            other => panic!("expected a failure, got {other:?}"),
        },
    }
}

/// GT2 2 mm: the outside diameter of z teeth, z 2 / pi - 2 x 0.254.
fn gt2_outside(teeth: f64) -> f64 {
    teeth * 2.0 / PI - 0.508
}

#[test]
fn pulleys_take_expressions_rebuild_reopen_and_edit() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "z", "expression": "20", "unit": ""}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "belt", "expression": "6 mm"}),
    );
    let pulley = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "plane": "xy",
               "teeth": "z", "belt_width": "belt", "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, pulley), FeatureStatus::Ok);
    assert_eq!(doc.feature(pulley).unwrap().name, "Pulley1");
    // 20 grooves of 6 curves each (no flat bottoms), 6 + 1 mm wide.
    let last = specs(&doc).last().unwrap().clone();
    assert!(
        last.ends_with("curves 120 from 0 length 7 twist 0"),
        "{last}"
    );
    // Tooth 1 on the x axis, tooth 11 opposite: the outside diameter.
    let r = gt2_outside(20.0) / 2.0;
    let (min, max) = bounds(&doc, "F1.b0");
    assert!(near(max[0], r) && near(min[0], -r), "{min:?} {max:?}");
    assert!(near(max[2], 7.0));

    // More teeth rebuild the pulley.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "z", "expression": "30"}),
    );
    assert_eq!(status(&doc, pulley), FeatureStatus::Ok);
    let r = gt2_outside(30.0) / 2.0;
    let (min, max) = bounds(&doc, "F1.b0");
    assert!(near(max[0], r) && near(min[0], -r), "{min:?} {max:?}");

    // Saved and opened again: the same definition and pulley.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded, pulley), FeatureStatus::Ok);
    assert_eq!(
        loaded.feature(pulley).unwrap().def,
        doc.feature(pulley).unwrap().def
    );
    let (min, max) = bounds(&loaded, "F1.b0");
    assert!(near(max[0], r) && near(min[0], -r));
    assert!(near(max[2], 7.0));
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(file.to_string().contains("\"teeth\":\"z\""), "{saved}");
    assert!(
        file.to_string().contains("\"profile\":\"gt2_2mm\""),
        "{saved}"
    );

    // Edited: a 2 mm clearance and flanges on both sides.
    command(
        &mut loaded,
        json!({"cmd": "edit_feature", "uid": pulley, "def": {
            "type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "plane": "xy",
            "teeth": "z", "belt_width": "belt", "clearance": 2,
            "flanges": {"sides": "both", "diameter": 24, "thickness": 1.5},
            "operation": "new_body"}}),
    );
    assert_eq!(status(&loaded, pulley), FeatureStatus::Ok);
    // The mock's unite keeps the solids it joins apart (a new body of each),
    // so the edit shows in its sweeps: the 30 teeth (6 curves each) 8 wide
    // after the first flange, the flanges 24 across (one curve each) on
    // both sides.
    let sweeps = of_feature(&loaded, "F1");
    let edit = &sweeps[sweeps.len() - 3..];
    assert!(edit[0].contains("curves 180 from 1.5 length 8"), "{edit:?}");
    assert!(edit[1].contains("curves 1 from 0 length 1.5"), "{edit:?}");
    assert!(edit[2].contains("curves 1 from 9.5 length 1.5"), "{edit:?}");
    let (min, max) = bounds(&loaded, "F1.b1");
    assert!(near(max[0], 12.0) && near(min[1], -12.0), "{min:?} {max:?}");
    assert!(near(min[2], 0.0) && near(max[2], 1.5), "{min:?} {max:?}");
    // The edit saved and opened again.
    let edited = loaded.to_json();
    let mut again = Document::from_json(&edited, MockKernel::default()).unwrap();
    again.recompute();
    assert_eq!(status(&again, pulley), FeatureStatus::Ok);
    assert_eq!(
        again.feature(pulley).unwrap().def,
        loaded.feature(pulley).unwrap().def
    );
}

#[test]
fn every_profile_has_its_pitch_and_outside_diameters() {
    let doc = Document::new(MockKernel::default());
    // (profile, teeth, pitch, the pitch line offset PLD): the outside
    // diameter N p / pi - 2 PLD.
    for (profile, teeth, pitch, pld) in [
        ("gt2_2mm", 20.0, 2.0, 0.254),
        ("gt2_3mm", 24.0, 3.0, 0.381),
        ("htd_3m", 30.0, 3.0, 0.381),
        ("htd_5m", 30.0, 5.0, 0.5715),
        ("t5", 20.0, 5.0, 0.425),
        ("xl", 15.0, 5.08, 0.254),
    ] {
        let values = query(
            &doc,
            json!({"query": "pulley_values", "def": {"kind": "toothed", "profile": profile,
                   "plane": "xy", "teeth": teeth, "belt_width": 6, "operation": "new_body"}}),
        );
        assert!(values.get("error").is_none(), "{profile}: {values}");
        let pd = values["pitch_diameter"].as_f64().unwrap();
        let od = values["outside_diameter"].as_f64().unwrap();
        assert!(near(pd, teeth * pitch / PI), "{profile}: {values}");
        assert!(
            near(od, teeth * pitch / PI - 2.0 * pld),
            "{profile}: {values}"
        );
        assert_eq!(values["teeth"], json!(teeth), "{profile}");
        assert_eq!(values["pitch"], json!(pitch), "{profile}");
        assert!(near(values["tooth_width"].as_f64().unwrap(), 7.0));
        assert!(values["root_diameter"].as_f64().unwrap() < od);
        let area = values["section_area"].as_f64().unwrap();
        assert!(area < PI * od * od / 4.0, "{profile}: {values}");
    }
    // A pitch correction moves the outside diameter, not the pitch
    // diameter the belt wraps.
    let values = query(
        &doc,
        json!({"query": "pulley_values", "def": {"kind": "toothed", "profile": "gt2_2mm",
               "plane": "xy", "teeth": 20, "belt_width": 6, "pitch_correction": 0.1,
               "operation": "new_body"}}),
    );
    assert!(near(values["pitch_diameter"].as_f64().unwrap(), 40.0 / PI));
    assert!(near(
        values["outside_diameter"].as_f64().unwrap(),
        gt2_outside(20.0) + 0.1
    ));
}

#[test]
fn flanges_hubs_bores_and_set_screws() {
    let mut doc = Document::new(MockKernel::default());
    // GT2 20 teeth for a 6 mm belt: flanges 18 x 1 on both sides, a hub
    // 16 x 7, a 5 mm bore and two M3 set screws in the hub's middle.
    let pulley = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "plane": "xy",
               "teeth": 20, "belt_width": 6, "clearance": 1,
               "flanges": {"sides": "both", "diameter": 18, "thickness": 1},
               "hub": {"diameter": 16, "length": 7},
               "bore": {"type": "plain", "diameter": 5},
               "set_screws": {"count": 2, "thread": {"designation": "M3x0.5"}},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, pulley), FeatureStatus::Ok);
    let specs = of_feature(&doc, "F1");
    // The teeth, two flanges, the hub, the bore and two set screws.
    assert_eq!(specs.len(), 7, "{specs:?}");
    assert!(
        specs[0].contains("curves 120 from 1 length 7"),
        "{}",
        specs[0]
    );
    assert!(
        specs[1].contains("curves 1 from 0 length 1"),
        "{}",
        specs[1]
    );
    assert!(
        specs[2].contains("curves 1 from 8 length 1"),
        "{}",
        specs[2]
    );
    assert!(
        specs[3].contains("curves 1 from 9 length 7"),
        "{}",
        specs[3]
    );
    // Through everything, a millimetre beyond each end.
    assert!(
        specs[4].contains("curves 1 from -1 length 18"),
        "{}",
        specs[4]
    );
    // From the axis 3.5 mm from the hub's end (16 - 3.5 from the plane),
    // out past the hub's 8 mm radius.
    for spec in &specs[5..] {
        assert!(
            spec.contains("at [0.0, 0.0, 12.5] curves 1 from 0 length 9"),
            "{spec}"
        );
    }
    // The faces by their roles.
    let body = "F1.b0";
    for face in [
        "F1:tip(1)",
        "F1:flank_a(1)",
        "F1:tip_fillet_b(20)",
        "F1:root(20)",
        "F1:flange_a",
        "F1:flange_b",
        "F1:hub",
        "F1:bore",
        "F1:set_screw(1)",
        "F1:set_screw(2)",
    ] {
        assert!(has_face(&doc, body, face), "{face}");
    }
    // The mount frame on the axis, as a library pulley's.
    let frame = doc
        .last_result()
        .result(pulley)
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get("bore").copied())
        .expect("the bore frame");
    assert_eq!(frame.translation, [0.0, 0.0, 0.0]);
    assert_eq!(frame.linear[2][2], 1.0);
    // The set screws are cosmetic threads.
    let threads = query(&doc, json!({"query": "threads"}));
    let screws = threads
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["feature"] == json!("F1"))
        .count();
    assert_eq!(screws, 2, "{threads}");

    // HTD 5M 30 teeth with a 12 mm bore, its DIN 6885 keyway (4 x 1.8) and
    // a 30 x 10 hub, one flange on the plane's side.
    let keyed = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "htd_5m", "plane": "xy",
               "center": [100, 0], "teeth": 30, "belt_width": 15,
               "flanges": {"sides": "one", "diameter": 52, "thickness": 1.5},
               "hub": {"diameter": 30, "length": 10},
               "bore": {"type": "plain", "diameter": 12}, "keyway": true,
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, keyed), FeatureStatus::Ok);
    let mine = of_feature(&doc, "F2");
    assert_eq!(mine.len(), 4, "{mine:?}");
    // The teeth after the flange, the flange, the hub, the keyed bore.
    assert!(mine[0].contains("from 1.5 length 16"), "{}", mine[0]);
    assert!(
        mine[1].contains("curves 1 from 0 length 1.5"),
        "{}",
        mine[1]
    );
    assert!(
        mine[2].contains("curves 1 from 17.5 length 10"),
        "{}",
        mine[2]
    );
    assert!(
        mine[3].contains("curves 4 from -1 length 29.5"),
        "{}",
        mine[3]
    );
    assert!(has_face(&doc, "F2.b0", "F2:keyway_top"));

    // A threaded bore: an M8 tap drill, a cosmetic thread.
    let threaded = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "htd_5m", "plane": "xy",
               "center": [200, 0], "teeth": 30, "belt_width": 15,
               "bore": {"type": "thread", "thread": {"designation": "M8x1.25"}},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, threaded), FeatureStatus::Ok);

    // A smooth idler with flanges and a bearing bore, and a toothed idler.
    let idler = add(
        &mut doc,
        json!({"type": "pulley", "kind": "idler", "plane": "xy", "center": [0, 100],
               "diameter": 18, "belt_width": 6,
               "flanges": {"sides": "both", "diameter": 22, "thickness": 1},
               "bore": {"type": "plain", "diameter": 10}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, idler), FeatureStatus::Ok);
    assert!(has_face(&doc, "F4.b0", "F4:running"));
    let (min, max) = bounds(&doc, "F4.b0");
    assert!(near(max[0], 11.0) && near(min[0], -11.0), "{min:?} {max:?}");
    let toothed_idler = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed_idler", "profile": "gt2_2mm", "plane": "xy",
               "center": [100, 100], "teeth": 30, "belt_width": 6,
               "flanges": {"sides": "both", "diameter": 24, "thickness": 1},
               "bore": {"type": "plain", "diameter": 10}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, toothed_idler), FeatureStatus::Ok);
}

#[test]
fn pulleys_check_their_values() {
    let mut doc = Document::new(MockKernel::default());
    let gt2 = |extra: Value| {
        let mut value = json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm",
                               "plane": "xy", "teeth": 20, "belt_width": 6,
                               "operation": "new_body"});
        for (k, v) in extra.as_object().unwrap() {
            value[k] = v.clone();
        }
        value
    };
    let message = rejected(&mut doc, gt2(json!({"teeth": 5})));
    assert!(message.contains("needs 10 to 1000 teeth"), "{message}");
    let message = rejected(&mut doc, gt2(json!({"teeth": 20.5})));
    assert!(message.contains("must be whole"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "plane": "xy", "teeth": 20,
               "belt_width": 6, "operation": "new_body"}),
    );
    assert!(message.contains("needs a belt profile"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "pulley", "kind": "idler", "plane": "xy", "teeth": 20, "diameter": 12,
               "belt_width": 6, "operation": "new_body"}),
    );
    assert!(message.contains("an idler has no teeth"), "{message}");
    let message = rejected(
        &mut doc,
        gt2(json!({"flanges": {"sides": "both", "diameter": 12, "thickness": 1}})),
    );
    assert!(message.contains("flanges must be larger"), "{message}");
    let message = rejected(
        &mut doc,
        gt2(json!({"set_screws": {"count": 1, "thread": {"designation": "M3x0.5"}}})),
    );
    assert!(message.contains("set screws need a hub"), "{message}");
    let message = rejected(
        &mut doc,
        gt2(json!({"hub": {"diameter": 16, "length": 7},
                   "set_screws": {"count": 5, "thread": {"designation": "M3x0.5"}}})),
    );
    assert!(message.contains("1 to 4 set screws"), "{message}");
    let message = rejected(
        &mut doc,
        gt2(json!({"bore": {"type": "plain", "diameter": 10}})),
    );
    assert!(
        message.contains("the bore reaches the grooves"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        gt2(
            json!({"bore": {"type": "thread", "thread": {"designation": "M5x0.8"}},
                   "keyway": true}),
        ),
    );
    assert!(message.contains("a keyway needs a plain bore"), "{message}");
    // Flanges a little larger than the outside diameter are fine: 18 mm on
    // 20 teeth (12.22 mm), 24 mm on 30 teeth (18.59 mm).
    for (teeth, diameter, x) in [(20, 18, 0), (30, 24, 100)] {
        let uid = add(
            &mut doc,
            gt2(json!({"teeth": teeth, "center": [x, 300],
                       "flanges": {"sides": "both", "diameter": diameter, "thickness": 1}})),
        )
        .unwrap();
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    }
}

#[test]
fn belts_run_round_two_pulleys() {
    let mut doc = Document::new(MockKernel::default());
    let gt2 = json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm",
                     "plane": "xy", "teeth": 20, "belt_width": 6, "operation": "new_body"});
    let small = add(&mut doc, gt2).unwrap();
    // A 40-tooth pulley 80 mm along the y axis from the 20-tooth one.
    let large = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "teeth": 40,
               "belt_width": 6,
               "belt_with": {"pulley": small, "center_distance": 80, "angle": "90 deg"},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, large), FeatureStatus::Ok);
    // Teeth 1 and 21 on the x axis, 11 and 31 on the y axis: centred on
    // (0, 80).
    let (min, max) = bounds(&doc, "F2.b0");
    assert!(near((min[1] + max[1]) / 2.0, 80.0), "{min:?} {max:?}");
    assert!(near((min[0] + max[0]) / 2.0, 0.0), "{min:?} {max:?}");
    assert!(near(max[0], gt2_outside(40.0) / 2.0), "{min:?} {max:?}");
    let frame = doc
        .last_result()
        .result(large)
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get("bore").copied())
        .expect("the bore frame");
    assert!(near(frame.translation[1], 80.0), "{frame:?}");

    // The worked example: pitch radii R = 40 x 2 / (2 pi) = 12.7323954 and
    // r = 20 x 2 / (2 pi) = 6.3661977, C = 80. sin(t) = (R - r) / C =
    // 0.0795774715, t = 0.0796616997 rad, cos(t) = 0.9968286844.
    // L = 2 C cos(t) + pi (R + r) + 2 t (R - r)
    //   = 159.4925895 + 60 + 1.0142843 = 220.5068738 mm,
    // 110.2534 teeth of 2 mm. The belt wraps pi - 2 t = 2.9822693 rad of
    // the small pulley: 20 x 2.9822693 / (2 pi) = 9.49 teeth, 9 whole in
    // mesh. Of the catalogue lengths 200, 220 and 240 mm, 220 is nearest.
    let values = query(
        &doc,
        json!({"query": "pulley_values", "uid": large, "belt_lengths": [200, 220, 240]}),
    );
    assert!(values.get("error").is_none(), "{values}");
    let get = |key: &str| values[key].as_f64().unwrap_or(f64::NAN);
    assert!((get("center_distance") - 80.0).abs() < 1.0e-9, "{values}");
    assert!((get("belt_length") - 220.5069).abs() < 0.01, "{values}");
    assert!((get("belt_teeth") - 110.2534).abs() < 0.01, "{values}");
    assert_eq!(values["catalogue_length"], json!(220.0), "{values}");
    assert_eq!(values["teeth_in_mesh"], json!(9.0), "{values}");
    assert_eq!(values["mate"], json!(small));
    // Without catalogue lengths, no catalogue length.
    let values = query(&doc, json!({"query": "pulley_values", "uid": large}));
    assert!(values.get("catalogue_length").is_none(), "{values}");

    // Two 20-tooth pulleys 80 mm apart: 2 x 80 + 40 = 200 mm, 100 teeth,
    // half of each pulley in mesh.
    let values = query(
        &doc,
        json!({"query": "pulley_values", "def": {"kind": "toothed", "profile": "gt2_2mm",
               "teeth": 20, "belt_width": 6,
               "belt_with": {"pulley": small, "center_distance": 80, "angle": "180 deg"},
               "operation": "new_body"}}),
    );
    assert!((values["belt_length"].as_f64().unwrap() - 200.0).abs() < 1.0e-9);
    assert!((values["belt_teeth"].as_f64().unwrap() - 100.0).abs() < 1.0e-9);
    assert_eq!(values["teeth_in_mesh"], json!(10.0), "{values}");

    // Different profiles do not share a belt; overlapping pitch circles
    // make no belt; a pulley on a belt takes no plane and no angle.
    let message = rejected(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "htd_3m", "teeth": 20,
               "belt_width": 6, "belt_with": {"pulley": small, "center_distance": 80},
               "operation": "new_body"}),
    );
    assert!(message.contains("different belt profiles"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "teeth": 40,
               "belt_width": 6, "belt_with": {"pulley": small, "center_distance": 15},
               "operation": "new_body"}),
    );
    assert!(message.contains("pitch circles overlap"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "teeth": 40,
               "belt_width": 6, "angle": 0.5,
               "belt_with": {"pulley": small, "center_distance": 80},
               "operation": "new_body"}),
    );
    assert!(message.contains("takes no angle"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "teeth": 40,
               "belt_width": 6, "plane": "xy",
               "belt_with": {"pulley": small, "center_distance": 80},
               "operation": "new_body"}),
    );
    assert!(message.contains("takes its plane from it"), "{message}");

    // A smooth idler on the same belt: its running radius stands in for a
    // pitch radius; the teeth in mesh are the toothed pulley's.
    let idler = add(
        &mut doc,
        json!({"type": "pulley", "kind": "idler", "diameter": 20, "belt_width": 6,
               "belt_with": {"pulley": small, "center_distance": 50, "angle": "-90 deg"},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, idler), FeatureStatus::Ok);
    let values = query(&doc, json!({"query": "pulley_values", "uid": idler}));
    // R = 10, r = 6.3661977, C = 50: sin(t) = 0.0726760, t = 0.0727402,
    // L = 100 cos(t) + pi 16.3661977 + 2 t 3.6338023
    //   = 99.7355600 + 51.4159265 + 0.5286469 = 151.6801334; the belt
    // wraps pi - 2 t = 2.9961123 rad of the 20 teeth: 9.54, 9 in mesh.
    assert!(
        (values["belt_length"].as_f64().unwrap() - 151.6801).abs() < 0.01,
        "{values}"
    );
    assert_eq!(values["teeth_in_mesh"], json!(9.0), "{values}");
}
