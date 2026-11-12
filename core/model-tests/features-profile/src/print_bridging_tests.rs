// SPDX-License-Identifier: MIT
//! Print bridging of counterbores that open toward the print bed
//! (mitcad#1516): a hole going up from the bed whose counterbore (or nut
//! pocket at its start) would leave the bore's ring in the air gets a
//! sacrificial layer or stepped bridges above the recess's floor, the
//! design's layer height thick each (0.2 mm without a print process); a
//! hole whose recess opens away from the bed is cut as it is. The mock
//! kernel's spec writes the mode, the layer and which holes are bridged;
//! the geometry is `test_print_bridging` in geometry/tests/test_hole.cpp.

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, FeatureUid};
use mitcad_model_api::DocumentApi;

/// The block's top (z = 20, normal +z).
const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

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

fn command(doc: &mut Document<MockKernel>, command: Value) {
    doc.command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
}

/// An M3 clearance hole through the block with its counterbore (6.5 wide,
/// 3.4 deep) and `extra` fields: from the bed, up from (30, 20, 0) on the
/// bottom, or from the top, down from (30, 20, 20).
fn hole(from_bed: bool, extra: Value) -> Value {
    let placement = if from_bed {
        json!({"type": "plane", "plane": {"origin": [0, 0, 0], "normal": [0, 0, -1]},
            "points": [[30, 20, 0]]})
    } else {
        json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[30, 20, 20]]})
    };
    let mut value = json!({"type": "hole", "placement": placement, "diameter": 3.4,
        "kind": {"type": "counterbore", "diameter": 6.5, "depth": 3.4},
        "extent": {"type": "through_all"}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    value
}

fn add(b: &mut Block, value: Value) -> FeatureUid {
    let uid = b.doc.add_feature(&def(value), None).unwrap().uid;
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    uid
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

#[test]
fn a_sacrificial_layer_lies_on_the_counterbore_floor_and_follows_the_layer_height() {
    let mut b = block();
    // No print process: the standard printer's 0.2 mm layers (mitcad#1498
    // D5). The kernel bridges the counterbore 3.4 deep: the layer lies on
    // its floor.
    let uid = add(&mut b, hole(true, json!({"print_bridging": "sacrificial"})));
    let spec = hole_spec(&b.doc);
    assert!(
        spec.contains("Counterbore { diameter: 6.5, depth: 3.4 }"),
        "{spec}"
    );
    assert!(
        spec.contains("bridging Sacrificial layer 0.200000 holes [true]"),
        "{spec}"
    );
    assert_eq!(
        faces_with(&b, uid, "bridge"),
        [format!("{uid}:hole0.bridge0")]
    );
    let stored = serde_json::from_str::<Value>(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()["def"]
        .clone();
    assert_eq!(stored["print_bridging"], "sacrificial", "{stored}");

    // The design's process profile sets the layer, and the hole follows it.
    command(
        &mut b.doc,
        json!([
            {"cmd": "create_print_profile", "kind": "process", "name": "Fine",
             "values": {"layer_height": 0.12}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile",
             "value": "fine"},
        ]),
    );
    assert!(
        hole_spec(&b.doc).contains("bridging Sacrificial layer 0.120000"),
        "{}",
        hole_spec(&b.doc)
    );
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    // The design's own layer height wins over its profile's.
    command(
        &mut b.doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "layer_height", "value": 0.16}),
    );
    assert!(
        hole_spec(&b.doc).contains("bridging Sacrificial layer 0.160000"),
        "{}",
        hole_spec(&b.doc)
    );
}

#[test]
fn stepped_bridges_take_two_layers() {
    let mut b = block();
    let uid = add(&mut b, hole(true, json!({"print_bridging": "stepped"})));
    let spec = hole_spec(&b.doc);
    assert!(
        spec.contains("bridging Stepped layer 0.200000 holes [true]"),
        "{spec}"
    );
    let bridges = faces_with(&b, uid, "bridge");
    for part in [
        "bridge0",
        "bridge0.side0",
        "bridge0.side1",
        "bridge1",
        "bridge1.side3",
    ] {
        assert!(
            bridges.contains(&format!("{uid}:hole0.{part}")),
            "{part} not in {bridges:?}"
        );
    }
    // Its layers follow the layer height too.
    command(
        &mut b.doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Print Process",
               "key": "layer_height", "value": 0.28}),
    );
    assert!(
        hole_spec(&b.doc).contains("bridging Stepped layer 0.280000"),
        "{}",
        hole_spec(&b.doc)
    );

    // A nut pocket at the start is bridged the same way.
    add(
        &mut b,
        hole(
            true,
            json!({"kind": {"type": "nut_pocket", "across_flats": 5.8, "depth": 2.7},
                   "print_bridging": "stepped"}),
        ),
    );
    let spec = hole_spec(&b.doc);
    assert!(spec.contains("NutPocket"), "{spec}");
    assert!(
        spec.contains("bridging Stepped layer 0.280000 holes [true]"),
        "{spec}"
    );
}

#[test]
fn a_counterbore_opening_away_from_the_bed_is_not_bridged() {
    let mut b = block();
    // Into the top, down towards the bed: the counterbore opens upward and
    // prints as it is.
    let uid = add(
        &mut b,
        hole(false, json!({"print_bridging": "sacrificial"})),
    );
    let spec = hole_spec(&b.doc);
    assert!(!spec.contains("bridging"), "{spec}");
    assert!(faces_with(&b, uid, "bridge").is_empty());
    assert!(
        b.doc
            .infos(uid)
            .iter()
            .any(|i| i.contains("opens toward the print bed")),
        "{:?}",
        b.doc.infos(uid)
    );
    // Once the top is the print bed face, the same hole opens toward it.
    b.doc.kernel().surfaces.borrow_mut().insert(
        TOP.to_owned(),
        SurfaceGeometry::Plane {
            origin: [30.0, 20.0, 20.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    command(
        &mut b.doc,
        json!({"cmd": "set_print_bed_face", "uid": "F2.b0", "face": TOP}),
    );
    assert!(
        hole_spec(&b.doc).contains("bridging Sacrificial layer 0.200000 holes [true]"),
        "{}",
        hole_spec(&b.doc)
    );
    assert_eq!(faces_with(&b, uid, "bridge").len(), 1);
}

#[test]
fn bridging_needs_a_recess_at_the_start() {
    let mut b = block();
    for (extra, expected) in [
        (
            json!({"kind": {"type": "countersink", "diameter": 6.5, "angle": "90 deg"},
                   "print_bridging": "sacrificial"}),
            "bridges a counterbore",
        ),
        (
            json!({"kind": {"type": "nut_pocket", "across_flats": 5.8, "depth": 2.7, "slot": 30},
                   "print_bridging": "stepped"}),
            "without a slot",
        ),
        (
            json!({"print_bridging": "stepped", "taper": "2 deg"}),
            "no taper or print shape",
        ),
        (
            json!({"print_bridging": "stepped",
                   "thread": {"designation": "M4x0.7", "class": "6H", "modeled": true}}),
            "make the thread cosmetic",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&def(hole(true, extra.clone())), None)
            .err()
            .unwrap_or_else(|| panic!("{extra} was accepted"))
            .to_string();
        assert!(
            error.contains(expected),
            "{extra}: '{expected}' not in '{error}'"
        );
    }
    // Without bridging (the default) nothing is bridged.
    add(&mut b, hole(true, json!({})));
    assert!(!hole_spec(&b.doc).contains("bridging"));
}
