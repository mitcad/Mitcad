// SPDX-License-Identifier: MIT
//! Print shapes of holes printed lying (mitcad#1515): a hole whose axis
//! lies parallel to the print bed gets a teardrop's point or a flat roof
//! pointing away from the bed (the body's print bed face, else the
//! design's −Z down); a hole across the bed stays round; the hole keeps its
//! nominal diameter, and setting the bed face evaluates it again. The mock
//! kernel's spec writes each hole's roof direction (`round` for none).

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::FeatureDef;
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, FeatureUid};
use mitcad_model_api::DocumentApi;

/// The block's top (z = 20, normal +z) and its side on y = 0 (normal −y).
const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
const SIDE: &str = "F2:side(c1[c4,c2])";

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) {
    doc.command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
}

/// A 6 mm hole 10 deep with `extra` fields: lying, into the block's side
/// at (30, 0, 10) along +y, or upright, into its top at (30, 20) along −z.
fn hole(lying: bool, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let placement = if lying {
        json!({"type": "plane", "plane": {"origin": [0, 0, 0], "normal": [0, -1, 0]},
            "points": [[30, 0, 10]]})
    } else {
        json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[30, 20, 20]]})
    };
    let mut value = json!({"type": "hole", "placement": placement, "diameter": 6,
        "extent": {"type": "distance", "depth": 10}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

fn add(b: &mut Block, def: &FeatureDef<mitcad_model::ValueInput>) -> FeatureUid {
    let uid = b.doc.add_feature(def, None).unwrap().uid;
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    uid
}

/// A parameter's value by the name the hole's definition gives it.
fn value_of(doc: &Document<MockKernel>, uid: FeatureUid, field: &str) -> f64 {
    let stored = query(doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    let name = stored[field].as_str().unwrap().to_owned();
    let id = doc.parameters().find(&name).unwrap();
    doc.parameters().value(id).unwrap()
}

/// The block's faces made by `uid` with `part` in their names.
fn faces_with(b: &Block, uid: FeatureUid, part: &str) -> Vec<String> {
    let prefix = format!("{uid}:hole0.");
    let mut faces: Vec<String> = b
        .doc
        .body_shape(b.body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .filter(|f| f.starts_with(&prefix) && f.contains(part))
        .collect();
    faces.sort();
    faces
}

/// The block's side on y = 0 a plane facing −y, as the print bed takes it.
fn side_plane(doc: &Document<MockKernel>) {
    doc.kernel().surfaces.borrow_mut().insert(
        SIDE.to_owned(),
        SurfaceGeometry::Plane {
            origin: [30.0, 0.0, 10.0],
            normal: [0.0, -1.0, 0.0],
        },
    );
}

#[test]
fn a_hole_lying_on_the_bed_gets_a_teardrop_pointing_up_and_keeps_its_diameter() {
    // No print bed face: the design's −Z is down.
    let mut b = block();
    let lying = add(&mut b, &hole(true, json!({"print_shape": "teardrop"})));
    let spec = hole_spec(&b.doc);
    assert!(spec.contains("print roofs [(0, 0, 1)] teardrop"), "{spec}");
    // Only the solid is shaped: the kernel drills the nominal diameter.
    assert!(spec.contains("diameter 6.000000"), "{spec}");
    assert_eq!(
        faces_with(&b, lying, "roof"),
        [
            format!("{lying}:hole0.roof0"),
            format!("{lying}:hole0.roof1")
        ]
    );
    // The hole's record keeps its nominal diameter and its print shape.
    assert!((value_of(&b.doc, lying, "diameter") - 6.0).abs() < 1e-9);
    let stored = query(&b.doc, json!({"query": "feature", "uid": lying}))["def"].clone();
    assert_eq!(stored["print_shape"], "teardrop");
    assert!(b.doc.infos(lying).is_empty(), "{:?}", b.doc.infos(lying));

    // An upright hole stays round, with an information.
    let upright = add(&mut b, &hole(false, json!({"print_shape": "teardrop"})));
    let spec = hole_spec(&b.doc);
    assert!(!spec.contains("print roofs"), "{spec}");
    assert!(faces_with(&b, upright, "roof").is_empty());
    assert!(
        b.doc
            .infos(upright)
            .iter()
            .any(|i| i.contains("no hole lies parallel to the print bed")),
        "{:?}",
        b.doc.infos(upright)
    );
    assert!((value_of(&b.doc, upright, "diameter") - 6.0).abs() < 1e-9);

    // A round hole (the default) writes no print shape.
    add(&mut b, &hole(true, json!({})));
    assert!(!hole_spec(&b.doc).contains("print roofs"));
}

#[test]
fn the_roof_points_away_from_the_print_bed_face_and_follows_it() {
    let mut b = block();
    side_plane(&b.doc);
    // Upright to the design's Z but lying on the side bed once it is set.
    let upright = add(&mut b, &hole(false, json!({"print_shape": "teardrop"})));
    assert!(!hole_spec(&b.doc).contains("print roofs"));
    // Setting the bed face evaluates the hole again: its roof points away
    // from the side, +y.
    command(
        &mut b.doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": SIDE}),
    );
    let spec = hole_spec(&b.doc);
    assert!(spec.contains("print roofs [(0, 1, 0)] teardrop"), "{spec}");
    assert_eq!(b.doc.status(upright), Some(&FeatureStatus::Ok));
    assert_eq!(faces_with(&b, upright, "roof").len(), 2);
    // Cleared, it is round again; undo brings the roof back (the results
    // come from the recompute's cache, so the body tells).
    command(
        &mut b.doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": null}),
    );
    assert!(faces_with(&b, upright, "roof").is_empty());
    command(&mut b.doc, json!({"cmd": "undo"}));
    assert_eq!(faces_with(&b, upright, "roof").len(), 2);

    // A hole into the bed face, its axis across the bed, stays round.
    let across = add(&mut b, &hole(true, json!({"print_shape": "teardrop"})));
    assert!(!hole_spec(&b.doc).contains("print roofs"));
    assert!(faces_with(&b, across, "roof").is_empty());
}

#[test]
fn a_flat_roof_lies_its_sag_allowance_above_the_circle() {
    // At the printed fit hole_roof_sag (0.4 mm), created with the hole.
    let mut b = block();
    let flat = add(
        &mut b,
        &hole(
            true,
            json!({"print_shape": "flat_roof", "roof_sag": "hole_roof_sag"}),
        ),
    );
    let spec = hole_spec(&b.doc);
    assert!(
        spec.contains("print roofs [(0, 0, 1)] flat roof 0.400000"),
        "{spec}"
    );
    assert!(spec.contains("diameter 6.000000"), "{spec}");
    assert_eq!(
        faces_with(&b, flat, "roof"),
        [
            format!("{flat}:hole0.roof"),
            format!("{flat}:hole0.roof0"),
            format!("{flat}:hole0.roof1")
        ]
    );
    let sag = b.doc.parameters().find("hole_roof_sag").unwrap();
    assert!((b.doc.parameters().value(sag).unwrap() - 0.4).abs() < 1e-9);
    // The roof follows its parameter.
    b.doc.set_parameter("hole_roof_sag", 0.6).unwrap();
    assert!(hole_spec(&b.doc).contains("flat roof 0.600000"));
    // Left out, the sag allowance is 0.4 mm.
    add(&mut b, &hole(true, json!({"print_shape": "flat_roof"})));
    assert!(hole_spec(&b.doc).contains("flat roof 0.400000"));
}

#[test]
fn print_shapes_leave_out_what_they_cannot_shape() {
    let mut b = block();
    for (extra, expected) in [
        (
            json!({"print_shape": "teardrop",
                "press_fit": {"shape": "hex", "interference": 0.1}}),
            "own section",
        ),
        (
            json!({"print_shape": "teardrop", "taper": "5 deg"}),
            "tapered hole has no print shape",
        ),
        (
            json!({"print_shape": "teardrop",
                "thread": {"designation": "M6x1", "class": "6H", "modeled": true}}),
            "make the thread cosmetic",
        ),
        (
            json!({"print_shape": "teardrop", "roof_sag": 0.4}),
            "only a flat roof has `roof_sag`",
        ),
        (
            json!({"print_shape": "flat_roof", "roof_sag": 0}),
            "greater than zero",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&hole(true, extra.clone()), None)
            .err()
            .unwrap_or_else(|| panic!("{extra} was accepted"))
            .to_string();
        assert!(
            error.contains(expected),
            "{extra}: '{expected}' not in '{error}'"
        );
    }
    // A cosmetic thread's bore takes the roof.
    add(
        &mut b,
        &hole(
            true,
            json!({"print_shape": "teardrop",
                "thread": {"designation": "M6x1", "class": "6H"}}),
        ),
    );
    assert!(hole_spec(&b.doc).contains("print roofs [(0, 0, 1)] teardrop"));
}
