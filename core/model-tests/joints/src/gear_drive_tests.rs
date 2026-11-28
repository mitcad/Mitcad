// SPDX-License-Identifier: MIT
//! Gear Drive (mitcad#2608) with the mock kernel: each type's components,
//! joints and links in one undo step; a planetary set driven from its sun
//! keeping every mesh engaged (the carrier and the planets at the ratios
//! the teeth give), with the ring or the carrier fixed; a compound train,
//! a rack and pinion and a bevel pair driven from their input; the drive
//! read back from a member and made again from a changed definition;
//! refusals in words leaving nothing.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::joints::Motion;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0e-6
}

/// A joint the drive made, by its name.
fn joint(added: &Value, name: &str) -> FeatureUid {
    added["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["name"] == name)
        .and_then(|j| j["uid"].as_str())
        .unwrap_or_else(|| panic!("no joint {name}: {added}"))
        .parse()
        .unwrap()
}

fn names(added: &Value, list: &str) -> Vec<String> {
    added[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_owned())
        .collect()
}

fn component(added: &Value) -> ComponentUid {
    added["component"].as_str().unwrap().parse().unwrap()
}

/// 18 sun teeth, three planets of 30, the ring's 78: module 1.
fn planetary(fixed: &str, planets: u32) -> Value {
    json!({"type": "planetary", "module": 1, "face_width": 8, "sun_teeth": 18,
           "planet_teeth": 30, "planets": planets, "fixed": fixed, "planet_bore": 6})
}

#[test]
fn a_planetary_set_turns_from_its_sun_with_every_mesh_engaged() {
    let mut doc = Document::new(MockKernel::default());
    let values = query(
        &doc,
        json!({"query": "gear_drive_values", "drive": planetary("ring", 3)}),
    );
    assert_eq!(values["ring_teeth"], 78.0, "{values}");
    let added = command(
        &mut doc,
        json!({"cmd": "add_gear_drive", "drive": planetary("ring", 3)}),
    );
    assert_eq!(doc.undo_label(), Some("Add Gear Drive"));
    assert_eq!(added["name"], "Planetary Drive");
    assert_eq!(
        names(&added, "members"),
        ["Sun", "Planet 1", "Planet 2", "Planet 3", "Carrier", "Ring"]
    );
    assert_eq!(
        names(&added, "joints"),
        [
            "Sun Joint",
            "Carrier Joint",
            "Ring Joint",
            "Planet 1 Joint",
            "Planet 2 Joint",
            "Planet 3 Joint"
        ]
    );
    assert_eq!(
        names(&added, "links"),
        [
            "Sun Link",
            "Planet 1 Mesh",
            "Planet 2 Mesh",
            "Planet 3 Mesh"
        ]
    );
    // Every member is a component of its own placed in the drive's.
    let frame = component(&added);
    assert_eq!(doc.assembly().children(frame).count(), 6);
    let joints = query(&doc, json!({"query": "joints"}));
    for link in joints["links"].as_array().unwrap() {
        assert_eq!(link["solved"], true, "{link}");
    }
    for j in joints["joints"].as_array().unwrap() {
        assert_eq!(j["status"], "ok", "{j}");
    }

    // Driven from the sun: the carrier and the planets follow, every mesh
    // keeping the relation it was made in (all at 0 then).
    let sun = joint(&added, "Sun Joint");
    assert_eq!(added["input"], sun.to_string());
    command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": sun.to_string(), "values": {"rz": 0.6}}),
    );
    let (zs, zp, zr) = (18.0, 30.0, 78.0);
    let ts = doc.joint_value(sun, Motion::Rz);
    let tc = doc.joint_value(joint(&added, "Carrier Joint"), Motion::Rz);
    assert!(near(ts, 0.6), "{ts}");
    assert!(near(tc, 0.6 / (1.0 + zr / zs)), "{tc}");
    for k in 1..=3 {
        let phi = doc.joint_value(joint(&added, &format!("Planet {k} Joint")), Motion::Rz);
        // As the carrier sees them: the sun's mesh and the fixed ring's.
        assert!(near(zs * (ts - tc) + zp * phi, 0.0), "planet {k}: {phi}");
        assert!(near(zr * (0.0 - tc) - zp * phi, 0.0), "planet {k}: {phi}");
    }

    // One undo step takes the whole drive back.
    let components = doc.assembly().components.len();
    doc.undo();
    doc.undo();
    assert!(doc.assembly().components.len() < components);
    assert!(!doc.assembly().exists(frame));
}

#[test]
fn a_planetary_set_with_its_carrier_fixed_turns_its_ring_back() {
    let mut doc = Document::new(MockKernel::default());
    let added = command(
        &mut doc,
        json!({"cmd": "add_gear_drive", "drive": planetary("carrier", 3)}),
    );
    let sun = joint(&added, "Sun Joint");
    assert_eq!(added["input"], sun.to_string());
    command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": sun.to_string(), "values": {"rz": 0.6}}),
    );
    let ring = doc.joint_value(joint(&added, "Ring Joint"), Motion::Rz);
    assert!(near(ring, -0.6 * 18.0 / 78.0), "{ring}");
    for k in 1..=3 {
        let phi = doc.joint_value(joint(&added, &format!("Planet {k} Joint")), Motion::Rz);
        assert!(near(phi, -0.6 * 18.0 / 30.0), "planet {k}: {phi}");
    }
}

#[test]
fn trains_racks_and_bevel_pairs_turn_from_their_input() {
    let mut doc = Document::new(MockKernel::default());
    // A compound train: 40 / 20 and 45 / 15, six turns in for one out.
    let train = command(
        &mut doc,
        json!({"cmd": "add_gear_drive", "drive": {"type": "gear_train", "module": 2,
               "face_width": 10, "teeth": [20, 40, 15, 45], "compound": true}}),
    );
    assert_eq!(train["values"]["ratio"], 6.0, "{train}");
    assert_eq!(
        names(&train, "joints"),
        [
            "Shaft 1 Joint",
            "Shaft 2 Joint",
            "Gear 3 on Shaft 2",
            "Shaft 3 Joint"
        ]
    );
    let input = joint(&train, "Shaft 1 Joint");
    command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": input.to_string(), "values": {"rz": 0.9}}),
    );
    let second = doc.joint_value(joint(&train, "Shaft 2 Joint"), Motion::Rz);
    let third = doc.joint_value(joint(&train, "Shaft 3 Joint"), Motion::Rz);
    assert!(near(second, -0.45), "{second}");
    assert!(near(third, 0.15), "{third}");

    // A rack and pinion: the pinion's pitch radius 20 per radian.
    let rack = command(
        &mut doc,
        json!({"cmd": "add_gear_drive", "drive": {"type": "rack_pinion", "module": 2,
               "face_width": 10, "pinion_teeth": 20, "rack_teeth": 30}}),
    );
    assert_eq!(names(&rack, "members"), ["Rack", "Pinion"]);
    let pinion = joint(&rack, "Pinion Joint");
    command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": pinion.to_string(), "values": {"rz": 0.5}}),
    );
    let slide = doc.joint_value(joint(&rack, "Rack Joint"), Motion::Tx);
    assert!(near(slide, 10.0), "{slide}");

    // A bevel pair of 20 and 30 teeth at right angles.
    let bevel = command(
        &mut doc,
        json!({"cmd": "add_gear_drive", "drive": {"type": "bevel_pair", "module": 2,
               "face_width": 8, "teeth": [20, 30]}}),
    );
    assert_eq!(names(&bevel, "members"), ["Bevel Gear 1", "Bevel Gear 2"]);
    assert_eq!(doc.assembly().children(component(&bevel)).count(), 2);
    let first = joint(&bevel, "Bevel Gear 1 Joint");
    command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": first.to_string(), "values": {"rz": 0.6}}),
    );
    let mate = doc.joint_value(joint(&bevel, "Bevel Gear 2 Joint"), Motion::Rz);
    assert!(near(mate, -0.4), "{mate}");
}

#[test]
fn a_drive_is_read_back_and_made_again_in_one_undo_step() {
    let mut doc = Document::new(MockKernel::default());
    let added = command(
        &mut doc,
        json!({"cmd": "add_gear_drive", "drive": planetary("ring", 3), "name": "Hub Drive"}),
    );
    let frame = component(&added);
    // From one of its members.
    let found = query(
        &doc,
        json!({"query": "gear_drive", "component": "Planet 2"}),
    );
    assert_eq!(found["component"], frame.to_string());
    assert_eq!(found["name"], "Hub Drive");
    assert_eq!(found["drive"]["planets"], 3.0);
    assert_eq!(found["values"]["ring_teeth"], 78.0);

    let depth = doc.undo_depth();
    let edited = command(
        &mut doc,
        json!({"cmd": "edit_gear_drive", "component": "Hub Drive",
               "drive": planetary("ring", 4)}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo_label(), Some("Edit Gear Drive"));
    assert_eq!(edited["occurrence"], added["occurrence"]);
    assert_eq!(edited["members"].as_array().unwrap().len(), 7);
    assert_eq!(doc.assembly().children(frame).count(), 7);
    // The names are free again: the members keep theirs.
    assert_eq!(names(&edited, "members")[0], "Sun");
    let found = query(
        &doc,
        json!({"query": "gear_drive", "component": "Hub Drive"}),
    );
    assert_eq!(found["drive"]["planets"], 4.0);
    doc.undo();
    assert_eq!(doc.assembly().children(frame).count(), 6);

    // A component in no drive.
    let error = doc
        .query(&json!({"query": "gear_drive", "component": "Root"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a gear drive"), "{error}");
}

#[test]
fn a_set_that_cannot_be_assembled_is_refused_and_leaves_nothing() {
    let mut doc = Document::new(MockKernel::default());
    let components = doc.assembly().components.len();
    let depth = doc.undo_depth();
    let refused = |doc: &mut Document<MockKernel>, drive: Value| {
        doc.command(&json!({"cmd": "add_gear_drive", "drive": drive}).to_string())
            .unwrap_err()
            .to_string()
    };
    // 18 + 78 teeth do not divide by five planets.
    let error = refused(&mut doc, planetary("ring", 5));
    assert!(error.contains("spaced evenly"), "{error}");
    // Six planets of 30 teeth round 18 overlap.
    let error = refused(&mut doc, planetary("ring", 6));
    assert!(error.contains("touch each other"), "{error}");
    let mut wide = planetary("ring", 3);
    wide["sun_bore"] = json!(16);
    let error = refused(&mut doc, wide);
    assert!(error.contains("reaches its tooth roots"), "{error}");
    let mut other = planetary("ring", 3);
    other["teeth"] = json!([20, 30]);
    let error = refused(&mut doc, other);
    assert!(error.contains("takes no teeth"), "{error}");
    assert_eq!(doc.assembly().components.len(), components);
    assert_eq!(doc.undo_depth(), depth);
}
