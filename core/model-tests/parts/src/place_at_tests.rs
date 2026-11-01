// SPDX-License-Identifier: MIT
//! Parts placed by picked geometry (mitcad#656) with the mock kernel: a
//! pulley on a shaft's side face following the shaft, a gear on an axis of
//! its own, a pulley at a sketch point following its sketch's plane, a
//! coil and a hole by the placement's other forms, a gear turned by its
//! mesh on its own axis, the old forms building as before, and the
//! refusals; Gear Pair and Belt Drive on axes pointing opposite ways and on
//! a shaft in another component's occurrence.

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::{SketchPlane, ValueInput};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{FaceName, SegmentKey};
use mitcad_model::transform::Transform;
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

fn ok(doc: &Document<MockKernel>, uid: FeatureUid) {
    assert_eq!(status(doc, uid), FeatureStatus::Ok, "{uid}");
}

fn rejected(doc: &mut Document<MockKernel>, value: Value) -> String {
    match add(doc, value) {
        Err(error) => error.to_string(),
        Ok(uid) => match status(doc, uid) {
            FeatureStatus::Failed(message) => message,
            other => panic!("expected a failure, got {other:?}"),
        },
    }
}

fn near3(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1.0e-6)
}

/// The mount frame a feature's result keeps: its origin and its axis.
fn mount(doc: &Document<MockKernel>, uid: FeatureUid, name: &str) -> ([f64; 3], [f64; 3]) {
    let frame = doc
        .last_result()
        .result(uid)
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get(name).copied())
        .unwrap_or_else(|| panic!("{uid} has no frame {name}"));
    (
        frame.translation,
        std::array::from_fn(|r| frame.linear[r][2]),
    )
}

/// A shaft: a circle of diameter `shaft_d` (8 mm) at the sketch origin
/// extruded `shaft_l` (30 mm) up z; its body and side face.
fn shaft(doc: &mut Document<MockKernel>) -> (String, String) {
    for (name, expression) in [("shaft_d", "8 mm"), ("shaft_l", "30 mm")] {
        command(
            doc,
            json!({"cmd": "add_parameter", "name": name, "expression": expression}),
        );
    }
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let added = doc
        .add_circle(sketch, [0.0, 0.0], &ValueInput::Name("shaft_d".into()))
        .unwrap();
    let extrude = add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": added.region}],
               "extent": {"type": "distance", "distance": "shaft_l"},
               "operation": "new_body"}),
    )
    .unwrap();
    ok(doc, extrude);
    let side = FaceName::side(extrude, SegmentKey::closed(added.curves[0]));
    (format!("{extrude}.b0"), side.to_string())
}

/// A GT2 pulley of 40 teeth with `extra`.
fn pulley(extra: Value) -> Value {
    let mut value = json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm",
                           "teeth": 40, "belt_width": 6, "operation": "new_body"});
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

/// A spur gear of module 2 and 20 teeth with `extra`.
fn gear(extra: Value) -> Value {
    let mut value = json!({"type": "gear", "kind": "spur", "module": 2, "teeth": 20,
                           "face_width": 10, "operation": "new_body"});
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

#[test]
fn a_pulley_on_a_shaft_by_at_follows_the_shaft() {
    let mut doc = Document::new(MockKernel::default());
    let (body, side) = shaft(&mut doc);
    // Picked near the top: 2 mm down from the top end, facing down.
    let placed = add(
        &mut doc,
        pulley(
            json!({"at": {"axis": {"body": body, "face": side}, "offset": 2,
                             "near": [4, 0, 29]},
                      "bore_clearance": 0.2}),
        ),
    )
    .unwrap();
    ok(&doc, placed);
    let (origin, axis) = mount(&doc, placed, "bore");
    assert!(near3(origin, [0.0, 0.0, 28.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    let values = query(&doc, json!({"query": "pulley_values", "uid": placed}));
    assert!(values.get("error").is_none(), "{values}");
    assert!(
        (values["bore_diameter"].as_f64().unwrap() - 8.2).abs() < 1.0e-9,
        "{values}"
    );
    assert_eq!(values["shaft_diameter"], json!(8.0), "{values}");
    // A longer shaft moves its top end, and the pulley with it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_l", "expression": "40 mm"}),
    );
    ok(&doc, placed);
    let (origin, _) = mount(&doc, placed, "bore");
    assert!(near3(origin, [0.0, 0.0, 38.0]), "{origin:?}");
    // The place_frame query gives the same frame.
    let frame = query(
        &doc,
        json!({"query": "place_frame",
               "at": {"axis": {"body": body, "face": side}, "offset": 2, "near": [4, 0, 39]}}),
    );
    assert_eq!(frame["seat"]["diameter"], json!(8.0), "{frame}");
    let origin: [f64; 3] = serde_json::from_value(frame["origin"].clone()).unwrap();
    assert!(near3(origin, [0.0, 0.0, 38.0]), "{frame}");
}

#[test]
fn a_hole_by_at_goes_into_the_shaft_along_its_axis() {
    let mut doc = Document::new(MockKernel::default());
    let (body, side) = shaft(&mut doc);
    let hole = add(
        &mut doc,
        json!({"type": "hole", "diameter": 3, "extent": {"type": "distance", "depth": 10},
               "placement": {"type": "at",
                             "at": {"axis": {"body": body, "face": side}, "near": [4, 0, 29]}}}),
    )
    .unwrap();
    ok(&doc, hole);
    let specs = doc.kernel().specs.borrow();
    let line = specs
        .iter()
        .find(|s| s.starts_with(&format!("hole {hole} at")))
        .expect("the hole's call");
    // From the top end, down the shaft.
    assert!(
        line.contains("30.0], [") && line.contains("-1.0])]"),
        "{line}"
    );
}

/// A hole at a point of a shaft's side goes across the axis into the
/// shaft, from the point moved onto the cylinder (mitcad#719).
#[test]
fn a_hole_on_a_shafts_side_goes_across_its_axis() {
    let mut doc = Document::new(MockKernel::default());
    let (body, side) = shaft(&mut doc);
    let hole = add(
        &mut doc,
        json!({"type": "hole", "diameter": 3, "extent": {"type": "distance", "depth": 4},
               "placement": {"type": "face", "body": body, "face": side,
                             "points": [[0, 9, 10]]}}),
    )
    .unwrap();
    ok(&doc, hole);
    let specs = doc.kernel().specs.borrow();
    let line = specs
        .iter()
        .find(|s| s.starts_with(&format!("hole {hole} at")))
        .expect("the hole's call");
    // On the cylinder of radius 4 at y = 4, z = 10, towards the axis.
    assert!(
        line.contains("([0.0, 4.0, 10.0], [0.0, -1.0, 0.0])"),
        "{line}"
    );
}

/// A hole on a shaft's side at a distance along its axis from its end
/// face, on the side of a point, follows the distance (mitcad#719).
#[test]
fn a_hole_at_a_distance_from_a_shafts_end_follows_it() {
    let mut doc = Document::new(MockKernel::default());
    let (body, side) = shaft(&mut doc);
    let hole = add(
        &mut doc,
        json!({"type": "hole", "diameter": 3, "extent": {"type": "distance", "depth": 4},
               "placement": {"type": "cylinder_offset", "body": body, "face": side,
                             "plane": "xy", "offset": "12 mm", "toward": [-9, 0, 0]}}),
    )
    .unwrap();
    ok(&doc, hole);
    let line = |doc: &Document<MockKernel>| {
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .rev()
            .find(|s| s.starts_with(&format!("hole {hole} at")))
            .cloned()
            .expect("the hole's call")
    };
    // 12 mm up from the shaft's bottom (the XY plane), on its -X side,
    // towards the axis.
    let at = line(&doc);
    assert!(at.contains("([-4.0, 0.0, 12.0], [1.0, 0.0, 0.0])"), "{at}");
    // The distance is a parameter of the hole.
    let name = query(&doc, json!({"query": "feature", "uid": hole}))["def"]["placement"]["offset"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": name, "expression": "20 mm"}),
    );
    ok(&doc, hole);
    let at = line(&doc);
    assert!(at.contains("([-4.0, 0.0, 20.0], [1.0, 0.0, 0.0])"), "{at}");
}

#[test]
fn a_gear_on_an_axis_of_its_own_keeps_its_bore() {
    let mut doc = Document::new(MockKernel::default());
    // `at` on an origin axis, 10 mm up it, with a plain bore of its own.
    let placed = add(
        &mut doc,
        gear(json!({"at": {"axis": "z", "offset": 10},
                    "bore": {"type": "plain", "diameter": 8}})),
    )
    .unwrap();
    ok(&doc, placed);
    let (origin, axis) = mount(&doc, placed, "gear");
    assert!(near3(origin, [0.0, 0.0, 10.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");
    let values = query(&doc, json!({"query": "gear_values", "uid": placed}));
    assert!(values.get("error").is_none(), "{values}");
    assert_eq!(values["bore_diameter"], json!(8.0), "{values}");
    assert!(values["seat"]["diameter"].is_null(), "{values}");
    // Without a bore it has none.
    let bare = add(&mut doc, gear(json!({"at": {"axis": "z", "flip": true}}))).unwrap();
    ok(&doc, bare);
    let values = query(&doc, json!({"query": "gear_values", "uid": bare}));
    assert!(values.get("bore_diameter").is_none(), "{values}");
    let (_, axis) = mount(&doc, bare, "gear");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    // A seat on an axis of its own takes a plain bore too.
    let seated = add(
        &mut doc,
        gear(json!({"seat": {"shaft": "z", "offset": 20},
                    "bore": {"type": "plain", "diameter": 6}})),
    )
    .unwrap();
    ok(&doc, seated);
    let (origin, _) = mount(&doc, seated, "gear");
    assert!(near3(origin, [0.0, 0.0, 20.0]), "{origin:?}");
}

#[test]
fn a_pulley_at_a_sketch_point_follows_its_sketch_plane() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "h", "expression": "5 mm"}),
    );
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": "h"}}),
    )
    .unwrap();
    let sketch = add(
        &mut doc,
        json!({"type": "sketch", "plane": plane,
               "entities": [{"id": "p1", "type": "point", "at": [10, 5]}]}),
    )
    .unwrap();
    let placed = add(
        &mut doc,
        pulley(json!({"at": {"point": {"sketch": sketch, "point": "p1"}}})),
    )
    .unwrap();
    ok(&doc, placed);
    let (origin, axis) = mount(&doc, placed, "bore");
    assert!(near3(origin, [10.0, 5.0, 5.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");
    // The plane moves up: the sketch and the pulley with it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "h", "expression": "12 mm"}),
    );
    ok(&doc, placed);
    let (origin, _) = mount(&doc, placed, "bore");
    assert!(near3(origin, [10.0, 5.0, 12.0]), "{origin:?}");
}

#[test]
fn a_coil_on_an_origin_plane_by_offsets_from_its_axes() {
    let mut doc = Document::new(MockKernel::default());
    let coil = add(
        &mut doc,
        json!({"type": "coil", "at": {"face": "xy", "offset1": 20, "offset2": 15},
               "diameter": 10,
               "helix": {"type": "revolutions_and_pitch", "revolutions": 3, "pitch": 5},
               "section_size": 2, "operation": "new_body"}),
    )
    .unwrap();
    ok(&doc, coil);
    let specs = doc.kernel().specs.borrow();
    let line = specs
        .iter()
        .find(|s| s.starts_with(&format!("coil {coil} ")))
        .expect("the coil's call");
    assert!(
        line.contains("at [20.0, 15.0, 0.0] axis [0.0, 0.0, 1.0]"),
        "{line}"
    );
}

#[test]
fn a_gear_on_its_own_axis_turns_to_mesh() {
    let mut doc = Document::new(MockKernel::default());
    let first = add(&mut doc, gear(json!({"at": {"axis": "z"}}))).unwrap();
    ok(&doc, first);
    // The old form: placed by its mate, in the direction of its x axis.
    let by_mate = add(&mut doc, gear(json!({"mesh_with": {"gear": first}}))).unwrap();
    ok(&doc, by_mate);
    let (origin, _) = mount(&doc, by_mate, "gear");
    assert!(
        (origin[0].hypot(origin[1]) - 40.0).abs() < 1.0e-9,
        "{origin:?}"
    );
    // The same place by its own point: turned the same way.
    let own = add(
        &mut doc,
        gear(json!({"at": {"point": {"point": origin}, "plane": "xy"},
                    "mesh_with": {"gear": first}})),
    )
    .unwrap();
    ok(&doc, own);
    assert!(doc.warnings(own).is_empty(), "{:?}", doc.warnings(own));
    let bounds = |uid: FeatureUid| {
        doc.body_shape(BodyUid::new(uid, 0))
            .and_then(|s| s.bounds)
            .expect("bounds")
    };
    let (a, b) = (bounds(by_mate), bounds(own));
    assert!(near3(a.min, b.min) && near3(a.max, b.max), "{a:?} {b:?}");
    // A millimetre further out: made, with a warning.
    let further = origin.map(|v| v * 41.0 / 40.0);
    let apart = add(
        &mut doc,
        gear(json!({"at": {"point": {"point": further}, "plane": "xy"},
                    "mesh_with": {"gear": first}})),
    )
    .unwrap();
    ok(&doc, apart);
    let warnings = doc.warnings(apart);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("the axes are 41 mm apart, the mesh needs 40 mm")),
        "{warnings:?}"
    );
    // The pair in one undo step.
    let depth = doc.undo_depth();
    let made = command(
        &mut doc,
        json!({"cmd": "add_gear_pair",
               "first": {"kind": "spur", "module": 2, "teeth": 20, "face_width": 10,
                         "at": {"face": "xy", "offset1": 100, "offset2": 0},
                         "operation": "new_body"},
               "second": {"kind": "spur", "module": 2, "teeth": 20, "face_width": 10,
                          "at": {"face": "xy", "offset1": 140, "offset2": 0},
                          "operation": "new_body"}}),
    );
    assert_eq!(doc.undo_depth(), depth + 1, "{made}");
    let second: FeatureUid = serde_json::from_value(made["uids"][1].clone()).unwrap();
    ok(&doc, second);
    assert!(
        doc.warnings(second).is_empty(),
        "{:?}",
        doc.warnings(second)
    );
}

#[test]
fn old_forms_build_and_save_as_before() {
    let mut doc = Document::new(MockKernel::default());
    let on_plane = add(&mut doc, pulley(json!({"plane": "xy", "center": [5, 0]}))).unwrap();
    ok(&doc, on_plane);
    let (origin, _) = mount(&doc, on_plane, "bore");
    assert!(near3(origin, [5.0, 0.0, 0.0]), "{origin:?}");
    let cylinder = add(
        &mut doc,
        json!({"type": "cylinder", "plane": "xy", "center": [1, 2], "diameter": 4,
               "height": 5, "operation": "new_body"}),
    )
    .unwrap();
    ok(&doc, cylinder);
    let coil = add(
        &mut doc,
        json!({"type": "coil", "plane": "xz", "center": [0, 0], "diameter": 10,
               "helix": {"type": "revolutions_and_pitch", "revolutions": 2, "pitch": 5},
               "section_size": 2, "operation": "new_body"}),
    )
    .unwrap();
    ok(&doc, coil);
    for uid in [on_plane, cylinder, coil] {
        let params = doc.parameters();
        let def = doc
            .feature(uid)
            .unwrap()
            .def
            .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
            .unwrap();
        let saved = serde_json::to_value(&def).unwrap();
        assert!(saved.get("at").is_none(), "{saved}");
        assert!(saved.get("plane").is_some(), "{saved}");
    }
}

#[test]
fn placements_that_do_not_fit_are_refused() {
    let mut doc = Document::new(MockKernel::default());
    let (body, side) = shaft(&mut doc);
    let message = rejected(
        &mut doc,
        pulley(json!({"plane": "xy", "at": {"axis": "z"}})),
    );
    assert!(message.contains("takes no plane"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "rack", "module": 2, "teeth": 10, "face_width": 10,
               "height": 10, "at": {"axis": "z"}, "operation": "new_body"}),
    );
    assert!(message.contains("a rack keeps its plane"), "{message}");
    let message = rejected(
        &mut doc,
        pulley(json!({"at": {"axis": "z", "point": "origin"}})),
    );
    assert!(
        message.contains("one of an axis, a point and a face"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        json!({"type": "box", "length": 5, "width": 5, "height": 5, "operation": "new_body",
               "at": {"face": {"body": body, "face": side}, "offset1": 2, "offset2": 3}}),
    );
    assert!(
        message.contains("measured from two of its edges"),
        "{message}"
    );
    let message = rejected(&mut doc, pulley(json!({"at": {"point": "origin"}})));
    assert!(
        message.contains("needs a plane or a planar face"),
        "{message}"
    );
}

/// A shaft's flat turns a part: `align` on a planar face takes its normal,
/// square to the axis, as the part's x axis (mitcad#656).
#[test]
fn a_pulley_turns_to_a_shafts_flat() {
    let mut doc = Document::new(MockKernel::default());
    // A box-like shaft up z, 10 x 10 x 30, its side2 a flat at 45 degrees.
    let shaft = add(
        &mut doc,
        json!({"type": "box", "plane": "xy", "corner": [-5, -5], "length": 10,
               "width": 10, "height": 30, "operation": "new_body"}),
    )
    .unwrap();
    ok(&doc, shaft);
    let body = format!("{shaft}.b0");
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let flat = FaceName::new(shaft, "side2", None).to_string();
    doc.kernel().surfaces.borrow_mut().insert(
        flat.clone(),
        SurfaceGeometry::Plane {
            origin: [3.0, 3.0, 15.0],
            normal: [half, half, 0.0],
        },
    );
    let placed = add(
        &mut doc,
        pulley(json!({"at": {"axis": "z", "offset": 5,
                             "align": {"body": body, "face": flat}}})),
    )
    .unwrap();
    ok(&doc, placed);
    let frame = doc
        .last_result()
        .result(placed)
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get("bore").copied())
        .expect("the pulley's frame");
    let x: [f64; 3] = std::array::from_fn(|r| frame.linear[r][0]);
    assert!(near3(x, [half, half, 0.0]), "{x:?}");
    assert!(near3(frame.translation, [0.0, 0.0, 5.0]), "{frame:?}");
    // A face across the axis (the top) gives no direction.
    let top = FaceName::new(shaft, "top", None).to_string();
    doc.kernel().surfaces.borrow_mut().insert(
        top.clone(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 30.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    let message = rejected(
        &mut doc,
        pulley(json!({"at": {"axis": "z", "align": {"body": body, "face": top}}})),
    );
    assert!(
        message.contains("lies square to the placement's axis"),
        "{message}"
    );
}

/// `align_side` turns a line's direction round where the part's y axis
/// would point away from it, so that a part placed from a face's far edge
/// grows into the face; without it the line is taken as it runs
/// (mitcad#656).
#[test]
fn align_side_turns_the_line_round() {
    let mut doc = Document::new(MockKernel::default());
    let x_of = |doc: &Document<MockKernel>, uid: FeatureUid| -> [f64; 3] {
        let frame = doc
            .last_result()
            .result(uid)
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.mounts.as_ref())
            .and_then(|m| m.get("bore").copied())
            .expect("the pulley's frame");
        std::array::from_fn(|r| frame.linear[r][0])
    };
    let as_it_runs = add(&mut doc, pulley(json!({"at": {"axis": "z", "align": "x"}}))).unwrap();
    ok(&doc, as_it_runs);
    assert!(near3(x_of(&doc, as_it_runs), [1.0, 0.0, 0.0]));
    let toward = add(
        &mut doc,
        pulley(json!({"at": {"axis": "z", "align": "x", "align_side": [0, 1, 0]}})),
    )
    .unwrap();
    ok(&doc, toward);
    assert!(near3(x_of(&doc, toward), [1.0, 0.0, 0.0]));
    let away = add(
        &mut doc,
        pulley(json!({"at": {"axis": "z", "align": "x", "align_side": [0, -1, 0]}})),
    )
    .unwrap();
    ok(&doc, away);
    assert!(near3(x_of(&doc, away), [-1.0, 0.0, 0.0]));
    // Only with align.
    let message = rejected(
        &mut doc,
        pulley(json!({"at": {"axis": "z", "align_side": [0, 1, 0]}})),
    );
    assert!(message.contains("only a placement with align"), "{message}");
}

/// `extra`'s fields over `value`'s.
fn with(mut value: Value, extra: Value) -> Value {
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

/// A pair's definitions as `add_gear_pair` and `add_belt_drive` take them
/// (no `type`): a spur gear of module 2 and 20 teeth, a GT2 pulley of 40
/// teeth, placed by `place`.
fn pair_gear(place: Value) -> Value {
    with(
        json!({"kind": "spur", "module": 2, "teeth": 20, "face_width": 10,
               "operation": "new_body"}),
        place,
    )
}

fn pair_pulley(place: Value) -> Value {
    with(
        json!({"kind": "toothed", "profile": "gt2_2mm", "teeth": 40, "belt_width": 6,
               "operation": "new_body"}),
        place,
    )
}

/// Axes that point opposite ways (mitcad#749): each part is built from its
/// placement along its own axis, so they grow apart and the offset along
/// the axes is measured from the wrong end. Gear Pair's and Belt Drive's
/// queries, `belt_check` and a gear meshed on its own axis warn.
#[test]
fn axes_pointing_opposite_ways_warn() {
    let mut doc = Document::new(MockKernel::default());
    let first = json!({"axis": "z"});
    let same = json!({"point": {"point": [40, 0, 0]}, "plane": "xy"});
    let flipped = json!({"point": {"point": [40, 0, 0]}, "plane": "xy", "flip": true});
    for extra in [
        json!({"query": "gear_pair"}),
        json!({"query": "belt_drive", "profile": "gt2_2mm"}),
    ] {
        let ask = |second: &Value| {
            query(
                &doc,
                with(
                    json!({"ratio": 1, "first": first, "second": second}),
                    extra.clone(),
                ),
            )
        };
        let result = ask(&flipped);
        let warning = result["warning"].as_str().unwrap_or_default();
        assert!(
            warning.contains("the second axis points the opposite way from the first"),
            "{result}"
        );
        assert!(warning.contains("flip one"), "{warning}");
        // The same way round: nothing to say.
        let result = ask(&same);
        assert!(result.get("warning").is_none(), "{result}");
    }

    // Two pulleys made so: their belt check says it too.
    let made = command(
        &mut doc,
        json!({"cmd": "add_belt_drive",
               "first": pair_pulley(json!({"at": first})),
               "second": pair_pulley(json!({"at": flipped}))}),
    );
    let check = query(
        &doc,
        json!({"query": "belt_check", "pulleys": made["uids"].clone()}),
    );
    let distance = check["center_distance"].as_f64().unwrap();
    assert!((distance - 40.0).abs() < 1.0e-6, "{check}");
    let warning = check["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("points the opposite way"), "{check}");

    // A gear on its own axis meshed with a gear on the opposite axis: made,
    // with a warning.
    let mate = add(&mut doc, gear(json!({"at": {"axis": "z"}}))).unwrap();
    ok(&doc, mate);
    let meshed = add(
        &mut doc,
        gear(json!({"at": flipped, "mesh_with": {"gear": mate}})),
    )
    .unwrap();
    ok(&doc, meshed);
    let warnings = doc.warnings(meshed);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("the gear's axis points the opposite way from its mate's")),
        "{warnings:?}"
    );
    // The same way round: no warning.
    let aligned = add(
        &mut doc,
        gear(json!({"at": same, "mesh_with": {"gear": mate}})),
    )
    .unwrap();
    ok(&doc, aligned);
    assert!(
        doc.warnings(aligned).is_empty(),
        "{:?}",
        doc.warnings(aligned)
    );
}

/// A shaft in another component's occurrence (mitcad#754): Gear Pair's and
/// Belt Drive's queries take it as a seat with its occurrence link, as the
/// parts' definitions do, and work out the distance where the occurrence
/// puts the shaft; the pair is made there. A gear that meshes stays on a
/// shaft of its own component.
#[test]
fn pairs_take_a_shaft_in_another_components_occurrence() {
    let mut doc = Document::new(MockKernel::default());
    // The shaft in Motor:1 (O1), 80 mm along X; the pair in the root.
    doc.create_component(
        Some("Motor"),
        Transform::translation([80.0, 0.0, 0.0]),
        true,
    )
    .unwrap();
    let (body, side) = shaft(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    // Picked near its bottom, in Motor's coordinates: from the bottom end
    // up the shaft, as the root's z axis runs.
    let seat = json!({"shaft": {"body": body, "face": side}, "near": [4, 0, 1],
                      "link": {"source": "O1"}});
    for extra in [
        json!({"query": "gear_pair"}),
        json!({"query": "belt_drive", "profile": "gt2_2mm"}),
    ] {
        let result = query(
            &doc,
            with(
                json!({"ratio": 1, "first": {"axis": "z"}, "second": {"seat": seat}}),
                extra,
            ),
        );
        let distance = result["center_distance"].as_f64().unwrap();
        assert!((distance - 80.0).abs() < 1.0e-6, "{result}");
        let axial = result["axial_offset"].as_f64().unwrap();
        assert!(axial.abs() < 1.0e-6, "{result}");
        assert!(result.get("warning").is_none(), "{result}");
    }
    // A seat takes no field of a placement's.
    let error = doc
        .query(
            &json!({"query": "belt_drive", "ratio": 1, "profile": "gt2_2mm",
                    "first": {"axis": "z"}, "second": {"seat": seat, "flip": true}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("flip"), "{error}");

    // A belt drive with its second pulley there.
    let made = command(
        &mut doc,
        json!({"cmd": "add_belt_drive",
               "first": pair_pulley(json!({"at": {"axis": "z"}})),
               "second": pair_pulley(json!({"seat": seat}))}),
    );
    let pulleys: [FeatureUid; 2] = serde_json::from_value(made["uids"].clone()).unwrap();
    ok(&doc, pulleys[1]);
    let (origin, axis) = mount(&doc, pulleys[1], "bore");
    assert!(near3(origin, [80.0, 0.0, 0.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");
    let check = query(
        &doc,
        json!({"query": "belt_check", "pulleys": made["uids"].clone()}),
    );
    let distance = check["center_distance"].as_f64().unwrap();
    assert!((distance - 80.0).abs() < 1.0e-6, "{check}");

    // A gear pair with its first gear there, the second 40 mm from it.
    let made = command(
        &mut doc,
        json!({"cmd": "add_gear_pair",
               "first": pair_gear(json!({"seat": seat})),
               "second": pair_gear(json!({"at": {"point": {"point": [40, 0, 0]},
                                                 "plane": "xy"}}))}),
    );
    let gears: [FeatureUid; 2] = serde_json::from_value(made["uids"].clone()).unwrap();
    ok(&doc, gears[0]);
    ok(&doc, gears[1]);
    let (origin, _) = mount(&doc, gears[0], "gear");
    assert!(near3(origin, [80.0, 0.0, 0.0]), "{origin:?}");
    assert!(
        doc.warnings(gears[1]).is_empty(),
        "{:?}",
        doc.warnings(gears[1])
    );
    // The gear that meshes on that shaft: refused, nothing of the pair
    // made.
    let depth = doc.undo_depth();
    let error = doc
        .command(
            &json!({"cmd": "add_gear_pair",
                    "first": pair_gear(json!({"at": {"point": {"point": [40, 0, 0]},
                                                    "plane": "xy"}})),
                    "second": pair_gear(json!({"seat": seat}))})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("not placed by a mesh too"), "{error}");
    assert_eq!(doc.undo_depth(), depth);
}

/// A pair whose second part is refused (mitcad#758 item 6): the first was
/// committed as a step of its own, which cleared the redo steps, and undone
/// again, which left it waiting on redo. Now both go in one step or none.
#[test]
fn a_refused_pair_keeps_the_undo_and_redo_steps() {
    let mut doc = Document::new(MockKernel::default());
    let placed = add(
        &mut doc,
        gear(json!({"at": {"point": {"point": [0, 0, 0]}, "plane": "xy"}})),
    )
    .unwrap();
    ok(&doc, placed);
    // A gear added and undone: its step waits on redo.
    let lone = add(
        &mut doc,
        gear(json!({"at": {"point": {"point": [100, 0, 0]}, "plane": "xy"}})),
    )
    .unwrap();
    ok(&doc, lone);
    let undone = doc.undo().expect("the gear's step");
    let depth = doc.undo_depth();
    let undo_label = doc.undo_label().map(str::to_owned);
    let revision = doc.revision();
    assert_eq!(doc.redo_label(), Some(undone.as_str()));

    // The first gear is fine, the second is seated on an axis and placed by
    // the mesh too: refused.
    let refusals = [
        (
            json!({"cmd": "add_gear_pair",
                   "first": pair_gear(json!({"at": {"point": {"point": [40, 0, 0]},
                                                    "plane": "xy"}})),
                   "second": pair_gear(json!({"seat": {"shaft": "z", "offset": 20}}))}),
            "not placed by a mesh too",
        ),
        // A belt drive whose second pulley has an empty name (a name the
        // first took is allowed, mitcad#1107).
        (
            json!({"cmd": "add_belt_drive",
                   "first": pair_pulley(json!({"at": {"axis": "z"}})),
                   "second": pair_pulley(json!({"at": {"axis": "z"}})),
                   "names": ["Drive", " "]}),
            "the feature name is empty",
        ),
    ];
    for (refused, expected) in refusals {
        let cmd = refused["cmd"].clone();
        let error = doc.command(&refused.to_string()).unwrap_err().to_string();
        assert!(error.contains(expected), "{cmd}: {error}");
        assert_eq!(doc.undo_depth(), depth, "{cmd}: {error}");
        assert_eq!(doc.undo_label().map(str::to_owned), undo_label, "{cmd}");
        assert_eq!(doc.redo_label(), Some(undone.as_str()), "{cmd}");
        assert_eq!(doc.revision(), revision, "{cmd}");
        assert_eq!(doc.features().count(), 1, "{cmd}");
    }

    // Redo still brings the undone gear back, and only it.
    assert_eq!(doc.redo().as_deref(), Some(undone.as_str()));
    assert_eq!(doc.redo_label(), None);
    assert_eq!(doc.undo_depth(), depth + 1);
    ok(&doc, lone);
}
