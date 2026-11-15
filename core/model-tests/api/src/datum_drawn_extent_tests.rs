// SPDX-License-Identifier: MIT
//! Where construction planes and axes are drawn (mitcad#1827, GitHub
//! mitcad/Mitcad#321): the `drawn` extent the `datums` and `datum` queries
//! report, fitted to what each is made from, the resize handles' size over
//! it (mitcad#1748) and the default size with nothing to measure. The mock
//! kernel gives the plate's top face and one edge the geometry the tests
//! set.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, RegionKey};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The `drawn` the `datums` query lists for a datum.
fn drawn(doc: &Document<MockKernel>, uid: &str) -> Value {
    let datums = query(doc, json!({"query": "datums", "origin": true}));
    datums
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["uid"] == uid)
        .unwrap_or_else(|| panic!("{uid} is not in {datums}"))
        .get("drawn")
        .cloned()
        .unwrap_or(Value::Null)
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} is no number"))
}

fn near(actual: &Value, expected: f64) {
    assert!(
        (number(actual) - expected).abs() < 1e-9,
        "{actual} is not {expected}"
    );
}

fn near_point(actual: &Value, expected: [f64; 3]) {
    for (i, expected) in expected.into_iter().enumerate() {
        assert!(
            (number(&actual[i]) - expected).abs() < 1e-9,
            "{actual} is not {expected:?}"
        );
    }
}

/// A plate 10 × 100 × 20 mm (Extrude, F2, body F2.b0) whose top face (at
/// z = 20) has the outline of its sketch's rectangle, and one of its edges
/// a 20 mm line up Z from the origin; its name.
fn plate() -> (Document<MockKernel>, String, String) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 10, "height": 100}),
    );
    let region: RegionKey =
        query(&doc, json!({"query": "sketch", "uid": "F1"}))["regions"][0]["key"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": region.to_string()}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let top = FaceName::end(FeatureUid(2), region).to_string();
    let body: BodyUid = "F2.b0".parse().unwrap();
    let edges: Vec<EdgeName> = doc
        .body_shape(body)
        .expect("the plate")
        .edges
        .iter()
        .map(|(a, b)| EdgeName::new(a.clone(), b.clone()))
        .collect();
    let kernel = doc.kernel();
    kernel.surfaces.borrow_mut().insert(
        top.clone(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 20.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    let corners = [
        [0.0, 0.0, 20.0],
        [10.0, 0.0, 20.0],
        [10.0, 100.0, 20.0],
        [0.0, 100.0, 20.0],
    ];
    let boundary = corners
        .iter()
        .enumerate()
        .map(|(i, start)| {
            (
                edges[i % edges.len()].clone(),
                Curve3::Line {
                    start: *start,
                    end: corners[(i + 1) % corners.len()],
                },
            )
        })
        .collect();
    kernel
        .face_boundaries
        .borrow_mut()
        .insert(top.clone(), boundary);
    let edge = edges[0].to_string();
    kernel.edge_curves.borrow_mut().insert(
        edge.clone(),
        CurveGeometry::Line {
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 20.0],
        },
    );
    (doc, top, edge)
}

fn add(doc: &mut Document<MockKernel>, def: Value) {
    command(doc, json!({"cmd": "add_feature", "def": def}));
}

/// Plane1 (F3) 5 mm above the plate's top, Plane2 (F4) 10 mm above
/// Plane1, Plane3 (F5) 30 mm above XY, Axis1 (F6) along the plate's edge,
/// Axis2 (F7) where XZ and YZ meet.
fn design() -> Document<MockKernel> {
    let (mut doc, top, edge) = plate();
    add(
        &mut doc,
        json!({"type": "construction_plane", "definition": {"type": "offset",
               "plane": {"body": "F2.b0", "face": top}, "distance": 5}}),
    );
    add(
        &mut doc,
        json!({"type": "construction_plane", "definition": {"type": "offset",
               "plane": "F3", "distance": 10}}),
    );
    add(
        &mut doc,
        json!({"type": "construction_plane", "definition": {"type": "offset",
               "plane": "xy", "distance": 30}}),
    );
    add(
        &mut doc,
        json!({"type": "construction_axis", "definition": {"type": "edge",
               "edge": {"body": "F2.b0", "edge": edge}}}),
    );
    add(
        &mut doc,
        json!({"type": "construction_axis", "definition": {"type": "two_planes",
               "plane1": "xz", "plane2": "yz"}}),
    );
    doc
}

/// A rectangle 110 × 20 mm (the face's 100 × 10 with a 5 mm margin all
/// round, 5 % of its longer side) along the face's long edge, centred over
/// the face at `z`.
fn over_the_top_face(drawn: &Value, z: f64) {
    assert_eq!(drawn["source"], "fitted", "{drawn}");
    assert!(drawn.get("size").is_none(), "{drawn}");
    let rect = &drawn["rect"];
    near(&rect["width"], 110.0);
    near(&rect["height"], 20.0);
    near_point(&rect["center"], [5.0, 50.0, z]);
    // Along the long edge (Y), either way.
    near(&json!(number(&rect["x_axis"][1]).abs()), 1.0);
}

#[test]
fn a_plane_on_a_face_is_drawn_over_the_face_and_one_from_it_over_the_same() {
    let doc = design();
    over_the_top_face(&drawn(&doc, "F3"), 25.0);
    // Made from Plane1: over what Plane1 covers, without a second margin.
    over_the_top_face(&drawn(&doc, "F4"), 35.0);
    // The datum query gives it too.
    let datum = query(&doc, json!({"query": "datum", "uid": "F3"}));
    assert_eq!(datum["drawn"], drawn(&doc, "F3"), "{datum}");
}

#[test]
fn an_axis_is_drawn_along_its_edge_and_free_geometry_at_the_default_size() {
    let doc = design();
    // The 20 mm edge with a 3 mm margin (the smallest) at each end.
    let axis = drawn(&doc, "F6");
    assert_eq!(axis["source"], "fitted", "{axis}");
    near(&axis["length"], 26.0);
    near_point(&axis["segment"]["start"], [0.0, 0.0, -3.0]);
    near_point(&axis["segment"]["end"], [0.0, 0.0, 23.0]);
    // Nothing to measure: 100 mm about the origin, as before.
    let plane = drawn(&doc, "F5");
    assert_eq!(plane["source"], "default", "{plane}");
    near(&plane["size"], 100.0);
    near(&plane["rect"]["width"], 100.0);
    near(&plane["rect"]["height"], 100.0);
    near_point(&plane["rect"]["center"], [0.0, 0.0, 30.0]);
    let axis = drawn(&doc, "F7");
    assert_eq!(axis["source"], "default", "{axis}");
    near(&axis["length"], 100.0);
    // The origin's datums are drawn as Preferences says: no extent.
    assert_eq!(drawn(&doc, "xy"), Value::Null);
}

#[test]
fn a_display_size_overrides_the_fitted_extent_and_null_brings_it_back() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F3", "size": 40}),
    );
    let sized = drawn(&doc, "F3");
    assert_eq!(sized["source"], "display_size", "{sized}");
    near(&sized["size"], 40.0);
    near(&sized["rect"]["width"], 40.0);
    near(&sized["rect"]["height"], 40.0);
    // Over the middle of the face, where the fitted one is.
    near_point(&sized["rect"]["center"], [5.0, 50.0, 25.0]);
    // Display only: Plane2, made from Plane1, is drawn as before.
    over_the_top_face(&drawn(&doc, "F4"), 35.0);
    command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F6", "size": 50}),
    );
    let axis = drawn(&doc, "F6");
    assert_eq!(axis["source"], "display_size", "{axis}");
    near_point(&axis["segment"]["start"], [0.0, 0.0, -15.0]);
    near_point(&axis["segment"]["end"], [0.0, 0.0, 35.0]);

    // Saved with the size, opened with the same drawing.
    let saved = doc.to_json();
    let kernel = MockKernel::default();
    *kernel.surfaces.borrow_mut() = doc.kernel().surfaces.borrow().clone();
    *kernel.face_boundaries.borrow_mut() = doc.kernel().face_boundaries.borrow().clone();
    *kernel.edge_curves.borrow_mut() = doc.kernel().edge_curves.borrow().clone();
    let mut loaded = Document::from_json(&saved, kernel).unwrap();
    loaded.recompute();
    assert_eq!(drawn(&loaded, "F3"), sized);
    assert_eq!(drawn(&loaded, "F6"), axis);

    // Reset Size: the fitted extent again.
    command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F3", "size": null}),
    );
    over_the_top_face(&drawn(&doc, "F3"), 25.0);
}

#[test]
fn the_drawn_extent_follows_the_model() {
    let mut doc = design();
    // Plane1 twice as far from the face: still over the face.
    let plane = query(&doc, json!({"query": "feature", "uid": "F3"}));
    let mut def = plane["def"].clone();
    def["definition"]["distance"] = json!(10);
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F3", "def": def}),
    );
    over_the_top_face(&drawn(&doc, "F3"), 30.0);
    over_the_top_face(&drawn(&doc, "F4"), 40.0);
}
