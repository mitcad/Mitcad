// SPDX-License-Identifier: MIT
//! Pulleys and gears seated on shafts (mitcad#578) with the mock kernel:
//! a seat on a shaft's side face (measured from the end nearer the pick)
//! and on its end edge, the offset and the flip, the bore from the shaft
//! and its clearance, the shaft made thicker and longer and moved with its
//! component (the part follows), belts and meshes from a seated part, and
//! the refusals.

use serde_json::{Value, json};

use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::{SketchPlane, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, SegmentKey};
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
            FeatureStatus::Failed(message) => message.agent_text().to_owned(),
            other => panic!("expected a failure, got {other:?}"),
        },
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0e-6
}

fn near3(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| near(a[i], b[i]))
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

/// A shaft in the active component: a circle of diameter `shaft_d` at the
/// sketch origin extruded `shaft_l` up z (8 and 30 mm), its body, side
/// face and top end edge (a circle as the mock's `edge_curves` gives it).
struct Shaft {
    body: String,
    side: String,
    end_face: String,
    end_edge: String,
}

fn shaft(doc: &mut Document<MockKernel>) -> Shaft {
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
    let end = FaceName::end(extrude, added.region.clone());
    let edge = EdgeName::new(end.clone(), side.clone());
    Shaft {
        body: format!("{extrude}.b0"),
        side: side.to_string(),
        end_face: end.to_string(),
        end_edge: edge.to_string(),
    }
}

/// The top end edge of a shaft `radius` across at `top`, as the kernel
/// gives it.
fn end_circle(doc: &Document<MockKernel>, shaft: &Shaft, radius: f64, top: f64) {
    doc.kernel().edge_curves.borrow_mut().insert(
        shaft.end_edge.clone(),
        CurveGeometry::Circle {
            center: [0.0, 0.0, top],
            normal: [0.0, 0.0, 1.0],
            radius,
            start: [radius, 0.0, top],
            end: [radius, 0.0, top],
        },
    );
}

/// A GT2 pulley of 40 teeth (groove bottoms 23.43 mm across) with `extra`.
fn pulley(extra: Value) -> Value {
    let mut value = json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm",
                           "teeth": 40, "belt_width": 6, "operation": "new_body"});
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

fn bore(doc: &Document<MockKernel>, uid: FeatureUid, values: &str) -> f64 {
    let values = query(doc, json!({"query": values, "uid": uid}));
    assert!(values.get("error").is_none(), "{values}");
    values["bore_diameter"].as_f64().expect("a bore")
}

#[test]
fn a_pulley_sits_on_a_shafts_side_face_from_the_end_nearer_the_pick() {
    let mut doc = Document::new(MockKernel::default());
    let shaft = shaft(&mut doc);
    // Picked near the top: 2 mm down from the top end, facing down.
    let seated = add(
        &mut doc,
        pulley(
            json!({"seat": {"shaft": {"body": shaft.body, "face": shaft.side},
                               "offset": 2, "near": [4, 0, 29]},
                      "bore_clearance": 0.2}),
        ),
    )
    .unwrap();
    ok(&doc, seated);
    let (origin, axis) = mount(&doc, seated, "bore");
    assert!(near3(origin, [0.0, 0.0, 28.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    // The bore: the shaft's 8 mm and the clearance.
    assert!(near(bore(&doc, seated, "pulley_values"), 8.2));
    let values = query(&doc, json!({"query": "pulley_values", "uid": seated}));
    assert!(
        near(values["shaft_diameter"].as_f64().unwrap(), 8.0),
        "{values}"
    );
    assert_eq!(values["seat"]["axis"], json!([0.0, 0.0, -1.0]), "{values}");

    // Flipped: 2 mm up from the bottom end, facing up.
    let flipped = add(
        &mut doc,
        pulley(
            json!({"seat": {"shaft": {"body": shaft.body, "face": shaft.side},
                               "offset": 2, "near": [4, 0, 29], "flip": true}}),
        ),
    )
    .unwrap();
    ok(&doc, flipped);
    let (origin, axis) = mount(&doc, flipped, "bore");
    assert!(near3(origin, [0.0, 0.0, 2.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");
    assert!(near(bore(&doc, flipped, "pulley_values"), 8.0));

    // A thicker shaft widens the bores; a longer one moves the top end and
    // the pulley measured from it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "12 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_l", "expression": "40 mm"}),
    );
    ok(&doc, seated);
    ok(&doc, flipped);
    assert!(near(bore(&doc, seated, "pulley_values"), 12.2));
    let (origin, _) = mount(&doc, seated, "bore");
    assert!(near3(origin, [0.0, 0.0, 38.0]), "{origin:?}");
    let (origin, _) = mount(&doc, flipped, "bore");
    assert!(near3(origin, [0.0, 0.0, 2.0]), "{origin:?}");

    // A keyway in the bore from the shaft (DIN 6885 for 12.2 mm).
    let keyed = add(
        &mut doc,
        pulley(
            json!({"seat": {"shaft": {"body": shaft.body, "face": shaft.side}},
                      "bore_clearance": 0.2, "keyway": true}),
        ),
    )
    .unwrap();
    ok(&doc, keyed);
    let body = format!("{keyed}.b0").parse().unwrap();
    let faces = &doc.body_shape(body).expect("the pulley").faces;
    assert!(
        faces
            .iter()
            .any(|f| f.to_string() == format!("{keyed}:keyway_top")),
        "{faces:?}"
    );

    // Too thick for the grooves: refused as a too large bore.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "23 mm"}),
    );
    match status(&doc, seated) {
        FeatureStatus::Failed(message) => {
            assert!(
                message.contains("the bore reaches the grooves"),
                "{message}"
            )
        }
        other => panic!("expected a failure, got {other:?}"),
    }

    // Saved and opened again: the same seat.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "8 mm"}),
    );
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    ok(&loaded, seated);
    assert_eq!(
        loaded.feature(seated).unwrap().def,
        doc.feature(seated).unwrap().def
    );
    assert!(saved.contains("\"bore_clearance\""), "{saved}");
}

#[test]
fn a_pulley_sits_on_a_shafts_end_edge_and_runs_a_belt() {
    let mut doc = Document::new(MockKernel::default());
    let shaft = shaft(&mut doc);
    end_circle(&doc, &shaft, 4.0, 30.0);
    let seated = add(
        &mut doc,
        pulley(
            json!({"seat": {"shaft": {"body": shaft.body, "edge": shaft.end_edge},
                               "offset": 3}}),
        ),
    )
    .unwrap();
    ok(&doc, seated);
    let (origin, axis) = mount(&doc, seated, "bore");
    assert!(near3(origin, [0.0, 0.0, 27.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    assert!(near(bore(&doc, seated, "pulley_values"), 8.0));
    // Flipped: from the shaft's other end.
    let flipped = add(
        &mut doc,
        pulley(
            json!({"seat": {"shaft": {"body": shaft.body, "edge": shaft.end_edge},
                               "offset": 3, "flip": true}}),
        ),
    )
    .unwrap();
    ok(&doc, flipped);
    let (origin, axis) = mount(&doc, flipped, "bore");
    assert!(near3(origin, [0.0, 0.0, 3.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");

    // A second pulley on one belt with the seated one, 80 mm along the
    // seat's y axis (the plane facing down has x along +X, y along -Y).
    let belted = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "teeth": 20,
               "belt_width": 6,
               "belt_with": {"pulley": seated, "center_distance": 80, "angle": "90 deg"},
               "operation": "new_body"}),
    )
    .unwrap();
    ok(&doc, belted);
    let (origin, axis) = mount(&doc, belted, "bore");
    assert!(near3(origin, [0.0, -80.0, 27.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    let values = query(&doc, json!({"query": "pulley_values", "uid": belted}));
    assert!(values["belt_length"].as_f64().unwrap() > 160.0, "{values}");

    // A panel's definition: the derived bore before the pulley is made.
    let values = query(
        &doc,
        json!({"query": "pulley_values", "def": {"kind": "toothed", "profile": "gt2_2mm",
               "teeth": 40, "belt_width": 6, "bore_clearance": "0.3 mm",
               "seat": {"shaft": {"body": shaft.body, "edge": shaft.end_edge}, "offset": "1 mm"},
               "operation": "new_body"}}),
    );
    assert!(values.get("error").is_none(), "{values}");
    assert!(
        near(values["bore_diameter"].as_f64().unwrap(), 8.3),
        "{values}"
    );
    assert_eq!(
        values["seat"]["origin"],
        json!([0.0, 0.0, 29.0]),
        "{values}"
    );
    // And the seat alone.
    let frame = query(
        &doc,
        json!({"query": "seat_frame",
               "seat": {"shaft": {"body": shaft.body, "edge": shaft.end_edge}, "offset": 1}}),
    );
    assert_eq!(frame["origin"], json!([0.0, 0.0, 29.0]), "{frame}");
    assert_eq!(frame["diameter"], json!(8.0), "{frame}");
    assert_eq!(frame["hole"], json!(false), "{frame}");
}

#[test]
fn a_pulley_follows_a_shaft_of_another_component() {
    let mut doc = Document::new(MockKernel::default());
    // The shaft in A:1 (O1) at (100, 0, 10): z 10 to 40 in the root.
    doc.create_component(Some("A"), Transform::translation([100.0, 0.0, 10.0]), true)
        .unwrap();
    let shaft = shaft(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    // Picked near the top, in A's coordinates as a pick gives it.
    let seat = |link: Option<Value>| {
        let mut seat = json!({"shaft": {"body": shaft.body, "face": shaft.side},
                              "offset": 2, "near": [4, 0, 29]});
        if let Some(link) = link {
            seat["link"] = link;
        }
        pulley(json!({"seat": seat}))
    };
    // Without the link the shaft is A's, which the root cannot see.
    let message = rejected(&mut doc, seat(None));
    assert!(message.contains("belongs to A"), "{message}");
    doc.undo();

    let seated = add(&mut doc, seat(Some(json!({"source": "O1"})))).unwrap();
    ok(&doc, seated);
    assert_eq!(doc.feature(seated).unwrap().component, ComponentUid::ROOT);
    let (origin, axis) = mount(&doc, seated, "bore");
    assert!(near3(origin, [100.0, 0.0, 38.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    let values = query(&doc, json!({"query": "pulley_values", "uid": seated}));
    assert_eq!(
        values["seat"]["origin"],
        json!([100.0, 0.0, 38.0]),
        "{values}"
    );

    // A moves 50 along y, and its shaft gets thicker: the pulley follows.
    doc.set_occurrence_transform(
        OccurrenceUid(1),
        Transform::translation([100.0, 50.0, 10.0]),
        false,
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "10 mm"}),
    );
    ok(&doc, seated);
    let (origin, _) = mount(&doc, seated, "bore");
    assert!(near3(origin, [100.0, 50.0, 38.0]), "{origin:?}");
    assert!(near(bore(&doc, seated, "pulley_values"), 10.0));
    // The link is kept in the design and listed with the occurrences the
    // pulley refers to.
    let def = query(&doc, json!({"query": "feature", "uid": seated}));
    assert_eq!(def["def"]["seat"]["link"], json!({"source": "O1"}), "{def}");
    let entry = doc.feature(seated).unwrap();
    assert!(
        mitcad_model::internal::joints::referenced_occurrences(&entry.def)
            .contains(&OccurrenceUid(1))
    );
}

#[test]
fn seats_refuse_what_is_not_a_round_shaft() {
    let mut doc = Document::new(MockKernel::default());
    let shaft = shaft(&mut doc);
    let on = |shaft: Value| pulley(json!({"seat": {"shaft": shaft}}));
    // A planar face, a straight edge (the mock's edges are lines unless
    // told otherwise), a datum.
    let message = rejected(
        &mut doc,
        on(json!({"body": shaft.body, "face": shaft.end_face})),
    );
    assert!(
        message.contains(&format!("face {} is not round", shaft.end_face)),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        on(json!({"body": shaft.body, "edge": shaft.end_edge})),
    );
    assert!(
        message.contains(&format!("edge {} is not round (a line)", shaft.end_edge)),
        "{message}"
    );
    let message = rejected(&mut doc, on(json!("xy")));
    assert!(
        message.contains("is not an axis, an edge or a face with an axis"),
        "{message}"
    );
    // A face the shaft no longer has fails with the reason.
    let gone = format!("{}:side(c99)", shaft.body.split('.').next().unwrap());
    let message = rejected(&mut doc, on(json!({"body": shaft.body, "face": gone})));
    assert!(
        message.contains(&format!("seat: body {} has no face {gone}", shaft.body)),
        "{message}"
    );

    // A seat with a plane or a plain bore of its own; a clearance without
    // a seat or on a threaded bore.
    let side = json!({"body": shaft.body, "face": shaft.side});
    let message = rejected(
        &mut doc,
        pulley(json!({"seat": {"shaft": side}, "plane": "xy"})),
    );
    assert!(
        message.contains("takes its plane and center from the shaft"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        pulley(json!({"seat": {"shaft": side}, "bore": {"type": "plain", "diameter": 8}})),
    );
    assert!(
        message.contains("takes its plain bore from the shaft"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        pulley(json!({"plane": "xy", "bore_clearance": 0.2})),
    );
    assert!(
        message.contains("a bore clearance needs a seat"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        pulley(json!({"seat": {"shaft": side}, "bore_clearance": 0.2,
                      "bore": {"type": "thread", "thread": {"designation": "M8x1.25"}}})),
    );
    assert!(
        message.contains("a threaded bore takes no bore clearance"),
        "{message}"
    );
    // A threaded bore on a seat is the thread's tap drill.
    let threaded = add(
        &mut doc,
        pulley(json!({"seat": {"shaft": side},
                      "bore": {"type": "thread", "thread": {"designation": "M8x1.25"}}})),
    )
    .unwrap();
    ok(&doc, threaded);
    // M8x1.25: D1 = 8 - 1.0825 x 1.25.
    let tap = bore(&doc, threaded, "pulley_values");
    assert!((tap - 6.647).abs() < 0.01, "{tap}");
}

/// A spur gear of module 2 and 20 teeth (roots 35 mm across) with `extra`.
fn gear(extra: Value) -> Value {
    let mut value = json!({"type": "gear", "kind": "spur", "module": 2, "teeth": 20,
                           "face_width": 10, "operation": "new_body"});
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

#[test]
fn a_gear_sits_on_a_shaft_and_meshes() {
    let mut doc = Document::new(MockKernel::default());
    let shaft = shaft(&mut doc);
    end_circle(&doc, &shaft, 4.0, 30.0);
    let side = json!({"body": shaft.body, "face": shaft.side});
    let seated = add(
        &mut doc,
        gear(
            json!({"seat": {"shaft": side, "offset": 5, "near": [4, 0, 29]},
                    "bore_clearance": 0.1}),
        ),
    )
    .unwrap();
    ok(&doc, seated);
    let (origin, axis) = mount(&doc, seated, "gear");
    assert!(near3(origin, [0.0, 0.0, 25.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    assert!(near(bore(&doc, seated, "gear_values"), 8.1));
    let values = query(&doc, json!({"query": "gear_values", "uid": seated}));
    assert!(
        near(values["shaft_diameter"].as_f64().unwrap(), 8.0),
        "{values}"
    );

    // On the end edge, flipped: 5 mm up from the bottom.
    let edge = add(
        &mut doc,
        gear(
            json!({"seat": {"shaft": {"body": shaft.body, "edge": shaft.end_edge},
                             "offset": 5, "flip": true}}),
        ),
    )
    .unwrap();
    ok(&doc, edge);
    let (origin, axis) = mount(&doc, edge, "gear");
    assert!(near3(origin, [0.0, 0.0, 5.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");

    // A gear meshing with the seated one: 40 mm along the seat's x axis.
    let meshed = add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "module": 2, "teeth": 20, "face_width": 10,
               "mesh_with": {"gear": seated}, "operation": "new_body"}),
    )
    .unwrap();
    ok(&doc, meshed);
    let (origin, _) = mount(&doc, meshed, "gear");
    assert!(near3(origin, [40.0, 0.0, 25.0]), "{origin:?}");

    // The shaft thicker: the bore follows; too thick for the teeth: refused.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "12 mm"}),
    );
    ok(&doc, seated);
    assert!(near(bore(&doc, seated, "gear_values"), 12.1));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "31 mm"}),
    );
    match status(&doc, seated) {
        FeatureStatus::Failed(message) => {
            assert!(message.contains("the bore reaches the teeth"), "{message}")
        }
        other => panic!("expected a failure, got {other:?}"),
    }
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "shaft_d", "expression": "8 mm"}),
    );

    // A panel's definition with its seat.
    let values = query(
        &doc,
        json!({"query": "gear_values", "def": {"kind": "spur", "module": 2, "teeth": 20,
               "face_width": 10, "seat": {"shaft": side}, "operation": "new_body"}}),
    );
    assert!(values.get("error").is_none(), "{values}");
    assert!(
        near(values["bore_diameter"].as_f64().unwrap(), 8.0),
        "{values}"
    );

    // Ring gears and racks have no bore; a bevel pair is not seated.
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "ring", "module": 2, "teeth": 40, "face_width": 10,
               "rim": 5, "seat": {"shaft": side}, "operation": "new_body"}),
    );
    assert!(message.contains("a ring gear has no bore"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "rack", "module": 2, "teeth": 10, "face_width": 10,
               "height": 10, "seat": {"shaft": side}, "operation": "new_body"}),
    );
    assert!(message.contains("a rack has no bore"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "bevel", "module": 2, "teeth": 20, "face_width": 5,
               "pair": true, "seat": {"shaft": side}, "operation": "new_body"}),
    );
    assert!(
        message.contains("a bevel pair is not placed on a shaft"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        gear(json!({"seat": {"shaft": side}, "mesh_with": {"gear": seated}})),
    );
    assert!(message.contains("not placed by a mesh too"), "{message}");
}
