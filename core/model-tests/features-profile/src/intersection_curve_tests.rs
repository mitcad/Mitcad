// SPDX-License-Identifier: MIT
//! Intersection Curve (mitcad#1405) with the mock kernel: the curve of an
//! arc on XZ and a line on XY, a circle swept along it, the curve and the
//! sweep following a dimension of each sketch, construction along it, and
//! the refusals (parallel planes, a branching result, stretches apart, no
//! crossing, one sketch twice, a path naming another feature). Since
//! mitcad#1417 also ribs and webs grown from it, construction along a 1 m
//! curve within 1e-6 mm, and the curve taken in another component through
//! an occurrence (a construction point, a sketch's section); since
//! mitcad#1496 the curve in the preview's report.

use serde_json::{Value, json};

use mitcad_model::datum::Datum;
use mitcad_model::ids::ComponentUid;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn text(value: &Value) -> String {
    value.as_str().expect("a text").to_owned()
}

fn uid(name: &str) -> FeatureUid {
    name.parse().unwrap()
}

fn status(doc: &Document<MockKernel>, feature: &str) -> FeatureStatus {
    doc.status(uid(feature)).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, feature: &str) -> String {
    match status(doc, feature) {
        FeatureStatus::Failed(message) => message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn add(doc: &mut Document<MockKernel>, def: Value) -> String {
    text(&command(doc, json!({"cmd": "add_feature", "def": def}))["uid"])
}

fn calls(doc: &Document<MockKernel>, op: &str) -> usize {
    doc.kernel().calls.borrow().get(op).copied().unwrap_or(0)
}

/// The ends of the curve's pieces (the mock's lines), in order.
fn curve_points(doc: &Document<MockKernel>, feature: &str) -> Vec<[f64; 3]> {
    let curve = doc.feature_curve(uid(feature)).expect("a curve");
    let mut points = Vec::new();
    for piece in curve.iter() {
        match &piece.curve {
            Curve3::Line { start, end } => {
                points.push(*start);
                points.push(*end);
            }
            other => panic!("the mock makes lines, not {other:?}"),
        }
    }
    points
}

/// Sketch1 (F1) on XZ: an arc of radius 20 (dimension `radius`) about a
/// fixed point at the origin, from (20, 0) round to (-20, 0); Sketch2 (F2)
/// on XY: a horizontal line from (-30, 10) to (30, 10), 10 (dimension
/// `offset`) from a fixed point at the origin; IntersectionCurve1 (F3) of
/// the arc and the line: the arc lifted to y = 10.
struct Design {
    doc: Document<MockKernel>,
    radius: String,
    offset: String,
}

fn intersection(first: &str, first_curve: &str, second: &str, second_curve: &str) -> Value {
    json!({"type": "intersection_curve",
           "first": {"sketch": first, "curves": [first_curve]},
           "second": {"sketch": second, "curves": [second_curve]}})
}

fn sketches(doc: &mut Document<MockKernel>) -> (String, String, String, String) {
    command(doc, json!({"cmd": "sketch.create", "plane": "xz"}));
    let centre = command(
        doc,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let centre = text(&centre["entities"][0]);
    let arc = command(
        doc,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": centre,
               "start": [20, 0], "end": [-20, 0]}),
    );
    let arc = text(&arc["made"][0]);
    let radius = command(
        doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "radius", "curve": arc}, "value": 20}),
    );
    let radius = text(&radius["dimensions"][0]);
    command(doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    let origin = command(
        doc,
        json!({"cmd": "sketch.add_point", "sketch": "F2", "at": [0, 0], "fixed": true}),
    );
    let origin = text(&origin["entities"][0]);
    let line = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": "F2", "start": [-30, 10], "end": [30, 10]}),
    );
    let line = text(&line["made"][0]);
    command(
        doc,
        json!({"cmd": "sketch.add_constraint", "sketch": "F2",
               "constraint": {"type": "horizontal", "line": line}}),
    );
    let offset = command(
        doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F2",
               "dimension": {"type": "point_line_distance", "point": origin, "line": line},
               "value": 10}),
    );
    let offset = text(&offset["dimensions"][0]);
    (arc, line, radius, offset)
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    let (arc, line, radius, offset) = sketches(&mut doc);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &line));
    assert_eq!(curve, "F3");
    Design {
        doc,
        radius,
        offset,
    }
}

/// Every point of the curve is `y` from XZ and `r` from the Y axis.
fn assert_on(doc: &Document<MockKernel>, y: f64, r: f64) {
    let points = curve_points(doc, "F3");
    assert!(points.len() >= 60, "{} points", points.len());
    for p in points {
        assert!((p[1].abs() - y).abs() < 1e-6, "{p:?} is not {y} from XZ");
        assert!(
            (p[0].hypot(p[2]) - r).abs() < 1e-6,
            "{p:?} is not {r} from the Y axis"
        );
    }
}

#[test]
fn a_circle_swept_along_the_curve_of_an_arc_and_a_line_follows_both_sketches() {
    let Design {
        mut doc,
        radius,
        offset,
    } = design();
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    assert_on(&doc, 10.0, 20.0);
    // The kernel got the arc along XZ's normal and the line along XY's.
    assert!(
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .any(|s| s.starts_with("intersection_curves c")
                && s.contains(" with c")
                && s.ends_with("along 0 0 1")),
        "{:?}",
        doc.kernel().specs.borrow()
    );
    // Its pieces in order along it, named after the feature.
    let names: Vec<String> = doc
        .feature_curve(uid("F3"))
        .unwrap()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names[0], "F3:curve1");
    assert_eq!(names[1], "F3:curve2");

    // Sketch3 (F4) on YZ: a 4 mm circle; Sweep1 (F5) along the curve.
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F4", "center": [20, 10], "diameter": 4}),
    );
    let region = text(&circle["region"]);
    let sweep = add(
        &mut doc,
        json!({"type": "sweep", "profiles": [{"sketch": "F4", "region": region}],
               "path": {"feature": "F3"}, "operation": "new_body"}),
    );
    assert_eq!(sweep, "F5");
    assert_eq!(status(&doc, "F5"), FeatureStatus::Ok);
    let swept = doc
        .kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|s| s.starts_with("sweep F5 "))
        .cloned()
        .expect("the sweep reached the kernel");
    assert!(
        swept.starts_with("sweep F5 path F3:curve1,F3:curve2,"),
        "{swept}"
    );

    // A dimension of the arc's sketch: the curve and the sweep follow.
    let sweeps = calls(&doc, "sweep");
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": radius,
               "value": "25 mm"}),
    );
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    assert_on(&doc, 10.0, 25.0);
    assert_eq!(status(&doc, "F5"), FeatureStatus::Ok);
    assert!(calls(&doc, "sweep") > sweeps, "the sweep was made again");

    // A dimension of the line's sketch: the same.
    let sweeps = calls(&doc, "sweep");
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F2", "dimension": offset,
               "value": "15 mm"}),
    );
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    assert_on(&doc, 15.0, 25.0);
    assert_eq!(status(&doc, "F5"), FeatureStatus::Ok);
    assert!(calls(&doc, "sweep") > sweeps, "the sweep was made again");

    // The design file keeps the feature and the path that names it.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let features = file["features"].as_array().unwrap();
    assert!(features.iter().any(|f| f["type"] == "intersection_curve"));
    assert!(
        features
            .iter()
            .any(|f| f["type"] == "sweep" && f["path"] == json!({"feature": "F3"}))
    );
    // (Reading evaluates nothing: recompute builds it.)
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(status(&reopened, "F3"), FeatureStatus::Ok);
    assert_on(&reopened, 15.0, 25.0);
    assert_eq!(status(&reopened, "F5"), FeatureStatus::Ok);
}

#[test]
fn construction_along_the_curve() {
    let Design { mut doc, .. } = design();
    // Halfway along the half circle: its top, 20 from the Y axis.
    let point = add(
        &mut doc,
        json!({"type": "construction_point", "definition": {"type": "along_path",
               "path": {"feature": "F3"}, "distance": 0.5}}),
    );
    let Some(Datum::Point(point)) = doc.datum(uid(&point)) else {
        panic!("a point");
    };
    let p = point.point;
    assert!(p[0].abs() < 1e-4, "{p:?}");
    assert!((p[1] - 10.0).abs() < 1e-9, "{p:?}");
    assert!((p[2].abs() - 20.0).abs() < 1e-4, "{p:?}");
    // A plane across it there: its normal along the curve, along X.
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane", "definition": {"type": "along_path",
               "path": {"feature": "F3"}, "distance": 0.5}}),
    );
    let Some(Datum::Plane(plane)) = doc.datum(uid(&plane)) else {
        panic!("a plane");
    };
    let n = plane.normal();
    assert!((n[0].abs() - 1.0).abs() < 1e-2, "{n:?}");
}

#[test]
fn sketches_in_parallel_planes_are_refused() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    let a = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let a = text(&a["made"][0]);
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    let b = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F2", "start": [5, -5], "end": [5, 5]}),
    );
    let b = text(&b["made"][0]);
    let curve = add(&mut doc, intersection("F1", &a, "F2", &b));
    let message = error(&doc, &curve);
    assert!(message.contains("lie in parallel planes"), "{message}");
    assert_eq!(calls(&doc, "intersection_curves"), 0);
}

#[test]
fn a_branching_crossing_is_refused_with_where_it_branches() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, line, _, _) = sketches(&mut doc);
    // Four pieces meeting at one point, as two crossing curves come.
    let spoke = |end: [f64; 3]| Curve3::Line {
        start: [0.0, 10.0, 20.0],
        end,
    };
    *doc.kernel().crossing.borrow_mut() = Some(vec![
        spoke([10.0, 10.0, 20.0]),
        spoke([-10.0, 10.0, 20.0]),
        spoke([0.0, 10.0, 30.0]),
        spoke([0.0, 10.0, 10.0]),
    ]);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &line));
    let message = error(&doc, &curve);
    assert!(
        message.contains("the curve branches at (0.000, 10.000, 20.000)"),
        "{message}"
    );
}

#[test]
fn a_crossing_in_stretches_apart_is_refused_with_each_stretch() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, line, _, _) = sketches(&mut doc);
    *doc.kernel().crossing.borrow_mut() = Some(vec![
        Curve3::Line {
            start: [0.0, 0.0, 0.0],
            end: [10.0, 0.0, 0.0],
        },
        Curve3::Line {
            start: [0.0, 5.0, 0.0],
            end: [10.0, 5.0, 0.0],
        },
    ]);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &line));
    let message = error(&doc, &curve);
    assert!(message.contains("2 separate stretches"), "{message}");
    assert!(
        message.contains("from (0.000, 5.000, 0.000) to (10.000, 5.000, 0.000)"),
        "{message}"
    );
}

#[test]
fn curves_that_do_not_cross_are_refused() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, _, _, _) = sketches(&mut doc);
    // A line on XY far beyond the arc's reach.
    let far = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F2", "start": [100, -5], "end": [130, -5]}),
    );
    let far = text(&far["made"][0]);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &far));
    let message = error(&doc, &curve);
    assert!(message.contains("do not cross"), "{message}");
}

#[test]
fn one_sketch_twice_and_paths_of_other_features_are_refused() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, _, _, _) = sketches(&mut doc);
    let def = intersection("F1", &arc, "F1", &arc);
    let refused = doc
        .command(&json!({"cmd": "add_feature", "def": def}).to_string())
        .unwrap_err();
    assert!(
        refused.0.contains("both chains are curves of"),
        "{}",
        refused.0
    );

    let profile = command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    let profile = text(&profile["uid"]);
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": profile, "center": [0, 0], "diameter": 4}),
    );
    let region = text(&circle["region"]);
    let sweep = json!({"type": "sweep", "profiles": [{"sketch": profile, "region": region}],
                       "path": {"feature": "F1"}, "operation": "new_body"});
    let refused = doc
        .command(&json!({"cmd": "add_feature", "def": sweep}).to_string())
        .unwrap_err();
    assert!(refused.0.contains("makes no curve"), "{}", refused.0);
}

/// Where the point `id` of the sketch `uid` is, in the sketch's plane.
fn point_at(doc: &Document<MockKernel>, uid: &str, id: &str) -> [f64; 2] {
    let s: Value = serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    let e = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone();
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

#[test]
fn the_curve_is_projected_into_later_sketches_and_followed() {
    let Design {
        mut doc, offset, ..
    } = design();
    // Sketch3 (F4) on YZ, x = 0: where the curve crosses it, the top of the
    // arc, 20 from the Y axis and 10 from XZ (YZ: x = -Z, y = +Y).
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    let crossed = command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": "F4", "source": {"feature": "F3"},
               "linked": true, "section": true}),
    );
    let point = crossed["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|e| e.starts_with('p'))
        .expect("a point")
        .to_owned();
    let p = point_at(&doc, "F4", &point);
    assert!((p[0].abs() - 20.0).abs() < 1e-4, "{p:?}");
    assert!((p[1] - 10.0).abs() < 1e-6, "{p:?}");
    // The line's sketch moved: the projected point follows the curve.
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F2", "dimension": offset,
               "value": "15 mm"}),
    );
    assert_eq!(status(&doc, "F4"), FeatureStatus::Ok);
    let p = point_at(&doc, "F4", &point);
    assert!((p[1] - 15.0).abs() < 1e-6, "{p:?}");

    // Sketch4 (F5) on XY: the curve projected along Z, its ends with it.
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    let projected = command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": "F5", "source": {"feature": "F3"},
               "linked": true}),
    );
    assert!(
        projected["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().is_some_and(|e| e.starts_with('c'))),
        "{projected}"
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        file["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["projections"]
                .as_array()
                .is_some_and(|p| p.iter().any(|p| p["source"] == json!({"feature": "F3"})))),
        "the projection records its source"
    );

    // Not into a sketch before it.
    let refused = doc
        .command(
            &json!({"cmd": "sketch.project", "sketch": "F1", "source": {"feature": "F3"}})
                .to_string(),
        )
        .unwrap_err();
    assert!(refused.0.contains("comes after"), "{}", refused.0);
}

/// The newest kernel spec that starts with `start`.
fn spec(doc: &Document<MockKernel>, start: &str) -> String {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|s| s.starts_with(start))
        .cloned()
        .unwrap_or_else(|| panic!("no spec {start}: {:?}", doc.kernel().specs.borrow()))
}

#[test]
fn ribs_and_webs_grow_from_the_curve_in_its_plane() {
    let Design {
        mut doc, offset, ..
    } = design();
    // A body for them to join (a join needs one): Sketch3 (F4) on YZ, a
    // 4 mm circle, swept along the curve (F5, Body1).
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F4", "center": [20, 10], "diameter": 4}),
    );
    let region = text(&circle["region"]);
    let sweep = add(
        &mut doc,
        json!({"type": "sweep", "profiles": [{"sketch": "F4", "region": region}],
               "path": {"feature": "F3"}, "operation": "new_body"}),
    );
    assert_eq!(status(&doc, &sweep), FeatureStatus::Ok);
    // The curve lies in y = 10 (the mock's: 32 lines along the lifted half
    // circle): a rib grows from it as from a chain of a sketch's curves,
    // its pieces named c1, c2, ... in order along it.
    let rib = add(
        &mut doc,
        json!({"type": "rib", "curves": {"feature": "F3"}, "thickness": 2,
               "extent": {"type": "depth", "depth": 5}}),
    );
    assert_eq!(rib, "F6");
    assert_eq!(status(&doc, &rib), FeatureStatus::Ok);
    let made = spec(&doc, "rib F6 ");
    assert!(made.starts_with("rib F6 chains c1,c2,c3,"), "{made}");
    assert!(
        made.contains(",c32 thickness 2 Symmetric depth Some(5.0)"),
        "{made}"
    );
    // A web: one wall per piece.
    let web = add(
        &mut doc,
        json!({"type": "web", "curves": {"feature": "F3"}, "thickness": 1,
               "thickness_location": "side1", "extent": {"type": "depth", "depth": 4}}),
    );
    assert_eq!(status(&doc, &web), FeatureStatus::Ok);
    let made = spec(&doc, &format!("web {web} "));
    assert!(
        made.starts_with(&format!("web {web} chains c1; c2; c3;")),
        "{made}"
    );
    // Both follow the curve: the line's sketch moved, they are made again.
    let ribs = calls(&doc, "rib");
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F2", "dimension": offset,
               "value": "15 mm"}),
    );
    assert_eq!(status(&doc, &rib), FeatureStatus::Ok);
    assert_eq!(status(&doc, &web), FeatureStatus::Ok);
    assert!(
        calls(&doc, "rib") >= ribs + 2,
        "the rib and the web were made again"
    );
    // The file keeps the curve as they name it, and reads it back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(
        file["features"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["type"] == "rib" || f["type"] == "web")
            .all(|f| f["curves"] == json!({"feature": "F3"})),
        "{file}"
    );
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(status(&reopened, &rib), FeatureStatus::Ok);
    assert_eq!(status(&reopened, &web), FeatureStatus::Ok);

    // A feature that makes no curve is refused.
    let refused = doc
        .command(
            &json!({"cmd": "add_feature", "def": {"type": "rib", "curves": {"feature": "F1"},
                    "thickness": 2, "extent": {"type": "to_next"}}})
            .to_string(),
        )
        .unwrap_err();
    assert!(refused.0.contains("makes no curve"), "{}", refused.0);
}

#[test]
fn a_rib_from_a_curve_off_any_plane_is_refused_with_where() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, line, _, _) = sketches(&mut doc);
    // A crossing that bends out of its plane.
    *doc.kernel().crossing.borrow_mut() = Some(vec![
        Curve3::Line {
            start: [20.0, 10.0, 0.0],
            end: [30.0, 10.0, 0.0],
        },
        Curve3::Line {
            start: [30.0, 10.0, 0.0],
            end: [30.0, 20.0, 0.0],
        },
        Curve3::Line {
            start: [30.0, 20.0, 0.0],
            end: [30.0, 20.0, 8.0],
        },
    ]);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &line));
    assert_eq!(status(&doc, &curve), FeatureStatus::Ok);
    let rib = add(
        &mut doc,
        json!({"type": "rib", "curves": {"feature": curve}, "thickness": 2,
               "extent": {"type": "depth", "depth": 5}}),
    );
    let message = error(&doc, &rib);
    assert!(message.contains("does not lie in one plane"), "{message}");
    assert_eq!(calls(&doc, "rib"), 0);
}

/// A line 300 long up Z from the arc's start (20, 0, 0), then a quarter
/// circle of length 700 turning from +Z to +X: a curve of 1 m, as a kernel
/// gives an exact crossing (a line and a conic), and its radius.
fn metre() -> (Vec<Curve3>, f64) {
    let r = 1400.0 / std::f64::consts::PI;
    (
        vec![
            Curve3::Line {
                start: [20.0, 0.0, 0.0],
                end: [20.0, 0.0, 300.0],
            },
            Curve3::Conic {
                center: [20.0 + r, 0.0, 300.0],
                normal: [0.0, 1.0, 0.0],
                x_axis: [-1.0, 0.0, 0.0],
                major: r,
                minor: r,
                start: 0.0,
                end: std::f64::consts::FRAC_PI_2,
                closed: false,
            },
        ],
        r,
    )
}

/// The point of [`metre`]'s curve at length `s` from its start, and its
/// tangent there.
fn metre_at(s: f64, r: f64) -> ([f64; 3], [f64; 3]) {
    if s <= 300.0 {
        return ([20.0, 0.0, s], [0.0, 0.0, 1.0]);
    }
    let angle = (s - 300.0) / r;
    (
        [20.0 + r - r * angle.cos(), 0.0, 300.0 + r * angle.sin()],
        [angle.sin(), 0.0, angle.cos()],
    )
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn construction_along_a_metre_long_curve_lies_on_it_within_a_micrometre_thousandth() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, line, _, _) = sketches(&mut doc);
    let (pieces, r) = metre();
    *doc.kernel().crossing.borrow_mut() = Some(pieces);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &line));
    assert_eq!(status(&doc, &curve), FeatureStatus::Ok);
    // Points at lengths along it (`physical`), and at fractions of it: on
    // the curve where that length along it ends, within 1e-6 mm. Measured
    // along chords the arc's points would lie some 17 nm short at 700 mm.
    for (distance_mm, physical) in [
        (150.0, true),
        (650.0, true),
        (777.7, true),
        (0.9, false),
        (0.999, false),
    ] {
        let s = if physical {
            distance_mm
        } else {
            distance_mm * 1000.0
        };
        let point = add(
            &mut doc,
            json!({"type": "construction_point", "definition": {"type": "along_path",
                   "path": {"feature": curve}, "distance": distance_mm,
                   "physical": physical}}),
        );
        let Some(Datum::Point(point)) = doc.datum(uid(&point)) else {
            panic!("a point");
        };
        let (want, _) = metre_at(s, r);
        assert!(
            distance(point.point, want) < 1e-6,
            "{distance_mm} ({physical}): {:?}, not {want:?}",
            point.point
        );
    }
    // A plane across it at 777.7 mm: through that point, its normal the
    // curve's tangent there.
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane", "definition": {"type": "along_path",
               "path": {"feature": curve}, "distance": 777.7, "physical": true}}),
    );
    let Some(Datum::Plane(plane)) = doc.datum(uid(&plane)) else {
        panic!("a plane");
    };
    let (want, tangent) = metre_at(777.7, r);
    assert!(distance(plane.origin, want) < 1e-6, "{:?}", plane.origin);
    let n = plane.normal();
    let along = n[0] * tangent[0] + n[1] * tangent[1] + n[2] * tangent[2];
    assert!(
        (along.abs() - 1.0).abs() < 1e-9,
        "{n:?} is not along {tangent:?}"
    );
}

#[test]
fn another_component_takes_the_curve_through_its_occurrence() {
    // Motor (O1) at (100, 0, 0) with the curve of the arc and the line (F1,
    // F2, F3): the lifted half circle, its top (0, 10, -20) in Motor's
    // coordinates. Frame (O2) at (100, 50, 0), active: Motor's (x, y, z) is
    // (x, y - 50, z) in Frame's.
    let mut doc = Document::new(MockKernel::default());
    let (_, motor) = doc
        .create_component(
            Some("Motor"),
            Transform::translation([100.0, 0.0, 0.0]),
            true,
        )
        .unwrap();
    let (arc, line, _, _) = sketches(&mut doc);
    let curve = add(&mut doc, intersection("F1", &arc, "F2", &line));
    assert_eq!(curve, "F3");
    assert_eq!(status(&doc, "F3"), FeatureStatus::Ok);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, frame) = doc
        .create_component(
            Some("Frame"),
            Transform::translation([100.0, 50.0, 0.0]),
            true,
        )
        .unwrap();
    let link = json!({"source": motor.to_string(), "target": frame.to_string()});

    // A point halfway along it, made in Frame by its `datum_links`.
    let point = add(
        &mut doc,
        json!({"type": "construction_point", "definition": {"type": "along_path",
               "path": {"feature": "F3"}, "distance": 0.5}, "datum_links": {"F3": link}}),
    );
    assert_eq!(status(&doc, &point), FeatureStatus::Ok);
    let Some(Datum::Point(at)) = doc.datum(uid(&point)) else {
        panic!("a point");
    };
    let p = at.point;
    assert!(p[0].abs() < 1e-6, "{p:?}");
    assert!((p[1] + 40.0).abs() < 1e-9, "{p:?}");
    assert!((p[2].abs() - 20.0).abs() < 1e-6, "{p:?}");
    // Without the link it is refused: the curve is in Motor's coordinates.
    let refused = doc
        .command(
            &json!({"cmd": "add_feature", "def": {"type": "construction_point",
                    "definition": {"type": "along_path", "path": {"feature": "F3"},
                    "distance": 0.5}}})
            .to_string(),
        )
        .unwrap_err();
    assert!(refused.0.contains("cannot use it"), "{}", refused.0);

    // Frame's sketch on its YZ: where the curve crosses it, through Motor:1
    // (YZ: x = -Z, y = +Y): (20, -40).
    let sketch = command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    let sketch = text(&sketch["uid"]);
    let crossed = command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": sketch, "source": {"feature": "F3"},
               "linked": true, "section": true, "occurrence": motor.to_string()}),
    );
    let crossing = crossed["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|e| e.starts_with('p'))
        .expect("a point")
        .to_owned();
    let q = point_at(&doc, &sketch, &crossing);
    assert!((q[0].abs() - 20.0).abs() < 1e-6, "{q:?}");
    assert!((q[1] + 40.0).abs() < 1e-6, "{q:?}");
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        file["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["projections"].as_array().is_some_and(|p| p
                .iter()
                .any(|p| p["source"] == json!({"feature": "F3"}) && p.get("link").is_some()))),
        "the projection records the curve and its link: {file}"
    );

    // Motor:1 5 mm along Y: the point and the crossing follow.
    doc.set_occurrence_transform(motor, Transform::translation([100.0, 5.0, 0.0]), false)
        .unwrap();
    let Some(Datum::Point(at)) = doc.datum(uid(&point)) else {
        panic!("a point");
    };
    assert!((at.point[1] + 35.0).abs() < 1e-9, "{:?}", at.point);
    assert_eq!(status(&doc, &sketch), FeatureStatus::Ok);
    let q = point_at(&doc, &sketch, &crossing);
    assert!((q[1] + 35.0).abs() < 1e-6, "{q:?}");
}

fn preview(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.preview(&value.to_string()).unwrap()).unwrap()
}

/// The preview of an Intersection Curve carries its curve as the
/// `feature_curves` query gives the finished one (mitcad#1496), added and
/// edited; a failed one, and another feature, carry none.
#[test]
fn the_preview_carries_the_curve_as_it_is_drawn() {
    let mut doc = Document::new(MockKernel::default());
    let (arc, line, _, _) = sketches(&mut doc);
    let def = intersection("F1", &arc, "F2", &line);
    let added = preview(&mut doc, json!({"cmd": "add_feature", "def": def}));
    assert_eq!(added["status"], "ok", "{added}");
    let points = added["curve"]["points"]
        .as_array()
        .expect("the curve's points")
        .clone();
    // The mock's 32 lines, their ends once.
    assert!(points.len() >= 30, "{} points", points.len());
    for p in &points {
        let (x, y, z) = (
            p[0].as_f64().unwrap(),
            p[1].as_f64().unwrap(),
            p[2].as_f64().unwrap(),
        );
        assert!((y - 10.0).abs() < 1e-6, "{p} is not 10 from XZ");
        assert!(
            (x.hypot(z) - 20.0).abs() < 1e-6,
            "{p} is not 20 from the Y axis"
        );
    }
    doc.clear_preview();

    // As the view draws the finished one.
    let curve = add(&mut doc, def.clone());
    let drawn: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature_curves"}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(drawn[0]["uid"], json!(curve));
    assert_eq!(drawn[0]["points"], Value::Array(points.clone()));

    // Edited: the curve again, under the feature's own uid.
    let edited = preview(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": curve, "def": def}),
    );
    assert_eq!(edited["status"], "ok", "{edited}");
    assert_eq!(edited["curve"]["points"], Value::Array(points));
    doc.clear_preview();

    // Curves that do not cross: the model's message, no curve.
    let far = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F2", "start": [100, -5], "end": [130, -5]}),
    );
    let far = text(&far["made"][0]);
    let failed = preview(
        &mut doc,
        json!({"cmd": "add_feature", "def": intersection("F1", &arc, "F2", &far)}),
    );
    assert_eq!(failed["status"], "error", "{failed}");
    assert!(
        failed["error"]
            .as_str()
            .is_some_and(|e| e.contains("do not cross")),
        "{failed}"
    );
    assert!(failed.get("curve").is_none(), "{failed}");
    doc.clear_preview();

    // Another feature carries none: a sweep along the curve.
    let sketch = command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    let sketch = text(&sketch["uid"]);
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": sketch, "center": [20, 10], "diameter": 4}),
    );
    let region = text(&circle["region"]);
    let sweep = preview(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sweep",
               "profiles": [{"sketch": sketch, "region": region}],
               "path": {"feature": curve}, "operation": "new_body"}}),
    );
    assert_eq!(sweep["status"], "ok", "{sweep}");
    assert!(sweep.get("curve").is_none(), "{sweep}");
}
