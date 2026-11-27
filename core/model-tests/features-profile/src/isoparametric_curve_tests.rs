// SPDX-License-Identifier: MIT
//! Isoparametric Curve (mitcad#2522) with the mock kernel: on a cylindrical
//! face, a u curve at a position is a line along the axis and a v curve a
//! circle of its radius; several are evenly spaced and drawn apart; a curve
//! through a picked point; the curve follows the face when the block grows;
//! one curve drives a Pipe; the definition's refusals; a face of a
//! component placed by a moved and turned occurrence, linked (mitcad#2554),
//! the clicked point moved with it; Reorient Design turns an At Point
//! curve's point with the design (mitcad#2571). The geometry is
//! checked with OCCT by `geometry/tests/test_iso_curve.cpp`
//! (`geometry.iso_curve`) and `tools/cli/tests/isoparametric_curve.json`
//! (`cli.isoparametric_curve`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::reorient::{Turn, TurnAxis};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::TopoName;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let result = doc
        .command(&value.to_string())
        .unwrap_or_else(|e| panic!("{value}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// The refusal of a command.
fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    match doc.command(&value.to_string()) {
        Ok(result) => panic!("{value} was not refused: {result}"),
        Err(error) => error.to_string(),
    }
}

fn uid(name: &str) -> FeatureUid {
    name.parse().unwrap()
}

fn status(doc: &Document<MockKernel>, feature: &str) -> FeatureStatus {
    doc.status(uid(feature)).cloned().expect("recomputed")
}

fn add(doc: &mut Document<MockKernel>, def: Value) -> String {
    command(doc, json!({"cmd": "add_feature", "def": def}))["uid"]
        .as_str()
        .expect("a uid")
        .to_owned()
}

fn specs(doc: &Document<MockKernel>, prefix: &str) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(prefix))
        .cloned()
        .collect()
}

/// The block's side face over c1, which the tests make a cylinder.
fn side() -> String {
    "F2:side(c1[c4,c2])"
        .parse::<TopoName>()
        .unwrap()
        .to_string()
}

/// Sketch1 (F1) and a 60 x 40 block 20 high (Extrude1, F2, body F2.b0,
/// its height the parameter d3) whose side face over c1 is a cylinder of
/// radius 10 about the Z axis.
fn cylinder_block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60,
               "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    doc.kernel().surfaces.borrow_mut().insert(
        side(),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 10.0],
            axis: [0.0, 0.0, 1.0],
            radius: 10.0,
        },
    );
    doc
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= 1e-6)
}

/// The ends of a line.
fn line_ends(curve: &Curve3) -> ([f64; 3], [f64; 3]) {
    let Curve3::Line { start, end } = curve else {
        panic!("not a line: {curve:?}");
    };
    (*start, *end)
}

fn feature_curves(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "feature_curves"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn a_u_curve_of_a_cylinder_is_a_line_along_its_axis() {
    let mut doc = cylinder_block();
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": {"body": "F2.b0", "face": side()},
               "position": 0.25}),
    );
    assert_eq!(iso, "F3");
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert_eq!(
        called.last(),
        Some(&format!("iso_curves {} u at 0.25", side())),
        "{called:?}"
    );
    let curve = doc.feature_curve(uid("F3")).expect("a curve");
    assert_eq!(curve.len(), 1);
    assert_eq!(curve[0].name, "F3:curve1");
    // A quarter turn round from X: at +Y, from the bottom to the top.
    let (start, end) = line_ends(&curve[0].curve);
    assert!(near(start, [0.0, 10.0, 0.0]), "{start:?}");
    assert!(near(end, [0.0, 10.0, 20.0]), "{end:?}");
    // Construction geometry: the feature_curves query lists it, one
    // polyline.
    let curves = feature_curves(&doc);
    assert_eq!(curves[0]["name"], "IsoparametricCurve1", "{curves}");
    assert!(curves[0].get("polylines").is_none(), "{curves}");

    // A Pipe along it.
    let pipe = add(
        &mut doc,
        json!({"type": "pipe", "path": {"feature": "F3"}, "size": 2, "operation": "new_body"}),
    );
    assert_eq!(status(&doc, &pipe), FeatureStatus::Ok);
    let piped = specs(&doc, &format!("pipe {pipe} "));
    assert!(
        piped.last().is_some_and(|s| s.contains("path F3:curve1 ")),
        "{piped:?}"
    );
}

#[test]
fn a_v_curve_of_a_cylinder_is_a_circle_of_its_radius() {
    let mut doc = cylinder_block();
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": {"body": "F2.b0", "face": side()},
               "direction": "v", "position": 0.5}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let curve = doc.feature_curve(uid(&iso)).expect("a curve");
    assert_eq!(curve.len(), 1);
    let Curve3::Conic {
        center,
        normal,
        major,
        minor,
        closed,
        ..
    } = curve[0].curve
    else {
        panic!("a circle: {:?}", curve[0].curve);
    };
    assert!(near(center, [0.0, 0.0, 10.0]), "{center:?}");
    assert!(near(normal, [0.0, 0.0, 1.0]), "{normal:?}");
    assert!((major - 10.0).abs() <= 1e-9 && (minor - 10.0).abs() <= 1e-9);
    assert!(closed);
}

#[test]
fn several_curves_are_evenly_spaced_and_drawn_apart() {
    let mut doc = cylinder_block();
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": {"body": "F2.b0", "face": side()},
               "direction": "v", "count": 3}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert_eq!(
        called.last(),
        Some(&format!("iso_curves {} v at 0.25,0.5,0.75", side())),
        "{called:?}"
    );
    let curve = doc.feature_curve(uid(&iso)).expect("a curve");
    let names: Vec<&str> = curve.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["F3:curve1", "F3:curve2", "F3:curve3"]);
    for (piece, z) in curve.iter().zip([5.0, 10.0, 15.0]) {
        let Curve3::Conic { center, .. } = piece.curve else {
            panic!("a circle: {:?}", piece.curve);
        };
        assert!(near(center, [0.0, 0.0, z]), "{center:?}");
    }
    // Three circles apart: three polylines, each at its own height.
    let curves = feature_curves(&doc);
    let polylines = curves[0]["polylines"].as_array().expect("polylines");
    assert_eq!(polylines.len(), 3, "{curves}");
    for (polyline, z) in polylines.iter().zip([5.0, 10.0, 15.0]) {
        for point in polyline.as_array().unwrap() {
            assert!((point[2].as_f64().unwrap() - z).abs() <= 1e-6, "{polyline}");
        }
    }
}

#[test]
fn a_curve_through_a_picked_point() {
    let mut doc = cylinder_block();
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": {"body": "F2.b0", "face": side()},
               "point": [0, -20, 5]}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert!(
        called.last().is_some_and(|s| s.contains(" u through ")),
        "{called:?}"
    );
    // Three quarters of a turn round from X: at -Y.
    let curve = doc.feature_curve(uid(&iso)).expect("a curve");
    let (start, end) = line_ends(&curve[0].curve);
    assert!(near(start, [0.0, -10.0, 0.0]), "{start:?}");
    assert!(near(end, [0.0, -10.0, 20.0]), "{end:?}");
}

#[test]
fn the_curve_follows_the_face() {
    let mut doc = cylinder_block();
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": {"body": "F2.b0", "face": side()},
               "position": 0.5}),
    );
    let (_, end) = line_ends(&doc.feature_curve(uid(&iso)).expect("a curve")[0].curve);
    assert!(near(end, [-10.0, 0.0, 20.0]), "{end:?}");
    // The block 35 high: the line runs up to its new top.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 35}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let (start, end) = line_ends(&doc.feature_curve(uid(&iso)).expect("a curve")[0].curve);
    assert!(near(start, [-10.0, 0.0, 0.0]), "{start:?}");
    assert!(near(end, [-10.0, 0.0, 35.0]), "{end:?}");
}

#[test]
fn the_definition_is_checked() {
    let mut doc = cylinder_block();
    let face = json!({"body": "F2.b0", "face": side()});
    let add_def = |def: Value| json!({"cmd": "add_feature", "def": def});
    let outside = refused(
        &mut doc,
        add_def(json!({"type": "isoparametric_curve", "face": face, "position": 1.5})),
    );
    assert!(outside.contains("from 0 to 1"), "{outside}");
    let none = refused(
        &mut doc,
        add_def(json!({"type": "isoparametric_curve", "face": face, "count": 0})),
    );
    assert!(none.contains("whole number from 1"), "{none}");
    let part = refused(
        &mut doc,
        add_def(json!({"type": "isoparametric_curve", "face": face, "count": 2.5})),
    );
    assert!(part.contains("whole number from 1"), "{part}");
    let both = refused(
        &mut doc,
        add_def(
            json!({"type": "isoparametric_curve", "face": face, "position": 0.5,
                       "count": 2}),
        ),
    );
    assert!(
        both.contains("one of a position, a point or a count"),
        "{both}"
    );
    let neither = refused(
        &mut doc,
        add_def(json!({"type": "isoparametric_curve", "face": face})),
    );
    assert!(
        neither.contains("one of a position, a point or a count"),
        "{neither}"
    );
    let axis = refused(
        &mut doc,
        add_def(json!({"type": "isoparametric_curve", "face": "z", "position": 0.5})),
    );
    assert!(axis.contains("a face"), "{axis}");
    // The direction is left out at u, kept at v.
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": face, "direction": "v", "count": 2}),
    );
    let def: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature", "uid": iso}).to_string())
            .unwrap(),
    )
    .unwrap();
    let text = def.to_string();
    assert!(text.contains("\"direction\":\"v\""), "{text}");
    assert!(text.contains("\"count\""), "{text}");
}

/// A placement turned `degrees` about the Z axis through the origin, then
/// moved by `offset`.
fn placed(offset: [f64; 3], degrees: f64) -> Transform {
    let turn = Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], degrees.to_radians()).unwrap();
    Transform::translation(offset).after(&turn)
}

/// The block's side face as a cylinder of radius 10 about the block's Z
/// axis, where `placement` shows it. The mock kernel's surfaces do not
/// move with a moved shape: the cylinder is put where the real kernel's
/// moved face is.
fn cylinder_at(doc: &Document<MockKernel>, face: &str, placement: &Transform) {
    doc.kernel().surfaces.borrow_mut().insert(
        face.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: placement.apply_point([0.0, 0.0, 10.0]),
            axis: placement.apply_vector([0.0, 0.0, 1.0]),
            radius: 10.0,
        },
    );
}

/// A face of a component placed by an occurrence (mitcad#2554): Drum
/// (Drum:1) turned 30 degrees about Z and moved 50 mm along X holds the
/// block; the root's curve takes its side face, linked from Drum:1. The
/// point clicked on the face is in Drum's coordinates, where the view's
/// click gives it, and the model moves it with the face: the curve runs
/// through the clicked place, also after Drum:1 moves and turns. Evenly
/// Spaced and At Position take no point and are unaffected. The real
/// kernel's geometry: `cli.isoparametric_curve`.
#[test]
fn a_curve_on_a_face_of_a_placed_component_runs_through_the_clicked_place() {
    let mut doc = Document::new(MockKernel::default());
    let first = placed([50.0, 0.0, 0.0], 30.0);
    let (drum, occurrence) = doc.create_component(Some("Drum"), first, true).unwrap();
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .expect("a sketch")
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [0, 0], "width": 60,
               "height": 40}),
    );
    let extrude = add(
        &mut doc,
        json!({"type": "extrude",
               "profiles": [{"sketch": sketch, "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
    );
    assert_eq!(doc.feature(uid(&extrude)).unwrap().component, drum);
    let body = format!("{extrude}.b0");
    let face = format!("{extrude}:side(c1[c4,c2])")
        .parse::<TopoName>()
        .unwrap()
        .to_string();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    cylinder_at(&doc, &face, &first);

    // Clicked a quarter turn round from Drum's X axis, 5 up: (0, 10, 5) in
    // Drum, (45, 8.660254037844386, 5) in the design.
    let clicked = [0.0, 10.0, 5.0];
    let on_face = json!({"body": body, "face": face});

    // Without the link the face is Drum's, which the root's curve cannot
    // read.
    let unlinked = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": on_face, "point": clicked}),
    );
    let error = status(&doc, &unlinked);
    assert!(
        error.error().is_some_and(|e| e.contains("belongs to Drum")),
        "{error:?}"
    );
    doc.undo();

    let link = json!({body.clone(): {"source": occurrence.to_string()}});
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": on_face, "point": clicked,
               "links": link}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert_eq!(
        called.last(),
        Some(&format!(
            "iso_curves {face} u through {:?}",
            ["45", "8.660254038", "5"]
        )),
        "{called:?}"
    );
    let ends = |doc: &Document<MockKernel>| {
        line_ends(&doc.feature_curve(uid(&iso)).expect("a curve")[0].curve)
    };
    let (start, end) = ends(&doc);
    assert!(near(start, [45.0, 8.660254037844386, 0.0]), "{start:?}");
    assert!(near(end, [45.0, 8.660254037844386, 20.0]), "{end:?}");
    let seen = first.apply_point(clicked);
    assert!(near([start[0], start[1], seen[2]], seen), "{start:?}");

    // Drum:1 moves to (20, 30, 5) and turns 90 degrees: the clicked place
    // is at (10, 30, 10) in the design, and the curve follows the face.
    let second = placed([20.0, 30.0, 5.0], 90.0);
    cylinder_at(&doc, &face, &second);
    doc.set_occurrence_transform(occurrence, second, false)
        .unwrap();
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert_eq!(
        called.last(),
        Some(&format!(
            "iso_curves {face} u through {:?}",
            ["10", "30", "10"]
        )),
        "{called:?}"
    );
    let (start, end) = ends(&doc);
    assert!(near(start, [10.0, 30.0, 5.0]), "{start:?}");
    assert!(near(end, [10.0, 30.0, 25.0]), "{end:?}");

    // Evenly Spaced and At Position take no point: the face's own range.
    let evenly = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": on_face, "direction": "v",
               "count": 3, "links": link}),
    );
    assert_eq!(status(&doc, &evenly), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert_eq!(
        called.last(),
        Some(&format!("iso_curves {face} v at 0.25,0.5,0.75")),
        "{called:?}"
    );
    let curve = doc.feature_curve(uid(&evenly)).expect("a curve");
    for (piece, z) in curve.iter().zip([10.0, 15.0, 20.0]) {
        let Curve3::Conic { center, .. } = piece.curve else {
            panic!("a circle: {:?}", piece.curve);
        };
        assert!(near(center, [20.0, 30.0, z]), "{center:?}");
    }
    let at = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": on_face, "position": 0.5,
               "links": link}),
    );
    assert_eq!(status(&doc, &at), FeatureStatus::Ok);
    let called = specs(&doc, "iso_curves ");
    assert_eq!(
        called.last(),
        Some(&format!("iso_curves {face} u at 0.5")),
        "{called:?}"
    );

    // The design file keeps the link and the point as given.
    let def: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature", "uid": iso}).to_string())
            .unwrap(),
    )
    .unwrap();
    let text = def.to_string();
    assert!(text.contains(&format!("\"links\":{link}")), "{text}");
    assert!(text.contains("\"point\":[0.0,10.0,5.0]"), "{text}");
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    cylinder_at(&loaded, &face, &second);
    loaded.recompute();
    assert_eq!(status(&loaded, &iso), FeatureStatus::Ok);
    let (start, _) = ends(&loaded);
    assert!(near(start, [10.0, 30.0, 5.0]), "{start:?}");

    // Only the face's body is linked.
    let other = refused(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "isoparametric_curve", "face": on_face,
               "position": 0.5, "links": {"F99.b0": {"source": occurrence.to_string()}}}}),
    );
    assert!(other.contains("not the one of its face"), "{other}");
}

/// The point an At Point curve keeps, as its definition has it.
fn kept_point(doc: &Document<MockKernel>, feature: &str) -> Option<[f64; 3]> {
    match &doc.feature(uid(feature)).expect("the curve").def {
        FeatureDef::IsoparametricCurve(iso) => iso.point,
        other => panic!("not an isoparametric curve: {other:?}"),
    }
}

/// Reorient Design (mitcad#2571): an At Point curve on the cylinder,
/// through a point clicked at -Y, turned 90 degrees about X with the
/// design. Its point turns with the design (it is in the coordinates of the
/// face's body, which turn), so the u line runs through the same place on
/// the turned face: the old line turned. Undo restores the point and the
/// line.
#[test]
fn reorient_design_turns_the_point_of_an_at_point_curve() {
    let mut doc = cylinder_block();
    let iso = add(
        &mut doc,
        json!({"type": "isoparametric_curve", "face": {"body": "F2.b0", "face": side()},
               "point": [0, -20, 5]}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let ends = |doc: &Document<MockKernel>| {
        line_ends(&doc.feature_curve(uid(&iso)).expect("a curve")[0].curve)
    };
    let (start, end) = ends(&doc);
    assert!(near(start, [0.0, -10.0, 0.0]), "{start:?}");
    assert!(near(end, [0.0, -10.0, 20.0]), "{end:?}");

    // The mock kernel's surfaces do not turn with the body: the cylinder is
    // put where the real kernel's turned face is.
    let turn = Turn::about(TurnAxis::X, 1);
    let original = SurfaceGeometry::Cylinder {
        origin: [0.0, 0.0, 10.0],
        axis: [0.0, 0.0, 1.0],
        radius: 10.0,
    };
    doc.kernel().surfaces.borrow_mut().insert(
        side(),
        SurfaceGeometry::Cylinder {
            origin: turn.vector([0.0, 0.0, 10.0]),
            axis: turn.vector([0.0, 0.0, 1.0]),
            radius: 10.0,
        },
    );
    command(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    assert_eq!(kept_point(&doc, &iso), Some([0.0, -5.0, -20.0]));
    let (turned_start, turned_end) = ends(&doc);
    assert!(
        near(turned_start, turn.vector(start)),
        "{turned_start:?} is not {:?}",
        turn.vector(start)
    );
    assert!(
        near(turned_end, turn.vector(end)),
        "{turned_end:?} is not {:?}",
        turn.vector(end)
    );

    doc.kernel().surfaces.borrow_mut().insert(side(), original);
    let undone = command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undone["label"], "Reorient Design");
    assert_eq!(kept_point(&doc, &iso), Some([0.0, -20.0, 5.0]));
    assert_eq!(status(&doc, &iso), FeatureStatus::Ok);
    let (start_again, end_again) = ends(&doc);
    assert!(near(start_again, start), "{start_again:?}");
    assert!(near(end_again, end), "{end_again:?}");
}
