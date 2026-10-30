// SPDX-License-Identifier: MIT
//! Measure in the design's coordinates (mitcad#646), on the mock kernel
//! (`MockKernel::measures_selections`: the closest points of the
//! selections' boxes): each selection is placed by its occurrence, given
//! or the owner's only one, through nested occurrences; bodies and datums
//! of the root are measured as before; and two parallel planes (planar
//! faces, datum planes, either way round) also report their offset. Axes
//! (mitcad#647): cylinders, a cone, circles and construction axes report
//! their axes, the angles between them and planes or straight edges, and
//! the axes' distances, also of parts placed as occurrences. Selections
//! (mitcad#985): errors list the paths that place a component placed
//! several times, occurrence paths by names, origin datums by name, and
//! the entity form `ui_state` lists.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model_api::{AnalysisApi, DocumentApi};

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// The measure query's answer.
fn measure(doc: &Document<MockKernel>, a: Value, b: Value) -> Value {
    let query = json!({"query": "measure", "a": a, "b": b});
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// What `between` says of two selections.
fn between(doc: &Document<MockKernel>, a: Value, b: Value) -> Value {
    measure(doc, a, b)["between"].clone()
}

/// The error of a measure query.
fn refused(doc: &Document<MockKernel>, a: Value, b: Value) -> String {
    let query = json!({"query": "measure", "a": a, "b": b});
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn near(actual: &Value, expected: f64) {
    let value = actual
        .as_f64()
        .unwrap_or_else(|| panic!("{actual} is no number"));
    assert!(
        (value - expected).abs() < 1e-6,
        "{value} instead of {expected}"
    );
}

fn near_point(actual: &Value, expected: [f64; 3]) {
    for (i, expected) in expected.into_iter().enumerate() {
        assert!(
            (actual[i].as_f64().unwrap() - expected).abs() < 1e-6,
            "{actual} instead of {expected:?}"
        );
    }
}

/// The distance between two points.
fn length(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt()
}

/// A 20 mm block of a 20 x 20 rectangle on XY, extruded by `height`, in
/// the active component (sketch `sketch`, the extrusion the next feature).
fn block(sketch: &str, height: f64) -> Value {
    json!([
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [0, 0], "width": 20, "height": 20},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
            "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
    ])
}

fn face(body: &str, face: &str) -> Value {
    json!({"body": body, "face": face})
}

fn face_in(body: &str, face: &str, occurrence: &str) -> Value {
    json!({"body": body, "face": face, "occurrence": occurrence})
}

/// A root block F2.b0 (0 to 20 on each axis) and a Block (F4.b0, the same
/// size, with the construction plane F5 on its top) inside an Assembly:
/// the Assembly at (100, 20, 30) turned 90 degrees about Z, the Block at
/// (5, 7, 11) in it. In the design the Block spans x 73 to 93, y 25 to
/// 45 and z 41 to 61.
fn nested() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(&mut doc, block("F1", 20.0));
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Assembly", "transform": {
                "translation": [100, 20, 30], "rotation": {"axis": [0, 0, 1], "angle": FRAC_PI_2}}},
            {"cmd": "create_component", "name": "Block", "transform": {"translation": [5, 7, 11]}},
        ]),
    );
    command(&mut doc, block("F3", 20.0));
    command(
        &mut doc,
        json!([
            {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
                "type": "offset", "plane": "xy", "distance": 20}}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    doc
}

#[test]
fn measure_places_each_selection_by_its_occurrence() {
    let mut doc = nested();
    let top = face("F2.b0", &format!("F2:end({R})"));
    let block_top = format!("F4:end({R})");
    // The Block's top in the design, 41 above the root block's.
    let placed = between(&doc, top.clone(), face_in("F4.b0", &block_top, "O1/O2"));
    near_point(&placed["on_a"], [20.0, 20.0, 20.0]);
    near_point(&placed["on_b"], [73.0, 25.0, 61.0]);
    near(
        &placed["distance"],
        length([20.0, 20.0, 20.0], [73.0, 25.0, 61.0]),
    );
    near(&placed["angle"], 0.0);
    near(&placed["offset"], 41.0);
    // Its only occurrence when none is given.
    assert_eq!(
        between(&doc, top.clone(), face("F4.b0", &block_top)),
        placed
    );
    // As text too.
    let text = doc
        .analysis(
            &json!({"query": "measure", "a": top, "b": face_in("F4.b0", &block_top, "O1/O2")})
                .to_string(),
            false,
        )
        .unwrap();
    assert!(
        text.contains("from [20.000, 20.000, 20.000] to [73.000, 25.000, 61.000]"),
        "{text}"
    );
    assert!(text.contains("Offset: 41.000000 mm"), "{text}");
    // A datum of the Block is placed likewise.
    let datum = between(
        &doc,
        top.clone(),
        json!({"datum": "F5", "occurrence": "O1/O2"}),
    );
    near(&datum["offset"], 41.0);
    near_point(&datum["on_b"], [10.0, 10.0, 61.0]);
    // An occurrence that does not place the body.
    let wrong = refused(&doc, top.clone(), face_in("F4.b0", &block_top, "O1"));
    assert!(wrong.contains("does not place body F4.b0"), "{wrong}");

    // The root's bodies and datums are measured as they are: no copy moved.
    let moved = doc.kernel().count("transform");
    let root = between(&doc, top.clone(), json!({"datum": "xy"}));
    near_point(&root["on_a"], [10.0, 10.0, 20.0]);
    near_point(&root["on_b"], [10.0, 10.0, 0.0]);
    near(&root["distance"], 20.0);
    near(&root["offset"], 20.0);
    let single = measure(&doc, json!({"body": "F2.b0"}), Value::Null);
    assert_eq!(single["a"]["kind"], "body", "{single}");
    assert_eq!(doc.kernel().count("transform"), moved);

    // A second Assembly, at (200, 0, 0) turned half a turn about Z: the
    // Block in it (x 5 to 25, y 7 to 27 in the Assembly) spans x 175 to 195,
    // y -27 to -7 and z 11 to 31.
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Assembly:1", "transform": {
            "translation": [200, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": PI}}}),
    );
    let ambiguous = refused(&doc, top.clone(), face("F4.b0", &block_top));
    assert!(ambiguous.contains("placed 2 times"), "{ambiguous}");
    let datum = refused(&doc, top.clone(), json!({"datum": "F5"}));
    assert!(datum.contains("placed 2 times"), "{datum}");
    let repeated = between(&doc, top.clone(), face_in("F4.b0", &block_top, "O3/O2"));
    near_point(&repeated["on_a"], [20.0, 0.0, 20.0]);
    near_point(&repeated["on_b"], [175.0, -7.0, 31.0]);
    near(&repeated["offset"], 11.0);
    let first = between(&doc, top.clone(), face_in("F4.b0", &block_top, "O1/O2"));
    assert_eq!(first, placed, "the first occurrence stays where it was");
    near(
        &between(&doc, top, json!({"datum": "F5", "occurrence": "O3/O2"}))["offset"],
        11.0,
    );
}

#[test]
fn optional_face_range_preserves_minimum_when_the_kernel_has_no_exact_maximum() {
    let doc = nested();
    let a = face("F2.b0", &format!("F2:end({R})"));
    let b = face_in("F4.b0", &format!("F4:end({R})"), "O1/O2");
    let ordinary = measure(&doc, a.clone(), b.clone());
    assert!(ordinary["between"].get("maximum_distance").is_none());
    let query = json!({"query": "measure", "a": a, "b": b, "face_range": true});
    let answer: Value = serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap();
    assert_eq!(
        answer["between"]["distance"],
        ordinary["between"]["distance"]
    );
    assert!(answer["between"]["maximum_distance"].is_null());
    for invalid in [
        json!({"query": "measure", "a": {"body": "F2.b0"}, "b": b, "face_range": true}),
        json!({"query": "measure", "a": a, "face_range": true}),
    ] {
        let error = doc.query(&invalid.to_string()).unwrap_err().to_string();
        assert!(error.contains("face_range needs two faces"), "{error}");
    }
}

/// A root block F2.b0 (0 to 20 on each axis, the construction plane F6 at
/// z = 7) and a Plate (F4.b0, 20 x 20 x 10, its construction plane F5 at
/// z = 3) placed twice: at (5, 5, 50) and at (100, 0, 50).
fn plates() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(&mut doc, block("F1", 20.0));
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate", "transform": {"translation": [5, 5, 50]}}),
    );
    command(&mut doc, block("F3", 10.0));
    command(
        &mut doc,
        json!([
            {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
                "type": "offset", "plane": "xy", "distance": 3}}},
            {"cmd": "activate_component", "component": "C0"},
            {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
                "type": "offset", "plane": "xy", "distance": 7}}},
            {"cmd": "copy_occurrence", "occurrence": "Plate:1", "transform": {
                "translation": [100, 0, 50]}},
        ]),
    );
    doc
}

#[test]
fn parallel_planes_report_their_offset() {
    let doc = plates();
    let top = face("F2.b0", &format!("F2:end({R})"));
    let bottom = format!("F4:start({R})");
    // Overlapping faces, their normals opposite: the offset is the
    // closest distance.
    let over = between(&doc, top.clone(), face_in("F4.b0", &bottom, "O1"));
    near_point(&over["on_a"], [12.5, 12.5, 20.0]);
    near_point(&over["on_b"], [12.5, 12.5, 50.0]);
    near(&over["distance"], 30.0);
    near(&over["angle"], PI);
    near(&over["offset"], 30.0);
    // Faces that do not overlap: the closest distance is longer.
    let apart = between(&doc, top.clone(), face_in("F4.b0", &bottom, "O2"));
    near_point(&apart["on_a"], [20.0, 10.0, 20.0]);
    near_point(&apart["on_b"], [100.0, 10.0, 50.0]);
    near(
        &apart["distance"],
        length([20.0, 10.0, 20.0], [100.0, 10.0, 50.0]),
    );
    near(&apart["offset"], 30.0);
    // Faces of two occurrences of one component.
    let both = between(
        &doc,
        face_in("F4.b0", &bottom, "O1"),
        face_in("F4.b0", &format!("F4:end({R})"), "O2"),
    );
    near_point(&both["on_a"], [25.0, 12.5, 50.0]);
    near_point(&both["on_b"], [100.0, 12.5, 60.0]);
    near(&both["offset"], 10.0);
    // A face against construction planes: the root's, a Plate's in each of
    // its occurrences, and the origin's.
    near(
        &between(&doc, top.clone(), json!({"datum": "F6"}))["offset"],
        13.0,
    );
    near(
        &between(
            &doc,
            top.clone(),
            json!({"datum": "F5", "occurrence": "O1"}),
        )["offset"],
        33.0,
    );
    near(
        &between(
            &doc,
            json!({"datum": "F5", "occurrence": "O2"}),
            top.clone(),
        )["offset"],
        33.0,
    );
    near(
        &between(
            &doc,
            json!({"datum": "F6"}),
            json!({"datum": "F5", "occurrence": "O1"}),
        )["offset"],
        46.0,
    );
    near(&between(&doc, top.clone(), json!("xy"))["offset"], 20.0);

    // No offset for planes that are not parallel, nor for a body.
    let side = doc
        .body_shape("F2.b0".parse::<BodyUid>().unwrap())
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == "side")
        .unwrap()
        .to_string();
    let across = between(&doc, top.clone(), face("F2.b0", &side));
    near(&across["angle"], FRAC_PI_2);
    assert!(across.get("offset").is_none(), "{across}");
    let upright = between(&doc, top.clone(), json!({"datum": "yz"}));
    assert!(upright.get("offset").is_none(), "{upright}");
    let body = between(&doc, top, json!({"body": "F4.b0", "occurrence": "O1"}));
    near(&body["distance"], 30.0);
    assert!(body.get("offset").is_none(), "{body}");
}

// Axes (mitcad#647): the mock kernel describes the faces and edges of the
// picks (`MockKernel::surfaces`, `edge_curves`) in their bodies'
// coordinates; the model places them by their occurrences and works out
// the angles and the axes' distances.

fn cylinder(center: [f64; 2]) -> Value {
    json!({"cmd": "add_feature", "def": {"type": "cylinder", "plane": {"type": "origin", "plane": "xy"},
        "center": center, "diameter": 10, "height": 30, "operation": "new_body"}})
}

fn surface(doc: &Document<MockKernel>, face: &str, geometry: SurfaceGeometry) {
    doc.kernel()
        .surfaces
        .borrow_mut()
        .insert(face.to_owned(), geometry);
}

fn curve(doc: &Document<MockKernel>, edge: &str, geometry: CurveGeometry) {
    let name = edge.parse::<EdgeName>().unwrap().to_string();
    doc.kernel().edge_curves.borrow_mut().insert(name, geometry);
}

/// A root cylinder (F1.b0, its round face's axis along Z through
/// (-30, 10)); a Pin component placed at (50, 0, 0) turned 90 degrees about
/// X, with a cylinder (F2.b0) whose axis is along Z through the Pin's
/// origin: in the design along -Y through (50, y, 0); a root block (F4.b0,
/// 0 to 20 on each axis, its front face on y = 0); and a third round face
/// (F5.b0) that the mock describes as a cone about Z through (0, 0, 100).
fn axes() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(&mut doc, cylinder([-30.0, 10.0]));
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Pin", "transform": {
            "translation": [50, 0, 0], "rotation": {"axis": [1, 0, 0], "angle": FRAC_PI_2}}}),
    );
    command(&mut doc, cylinder([0.0, 0.0]));
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(&mut doc, block("F3", 20.0));
    command(&mut doc, cylinder([0.0, 0.0]));
    surface(
        &doc,
        "F1:side0",
        SurfaceGeometry::Cylinder {
            origin: [-30.0, 10.0, 15.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    surface(
        &doc,
        "F2:side0",
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 15.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    surface(
        &doc,
        "F5:side0",
        SurfaceGeometry::Cone {
            origin: [0.0, 0.0, 100.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
            half_angle: 0.3,
        },
    );
    doc
}

#[test]
fn cylinders_report_their_axes_in_the_design() {
    let mut doc = axes();
    // The root cylinder's axis, and the Pin's turned by its occurrence.
    let root = measure(&doc, face("F1.b0", "F1:side0"), Value::Null);
    near_point(&root["a"]["axis"]["origin"], [-30.0, 10.0, 15.0]);
    near_point(&root["a"]["axis"]["direction"], [0.0, 0.0, 1.0]);
    let pin = measure(&doc, face("F2.b0", "F2:side0"), Value::Null);
    near_point(&pin["a"]["axis"]["origin"], [50.0, -15.0, 0.0]);
    near_point(&pin["a"]["axis"]["direction"], [0.0, -1.0, 0.0]);
    // A planar face has no axis.
    let top = face("F4.b0", &format!("F4:end({R})"));
    let plane = measure(&doc, top, Value::Null);
    assert!(plane["a"].get("axis").is_none(), "{plane}");

    // Skew axes: the common perpendicular from (-30, 10, 0) on the root's
    // axis to (50, 10, 0) on the Pin's.
    let skew = between(&doc, face("F1.b0", "F1:side0"), face("F2.b0", "F2:side0"));
    near(&skew["angle"], FRAC_PI_2);
    near(&skew["axis_distance"], 80.0);
    near_point(&skew["axis_points"][0], [-30.0, 10.0, 0.0]);
    near_point(&skew["axis_points"][1], [50.0, 10.0, 0.0]);
    assert!(skew.get("center_distance").is_none(), "{skew}");

    // The Pin placed again, upright at (0, 50, 0): its axis along Z
    // through (0, 50), parallel to the root's and 50 from it, the
    // distance of their centres across the axes.
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Pin:1", "transform": {
            "translation": [0, 50, 0]}}),
    );
    let parallel = between(
        &doc,
        face("F1.b0", "F1:side0"),
        face_in("F2.b0", "F2:side0", "O2"),
    );
    near(&parallel["angle"], 0.0);
    near(&parallel["axis_distance"], 50.0);
    near_point(&parallel["axis_points"][0], [-30.0, 10.0, 15.0]);
    near_point(&parallel["axis_points"][1], [0.0, 50.0, 15.0]);
    // The first occurrence stays where it was.
    let first = between(
        &doc,
        face("F1.b0", "F1:side0"),
        face_in("F2.b0", "F2:side0", "O1"),
    );
    assert_eq!(first, skew);
    // The two occurrences of one cylinder: the upright one's axis and the
    // turned one's, skew, 50 apart along X.
    let both = between(
        &doc,
        face_in("F2.b0", "F2:side0", "O2"),
        face_in("F2.b0", "F2:side0", "O1"),
    );
    near(&both["angle"], FRAC_PI_2);
    near(&both["axis_distance"], 50.0);
    near_point(&both["axis_points"][0], [0.0, 50.0, 0.0]);
    near_point(&both["axis_points"][1], [50.0, 50.0, 0.0]);
}

#[test]
fn axes_against_planes_edges_and_construction_axes() {
    let doc = axes();
    let round = face("F1.b0", "F1:side0");
    // A cylinder against the block's top: square to it, no axis distance.
    let top = between(&doc, round.clone(), face("F4.b0", &format!("F4:end({R})")));
    near(&top["angle"], FRAC_PI_2);
    assert!(top.get("axis_distance").is_none(), "{top}");
    assert!(top.get("offset").is_none(), "{top}");
    // Against its front face (y = 0): parallel, the axis 10 from it.
    let front = between(&doc, face("F4.b0", "F4:side(c1[c4,c2])"), round.clone());
    near(&front["angle"], 0.0);
    near(&front["axis_distance"], 10.0);
    near_point(&front["axis_points"][0], [-30.0, 0.0, 15.0]);
    near_point(&front["axis_points"][1], [-30.0, 10.0, 15.0]);

    // The cone's axis against a straight edge of the block rising at 45
    // degrees in the plane x = 20.
    let edge = format!("E{{F4:end({R})|F4:side(c2[c1,c3])}}");
    curve(
        &doc,
        &edge,
        CurveGeometry::Line {
            start: [20.0, 0.0, 0.0],
            end: [20.0, 20.0, 20.0],
        },
    );
    let cone = between(
        &doc,
        face("F5.b0", "F5:side0"),
        json!({"body": "F4.b0", "edge": edge}),
    );
    near(&cone["angle"], FRAC_PI_4);
    near(&cone["axis_distance"], 20.0);
    near_point(&cone["axis_points"][0], [0.0, 0.0, 0.0]);
    near_point(&cone["axis_points"][1], [20.0, 0.0, 0.0]);
    let alone = measure(&doc, face("F5.b0", "F5:side0"), Value::Null);
    near_point(&alone["a"]["axis"]["origin"], [0.0, 0.0, 100.0]);

    // A circle edge (the root cylinder's top rim) against the origin's X
    // and Z axes.
    let rim = "E{F1:top|F1:side0}";
    curve(
        &doc,
        rim,
        CurveGeometry::Circle {
            center: [-30.0, 10.0, 30.0],
            normal: [0.0, 0.0, 1.0],
            radius: 5.0,
            start: [-25.0, 10.0, 30.0],
            end: [-25.0, 10.0, 30.0],
        },
    );
    let circle = json!({"body": "F1.b0", "edge": rim});
    let rim_alone = measure(&doc, circle.clone(), Value::Null);
    near_point(&rim_alone["a"]["axis"]["origin"], [-30.0, 10.0, 30.0]);
    near_point(&rim_alone["a"]["axis"]["direction"], [0.0, 0.0, 1.0]);
    let x = between(&doc, circle.clone(), json!({"datum": "x"}));
    near(&x["angle"], FRAC_PI_2);
    near(&x["axis_distance"], 10.0);
    near_point(&x["axis_points"][0], [-30.0, 10.0, 0.0]);
    near_point(&x["axis_points"][1], [-30.0, 0.0, 0.0]);
    assert!(x.get("center_distance").is_none(), "{x}");
    let z = between(&doc, json!({"datum": "z"}), circle.clone());
    near(&z["angle"], 0.0);
    near(&z["axis_distance"], length([0.0; 3], [-30.0, 10.0, 0.0]));
    let datum = measure(&doc, json!({"datum": "z"}), Value::Null);
    near_point(&datum["a"]["axis"]["direction"], [0.0, 0.0, 1.0]);

    // Two circles: their centres' distance beside the axes'.
    let bottom = "E{F1:bottom|F1:side0}";
    curve(
        &doc,
        bottom,
        CurveGeometry::Circle {
            center: [-30.0, 10.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            radius: 5.0,
            start: [-25.0, 10.0, 0.0],
            end: [-25.0, 10.0, 0.0],
        },
    );
    let rims = between(&doc, circle, json!({"body": "F1.b0", "edge": bottom}));
    near(&rims["center_distance"], 30.0);
    near(&rims["axis_distance"], 0.0);
    near(&rims["angle"], 0.0);

    // As text.
    let text = doc
        .analysis(
            &json!({"query": "measure", "a": round, "b": {"datum": "x"}}).to_string(),
            false,
        )
        .unwrap();
    assert!(
        text.contains("axis through [-30.000, 10.000, 15.000] along [0.000, 0.000, 1.000]"),
        "{text}"
    );
    assert!(text.contains("Axis distance: 10.000000 mm"), "{text}");
}

/// A face of a body placed by an occurrence is measured on the body as it
/// is, its results moved into the design (mitcad#981): no moved copy of the
/// whole body (the selection card measures every face selected, and a copy
/// of a body with modelled threads took most of a second). Two faces of
/// one occurrence are measured between in the body's coordinates, their
/// points then placed; faces of two placements still each placed.
#[test]
fn a_placed_face_is_measured_without_a_moved_copy_of_its_body() {
    let doc = nested();
    let copies = doc.kernel().count("transform");
    let block_top = face_in("F4.b0", &format!("F4:end({R})"), "O1/O2");
    let block_bottom = face_in("F4.b0", &format!("F4:start({R})"), "O1/O2");

    let single = measure(&doc, block_top.clone(), Value::Null);
    assert_eq!(single["a"]["kind"], "face", "{single}");
    near(&single["a"]["plane"]["origin"][2], 61.0);
    near_point(&single["a"]["plane"]["normal"], [0.0, 0.0, 1.0]);
    assert_eq!(doc.kernel().count("transform"), copies, "no moved copy");

    // Top and bottom of the Block, which spans x 73 to 93, y 25 to 45 and
    // z 41 to 61 in the design.
    let own = between(&doc, block_top.clone(), block_bottom);
    near_point(&own["on_a"], [83.0, 35.0, 61.0]);
    near_point(&own["on_b"], [83.0, 35.0, 41.0]);
    near(&own["distance"], 20.0);
    near(&own["offset"], 20.0);
    assert_eq!(doc.kernel().count("transform"), copies, "no moved copies");

    // The root's top and the Block's: placed apart, measured placed.
    let apart = between(&doc, face("F2.b0", &format!("F2:end({R})")), block_top);
    near_point(&apart["on_b"], [73.0, 25.0, 61.0]);
    assert!(doc.kernel().count("transform") > copies);
}

/// [`nested`] with the Block placed three times in the Assembly
/// (mitcad#985): Block:1 (O1/O2) at (5, 7, 11), Block:2 (O1/O3) at
/// (5, 47, 11) and Block:3 (O1/O4) at (5, 87, 11), all in the Assembly's
/// coordinates. Block:2's origin is at (53, 25, 41) in the design, its x
/// axis along the design's +Y.
fn three_blocks() -> Document<MockKernel> {
    let mut doc = nested();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C1"}),
    );
    for (y, uid, name) in [(47, "O3", "Block:2"), (87, "O4", "Block:3")] {
        let copy = command(
            &mut doc,
            json!({"cmd": "copy_occurrence", "occurrence": "O1/O2",
                   "transform": {"translation": [5, y, 11]}}),
        );
        assert_eq!(copy["occurrence"], uid, "{copy}");
        assert_eq!(copy["name"], name, "{copy}");
    }
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    doc
}

/// Where the Block of [`three_blocks`] is placed, as errors list it.
const BLOCK_PATHS: &str = "O1/O2 (Assembly:1/Block:1), O1/O3 (Assembly:1/Block:2), \
                           O1/O4 (Assembly:1/Block:3)";

/// The error of measuring one selection.
fn refused_alone(doc: &Document<MockKernel>, a: Value) -> String {
    refused(doc, a, Value::Null)
}

#[test]
fn errors_name_the_occurrences_that_place_a_selection() {
    let mut doc = three_blocks();
    let block_top = format!("F4:end({R})");
    // No occurrence for a component placed three times: the three paths
    // with their names.
    let datum = refused_alone(&doc, json!({"datum": "F5"}));
    assert!(
        datum.contains(&format!(
            "is in a component placed 3 times: {BLOCK_PATHS}; give one as occurrence"
        )),
        "{datum}"
    );
    let body = refused_alone(&doc, face("F4.b0", &block_top));
    assert!(
        body.contains(&format!(
            "body F4.b0 is in a component placed 3 times: {BLOCK_PATHS}"
        )),
        "{body}"
    );
    // A path that does not place it, one through the wrong parent and an
    // occurrence that does not exist: each names the paths that do.
    for (occurrence, wrong) in [
        ("O1", "occurrence 'O1' does not place"),
        ("O3", "Block:2 is placed in Assembly, not in"),
        ("O1/O9", "occurrence O9 does not exist"),
    ] {
        let error = refused_alone(&doc, json!({"datum": "F5", "occurrence": occurrence}));
        assert!(error.contains(wrong), "{occurrence}: {error}");
        assert!(
            error.contains(&format!("is placed at {BLOCK_PATHS}")),
            "{occurrence}: {error}"
        );
        let error = refused_alone(&doc, face_in("F4.b0", &block_top, occurrence));
        assert!(error.contains(wrong), "{occurrence}: {error}");
        assert!(
            error.contains(&format!("body F4.b0 is placed at {BLOCK_PATHS}")),
            "{occurrence}: {error}"
        );
    }
    // A root body given an occurrence.
    let root = refused_alone(&doc, face_in("F2.b0", &format!("F2:end({R})"), "O1"));
    assert!(root.contains("does not place body F2.b0"), "{root}");
    assert!(root.contains("body F2.b0 is in the root"), "{root}");

    // At most ten paths, then how many more.
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C1"}),
    );
    for i in 0..9 {
        command(
            &mut doc,
            json!({"cmd": "copy_occurrence", "occurrence": "O1/O2",
                   "transform": {"translation": [5, 127 + 40 * i, 11]}}),
        );
    }
    let many = refused_alone(&doc, json!({"datum": "F5"}));
    assert!(many.contains("placed 12 times"), "{many}");
    assert!(
        many.contains("… and 2 more; the instances query lists them"),
        "{many}"
    );
    assert!(many.contains("O1/O11 (Assembly:1/Block:10)"), "{many}");
    assert!(!many.contains("Block:11"), "{many}");
}

#[test]
fn occurrence_paths_are_taken_by_names() {
    let doc = three_blocks();
    let top = face("F2.b0", &format!("F2:end({R})"));
    let block_top = format!("F4:end({R})");
    let by_ids = measure(&doc, top.clone(), face_in("F4.b0", &block_top, "O1/O3"));
    assert_eq!(by_ids["b"]["occurrence"], "O1/O3", "{by_ids}");
    // By names as the components query gives them, in any case, a
    // component's name alone where it is placed once, and mixed with ids:
    // each answered as the ids.
    for path in [
        "Assembly:1/Block:2",
        "assembly:1/block:2",
        "Assembly/Block:2",
        "O1/Block:2",
        " Assembly:1 / Block:2 ",
    ] {
        let by_names = measure(&doc, top.clone(), face_in("F4.b0", &block_top, path));
        assert_eq!(by_names, by_ids, "{path}");
    }
    let datum = measure(
        &doc,
        json!({"datum": "F5", "occurrence": "Assembly:1/Block:3"}),
        Value::Null,
    );
    assert_eq!(datum["a"]["occurrence"], "O1/O4", "{datum}");
    assert_eq!(
        datum,
        measure(
            &doc,
            json!({"datum": "F5", "occurrence": "O1/O4"}),
            Value::Null
        )
    );
    // An unknown name: the occurrences where the path has got to.
    let unknown = refused_alone(
        &doc,
        json!({"datum": "F5", "occurrence": "Assembly:1/Block:9"}),
    );
    assert!(
        unknown.contains(&format!(
            "there is no occurrence 'Block:9' in Assembly; its occurrences are {BLOCK_PATHS}"
        )),
        "{unknown}"
    );
    let top_level = refused_alone(
        &doc,
        json!({"datum": "F5", "occurrence": "Frame:1/Block:1"}),
    );
    assert!(
        top_level.contains("there is no occurrence 'Frame:1'"),
        "{top_level}"
    );
    assert!(top_level.contains("O1 (Assembly:1)"), "{top_level}");
    // An ambiguous one (the component placed three times there): the
    // candidates.
    let ambiguous = refused_alone(
        &doc,
        json!({"datum": "F5", "occurrence": "Assembly:1/Block"}),
    );
    assert!(
        ambiguous.contains(&format!("'Block' in Assembly is any of {BLOCK_PATHS}")),
        "{ambiguous}"
    );
}

#[test]
fn origin_datums_are_taken_by_name() {
    let doc = three_blocks();
    // In the root, in any case, as fields or as text.
    for xy in [json!({"datum": "xy"}), json!({"datum": "XY"}), json!("XY")] {
        let answer = measure(&doc, xy, Value::Null);
        near_point(&answer["a"]["plane"]["origin"], [0.0, 0.0, 0.0]);
        near_point(&answer["a"]["plane"]["normal"], [0.0, 0.0, 1.0]);
    }
    for x in [json!({"datum": "x"}), json!({"datum": "X"}), json!("X")] {
        let answer = measure(&doc, x, Value::Null);
        near_point(&answer["a"]["axis"]["origin"], [0.0, 0.0, 0.0]);
        near_point(&answer["a"]["axis"]["direction"], [1.0, 0.0, 0.0]);
    }
    // In an occurrence: that occurrence's origin, the path by ids or names.
    let xy = measure(
        &doc,
        json!({"datum": "XY", "occurrence": "O1/O3"}),
        Value::Null,
    );
    near_point(&xy["a"]["plane"]["origin"], [53.0, 25.0, 41.0]);
    near_point(&xy["a"]["plane"]["normal"], [0.0, 0.0, 1.0]);
    assert_eq!(xy["a"]["occurrence"], "O1/O3", "{xy}");
    assert_eq!(
        xy,
        measure(
            &doc,
            json!({"datum": "xy", "occurrence": "Assembly:1/Block:2"}),
            Value::Null
        )
    );
    let x = measure(
        &doc,
        json!({"datum": "x", "occurrence": "Assembly:1/Block:2"}),
        Value::Null,
    );
    near_point(&x["a"]["axis"]["origin"], [53.0, 25.0, 41.0]);
    near_point(&x["a"]["axis"]["direction"], [0.0, 1.0, 0.0]);
    assert_eq!(x["a"]["occurrence"], "O1/O3", "{x}");
    for id in ["Origin", "YZ", "Xz", "Z"] {
        measure(
            &doc,
            json!({"datum": id, "occurrence": "O1/O3"}),
            Value::Null,
        );
    }
    // Neither a construction feature nor an origin datum.
    let wrong = refused_alone(&doc, json!({"datum": "top", "occurrence": "O1/O3"}));
    assert!(wrong.contains("invalid datum 'top'"), "{wrong}");
    assert!(wrong.contains("\"xy\""), "{wrong}");
}

#[test]
fn the_entity_form_measures_as_the_fields() {
    let doc = three_blocks();
    let top = face("F2.b0", &format!("F2:end({R})"));
    let block_top = format!("F4:end({R})");
    // As ui_state lists the selection, passed on unchanged.
    let entity = json!({"kind": "face", "owner": "F4.b0", "name": block_top,
        "occurrence": "O1/O3", "geometry": "plane",
        "reference": {"body": "F4.b0", "face": block_top}, "text": "Face of F4.b0"});
    assert_eq!(
        measure(&doc, top.clone(), entity),
        measure(&doc, top.clone(), face_in("F4.b0", &block_top, "O1/O3"))
    );
    for (entity, fields) in [
        (
            json!({"kind": "body", "owner": "F4.b0", "name": "", "occurrence": "O1/O4"}),
            json!({"body": "F4.b0", "occurrence": "O1/O4"}),
        ),
        (
            json!({"kind": "plane", "owner": "F5", "name": "", "occurrence": "O1/O2"}),
            json!({"datum": "F5", "occurrence": "O1/O2"}),
        ),
        (
            json!({"kind": "point", "owner": "origin", "name": "", "occurrence": "O1/O3"}),
            json!({"datum": "origin", "occurrence": "O1/O3"}),
        ),
        (
            json!({"kind": "axis", "owner": "z", "name": "", "occurrence": "Assembly:1/Block:3"}),
            json!({"datum": "z", "occurrence": "O1/O4"}),
        ),
        (
            json!({"kind": "face", "owner": "F2.b0", "name": format!("F2:end({R})"), "occurrence": ""}),
            json!({"body": "F2.b0", "face": format!("F2:end({R})"), "occurrence": ""}),
        ),
    ] {
        assert_eq!(
            measure(&doc, top.clone(), entity.clone()),
            measure(&doc, top.clone(), fields),
            "{entity}"
        );
    }
    let point = measure(
        &doc,
        json!({"kind": "point", "owner": "origin", "name": "", "occurrence": "O1/O3"}),
        Value::Null,
    );
    assert_eq!(point["a"]["occurrence"], "O1/O3", "{point}");
    // The entity form's errors.
    let sketch = refused_alone(&doc, json!({"kind": "sketch", "owner": "F3", "name": ""}));
    assert!(
        sketch.contains("a sketch entity cannot be measured"),
        "{sketch}"
    );
    let both = refused_alone(
        &doc,
        json!({"kind": "body", "owner": "F4.b0", "body": "F4.b0", "occurrence": "O1/O2"}),
    );
    assert!(both.contains("not both"), "{both}");
    let named = refused_alone(
        &doc,
        json!({"kind": "edge", "owner": "F4.b0", "name": block_top, "occurrence": "O1/O2"}),
    );
    assert!(named.contains("is no edge name of body F4.b0"), "{named}");
    // Placed three times without an occurrence: the same error as fields.
    let placed = refused_alone(&doc, json!({"kind": "plane", "owner": "F5", "name": ""}));
    assert!(placed.contains(BLOCK_PATHS), "{placed}");
}
