// SPDX-License-Identifier: MIT
//! Section Analysis through a pick (mitcad#1769): a cylindrical face or a
//! circular edge places a plane that contains its axis, turned about that
//! axis by the Angle; a straight edge a plane through its middle, normal
//! to it. The mock kernel gives the picked faces and edges the geometry
//! the tests set.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const HOLE: &str = "F4:side(c1)";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "analyses"}"#).unwrap()).unwrap();
    answer.as_array().unwrap().clone()
}

fn section_plane(doc: &Document<MockKernel>, mut query: Value) -> Result<Value, String> {
    query["query"] = json!("section_plane");
    doc.query(&query.to_string())
        .map(|answer| serde_json::from_str(&answer).unwrap())
        .map_err(|e| e.to_string())
}

fn near(actual: &Value, expected: [f64; 3]) {
    for (i, expected) in expected.into_iter().enumerate() {
        assert!(
            (actual[i].as_f64().unwrap() - expected).abs() < 1e-9,
            "{actual} is not {expected:?}"
        );
    }
}

fn vector(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| value[i].as_f64().unwrap())
}

/// The plane `{origin, normal}` contains the line through `point` along
/// `direction`.
fn contains_line(plane: &Value, point: [f64; 3], direction: [f64; 3]) {
    let origin = vector(&plane["origin"]);
    let normal = vector(&plane["normal"]);
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let away = [0, 1, 2].map(|i| point[i] - origin[i]);
    assert!(dot(away, normal).abs() < 1e-9, "{plane} misses {point:?}");
    assert!(
        dot(direction, normal).abs() < 1e-9,
        "{plane} is not along {direction:?}"
    );
}

/// The model commands of a 20 mm block (F2.b0) with a 6 mm hole along Z
/// through (10, 10) (Extrude2, F4, its round face `F4:side(c1)`).
fn block_with_hole() -> Value {
    json!([
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
          "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_circle", "sketch": "F3", "center": [10, 10], "diameter": 6},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
          "extent": {"type": "through_all"}, "operation": "cut"}}
    ])
}

/// The hole's round face is a cylinder along Z through (10, 10), as the
/// kernel describes it (the axis point nearest the face's centre).
fn hole_is_a_cylinder(doc: &Document<MockKernel>) {
    doc.kernel().surfaces.borrow_mut().insert(
        HOLE.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [10.0, 10.0, 10.0],
            axis: [0.0, 0.0, 1.0],
            radius: 3.0,
        },
    );
}

/// An edge of F2.b0 between a face named `one` and a face whose name
/// starts with `other`, by its name.
fn edge_between(doc: &Document<MockKernel>, one: &str, other: &str) -> String {
    let body: BodyUid = "F2.b0".parse().unwrap();
    let shape = doc.body_shape(body).expect("the block");
    shape
        .edges
        .iter()
        .find(|(a, b)| {
            let (a, b) = (a.to_string(), b.to_string());
            (a == one && b.starts_with(other)) || (b == one && a.starts_with(other))
        })
        .map(|(a, b)| {
            format!("E{{{a}|{b}}}")
                .parse::<EdgeName>()
                .unwrap()
                .to_string()
        })
        .unwrap_or_else(|| panic!("no edge between {one} and {other}*"))
}

fn face(face: &str) -> Value {
    json!({"body": "F2.b0", "face": face})
}

fn edge(edge: &str) -> Value {
    json!({"body": "F2.b0", "edge": edge})
}

#[test]
fn a_hole_s_face_places_a_plane_through_its_axis_turned_by_the_angle() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, block_with_hole());
    hole_is_a_cylinder(&doc);

    // The axis along Z: the turn starts at XZ, the first origin plane
    // that contains it; the normal is XZ's (+Y). The picked axis comes
    // back for the Angle's ring, without an occurrence of its own.
    let plane = section_plane(&doc, json!({"plane": face(HOLE)})).unwrap();
    near(&plane["origin"], [10.0, 10.0, 10.0]);
    near(&plane["normal"], [0.0, 1.0, 0.0]);
    contains_line(&plane, [10.0, 10.0, 0.0], [0.0, 0.0, 1.0]);
    near(&plane["axis"]["origin"], [10.0, 10.0, 10.0]);
    near(&plane["axis"]["direction"], [0.0, 0.0, 1.0]);
    assert!(plane.get("axis_occurrence").is_none(), "{plane}");
    assert_eq!(plane["occurrence"], "");

    // The Angle turns it about the hole's axis without an Axis: a quarter
    // round right-handed about +Z takes +Y to -X; still through the axis.
    let quarter = std::f64::consts::FRAC_PI_2;
    let turned = section_plane(&doc, json!({"plane": face(HOLE), "angle": quarter})).unwrap();
    near(&turned["normal"], [-1.0, 0.0, 0.0]);
    contains_line(&turned, [10.0, 10.0, 0.0], [0.0, 0.0, 1.0]);
    let angle = 30f64.to_radians();
    let (sin, cos) = angle.sin_cos();
    let thirty = section_plane(&doc, json!({"plane": face(HOLE), "angle": angle})).unwrap();
    near(&thirty["normal"], [-sin, cos, 0.0]);
    contains_line(&thirty, [10.0, 10.0, 20.0], [0.0, 0.0, 1.0]);
    // Distance moves it along the turned normal; Flip reverses only the
    // normal.
    let moved = section_plane(
        &doc,
        json!({"plane": face(HOLE), "angle": quarter, "offset": 2, "flip": true}),
    )
    .unwrap();
    near(&moved["origin"], [8.0, 10.0, 10.0]);
    near(&moved["normal"], [1.0, 0.0, 0.0]);
    // An Axis given is used instead of the hole's.
    let about_x = section_plane(
        &doc,
        json!({"plane": face(HOLE), "axis": "x", "angle": quarter}),
    )
    .unwrap();
    near(&about_x["normal"], [0.0, 0.0, 1.0]);
    near(&about_x["axis"]["direction"], [1.0, 0.0, 0.0]);
    assert_eq!(about_x["axis_occurrence"], "");

    // Kept as an analysis: the face as the plane, the angle without an
    // axis; the analyses query gives the turned plane, and a file keeps it.
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": face(HOLE),
               "angle": quarter}}),
    );
    let kept = &analyses(&doc)[0];
    assert_eq!(kept["plane"], face(HOLE));
    assert!(kept.get("axis").is_none(), "{kept}");
    near(&kept["section"]["normal"], [-1.0, 0.0, 0.0]);
    near(&kept["section"]["origin"], [10.0, 10.0, 10.0]);
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": {"type": "section",
               "plane": face(HOLE), "angle": angle}}),
    );
    near(&analyses(&doc)[0]["section"]["normal"], [-sin, cos, 0.0]);
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    hole_is_a_cylinder(&loaded);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
}

#[test]
fn a_circular_edge_places_a_plane_through_the_circle_s_axis() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, block_with_hole());
    // The hole's rim at the top: a circle about (10, 10, 20); its normal
    // as the kernel gives it (down), the axis along its largest component
    // (+Z).
    let rim = edge_between(&doc, HOLE, "F4:end(");
    doc.kernel().edge_curves.borrow_mut().insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [10.0, 10.0, 20.0],
            normal: [0.0, 0.0, -1.0],
            radius: 3.0,
            start: [13.0, 10.0, 20.0],
            end: [13.0, 10.0, 20.0],
        },
    );
    let plane = section_plane(&doc, json!({"plane": edge(&rim)})).unwrap();
    near(&plane["origin"], [10.0, 10.0, 20.0]);
    near(&plane["normal"], [0.0, 1.0, 0.0]);
    contains_line(&plane, [10.0, 10.0, 0.0], [0.0, 0.0, 1.0]);
    near(&plane["axis"]["direction"], [0.0, 0.0, 1.0]);
    let quarter = std::f64::consts::FRAC_PI_2;
    let turned = section_plane(&doc, json!({"plane": edge(&rim), "angle": quarter})).unwrap();
    near(&turned["normal"], [-1.0, 0.0, 0.0]);
    contains_line(&turned, [10.0, 10.0, 0.0], [0.0, 0.0, 1.0]);

    // A circle about an axis along X: the turn starts at XY (the first
    // origin plane that contains X), its normal +Z.
    doc.kernel().edge_curves.borrow_mut().insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [0.0, 10.0, 10.0],
            normal: [1.0, 0.0, 0.0],
            radius: 3.0,
            start: [0.0, 13.0, 10.0],
            end: [0.0, 10.0, 13.0],
        },
    );
    let across = section_plane(&doc, json!({"plane": edge(&rim)})).unwrap();
    near(&across["normal"], [0.0, 0.0, 1.0]);
    contains_line(&across, [5.0, 10.0, 10.0], [1.0, 0.0, 0.0]);
    // An axis askew to all three: the first origin plane whose normal is
    // at least 45 degrees from it (XY's +Z is 54.7 degrees from (1, 1, 1)),
    // made square to the axis.
    let s = 1.0 / 3f64.sqrt();
    doc.kernel().edge_curves.borrow_mut().insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [10.0, 10.0, 10.0],
            normal: [s, s, s],
            radius: 3.0,
            start: [10.0, 10.0, 10.0],
            end: [10.0, 10.0, 10.0],
        },
    );
    let askew = section_plane(&doc, json!({"plane": edge(&rim)})).unwrap();
    let k = 1.0 / 6f64.sqrt();
    near(&askew["normal"], [-k, -k, 2.0 * k]);
    contains_line(&askew, [12.0, 12.0, 12.0], [s, s, s]);
}

#[test]
fn a_straight_edge_places_a_plane_through_its_middle_normal_to_it() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, block_with_hole());
    // The block's front top edge, from (0, 0, 20) to (20, 0, 20).
    let front = edge_between(&doc, "F2:side(c1[c4,c2])", "F2:end(");
    doc.kernel().edge_curves.borrow_mut().insert(
        front.clone(),
        CurveGeometry::Line {
            start: [0.0, 0.0, 20.0],
            end: [20.0, 0.0, 20.0],
        },
    );
    let plane = section_plane(&doc, json!({"plane": edge(&front)})).unwrap();
    near(&plane["origin"], [10.0, 0.0, 20.0]);
    near(&plane["normal"], [1.0, 0.0, 0.0]);
    assert!(plane.get("axis").is_none(), "no axis of its own: {plane}");
    let moved = section_plane(&doc, json!({"plane": edge(&front), "offset": 4})).unwrap();
    near(&moved["origin"], [14.0, 0.0, 20.0]);
    // An angle needs an Axis, as a plane's does; with one it turns.
    let quarter = std::f64::consts::FRAC_PI_2;
    let error = section_plane(&doc, json!({"plane": edge(&front), "angle": quarter})).unwrap_err();
    assert!(error.contains("an angle needs an axis"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": edge(&front),
               "angle": quarter}}),
    );
    assert!(error.contains("an angle needs an axis"), "{error}");
    let turned = section_plane(
        &doc,
        json!({"plane": edge(&front), "axis": "y", "angle": quarter}),
    )
    .unwrap();
    near(&turned["normal"], [0.0, 0.0, -1.0]);
    // A face that is neither planar nor round about an axis (a sphere) is
    // refused, as a plane reference refuses it.
    doc.kernel().surfaces.borrow_mut().insert(
        HOLE.to_owned(),
        SurfaceGeometry::Sphere {
            center: [10.0, 10.0, 10.0],
            radius: 3.0,
        },
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": face(HOLE)}}),
    );
    assert!(error.contains("the face is not planar"), "{error}");
    assert!(analyses(&doc).is_empty(), "nothing refused was added");
}

/// In a component placed twice deep, the hole's plane is worked out in
/// the component's coordinates (its own origin planes) and moved into the
/// design by the occurrence's path.
#[test]
fn a_hole_s_plane_in_an_occurrence_is_in_design_coordinates() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Assembly", "transform": {
                "translation": [100, 20, 30], "rotation": {"axis": [1, 0, 0], "angle": std::f64::consts::FRAC_PI_2}}},
            {"cmd": "create_component", "name": "Block", "transform": {"translation": [5, 7, 11]}}
        ]),
    );
    command(&mut doc, block_with_hole());
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    hole_is_a_cylinder(&doc);
    // (10, 10, 10) in Block is (15, 17, 21) in Assembly, which turns it
    // a quarter round about X and moves it: (115, -1, 47). The component's
    // Z (the hole's axis) is the design's -Y, its +Y (XZ's normal) the
    // design's +Z.
    let plane = section_plane(&doc, json!({"plane": face(HOLE), "occurrence": "O1/O2"})).unwrap();
    near(&plane["origin"], [115.0, -1.0, 47.0]);
    near(&plane["normal"], [0.0, 0.0, 1.0]);
    near(&plane["axis"]["origin"], [115.0, -1.0, 47.0]);
    near(&plane["axis"]["direction"], [0.0, -1.0, 0.0]);
    assert_eq!(plane["occurrence"], "O1/O2");
    // Turned a quarter round about the design's -Y: +Z goes to -X.
    let quarter = std::f64::consts::FRAC_PI_2;
    let turned = section_plane(
        &doc,
        json!({"plane": face(HOLE), "occurrence": "O1/O2", "angle": quarter}),
    )
    .unwrap();
    near(&turned["normal"], [-1.0, 0.0, 0.0]);
    contains_line(&turned, [115.0, 0.0, 47.0], [0.0, 1.0, 0.0]);
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": face(HOLE),
               "occurrence": "O1/O2", "offset": 5}}),
    );
    let kept = &analyses(&doc)[0];
    assert_eq!(kept["occurrence"], "O1/O2");
    near(&kept["section"]["origin"], [115.0, -1.0, 52.0]);
}

/// The hole deleted: analyses through its face and its rim cannot find
/// their planes, say why, and stay; undone, they find them again.
#[test]
fn a_section_through_a_removed_hole_cannot_be_found() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, block_with_hole());
    hole_is_a_cylinder(&doc);
    let rim = edge_between(&doc, HOLE, "F4:end(");
    doc.kernel().edge_curves.borrow_mut().insert(
        rim.clone(),
        CurveGeometry::Circle {
            center: [10.0, 10.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            radius: 3.0,
            start: [13.0, 10.0, 20.0],
            end: [13.0, 10.0, 20.0],
        },
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "name": "Bore", "def": {"type": "section", "plane": face(HOLE)}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "name": "Rim", "def": {"type": "section", "plane": edge(&rim)}}),
    );
    assert!(analyses(&doc).iter().all(|a| a.get("section").is_some()));

    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F4"}));
    // The mock describes any face it was given a surface for; the kernel
    // finds no face of the hole once the hole is gone.
    let kept = doc.kernel().surfaces.borrow_mut().remove(HOLE);
    let gone = analyses(&doc);
    assert_eq!(gone.len(), 2, "the analyses stay");
    for analysis in &gone {
        assert!(analysis.get("section").is_none(), "{analysis}");
        assert!(
            analysis["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{analysis}"
        );
    }
    // Neither can be kept as it is.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Rim", "def": {"type": "section",
               "plane": edge(&rim), "offset": 1}}),
    );
    assert!(error.contains("the section's plane"), "{error}");

    command(&mut doc, json!({"cmd": "undo"}));
    doc.kernel()
        .surfaces
        .borrow_mut()
        .insert(HOLE.to_owned(), kept.expect("the hole's surface"));
    assert!(analyses(&doc).iter().all(|a| a.get("section").is_some()));
}
