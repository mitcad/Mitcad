// SPDX-License-Identifier: MIT
//! A centre pattern's bolt circle and the centre lines of a view as picks
//! of their own (mitcad#1693) on the mock kernel: a click on a centre
//! pattern's circle picks it as a circle, so that a dimension (Auto or
//! Diameter) gives its diameter, `Ø60 B.C.`; an angle is between two
//! radial lines of a pattern, a centre mark's line, a centreline or a
//! straight edge; `drawing_pick` says so; and they follow a change of the
//! pattern's radius.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// The flange's centre and its holes' radius, on the top of the block.
const CENTER: [f64; 2] = [50.0, 50.0];
const HOLE: f64 = 4.0;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

fn annotation(doc: &Document<MockKernel>, id: &Value) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == *id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

fn value(entry: &Value) -> f64 {
    entry["value"]
        .as_f64()
        .unwrap_or_else(|| panic!("no value: {entry}"))
}

/// A point at an angle (degrees) and a distance from the flange's centre.
fn polar(degrees: f64, radius: f64) -> [f64; 2] {
    let (s, c) = degrees.to_radians().sin_cos();
    [CENTER[0] + radius * c, CENTER[1] + radius * s]
}

fn line(start: [f64; 2], end: [f64; 2]) -> CurveGeometry {
    CurveGeometry::Line {
        start: [start[0], start[1], 20.0],
        end: [end[0], end[1], 20.0],
    }
}

/// A whole circle of radius `HOLE` on the block's top about `center`.
fn hole(center: [f64; 2]) -> CurveGeometry {
    let rim = [center[0] + HOLE, center[1], 20.0];
    CurveGeometry::Circle {
        center: [center[0], center[1], 20.0],
        normal: [0.0, 0.0, 1.0],
        radius: HOLE,
        start: rim,
        end: rim,
    }
}

/// The six holes on a circle of `radius` about the flange's centre, from
/// 0 degrees every 60, as the block's edges 0 to 5.
fn place_holes(doc: &Document<MockKernel>, names: &[String], radius: f64) {
    let mut curves = doc.kernel().edge_curves.borrow_mut();
    for (k, name) in names.iter().take(6).enumerate() {
        curves.insert(name.clone(), hole(polar(60.0 * k as f64, radius)));
    }
}

/// A flange like the report's: a 100 x 100 x 20 block (F2.b0) whose top
/// has six holes of radius 4 on a circle of 60 about (50, 50) (its edges
/// 0 to 5, from 0 degrees every 60) and its front edge (y 0, edge 6); a
/// drawing whose view 1 looks at the top at 1:1; a centre pattern on the
/// holes (its id returned with the edges' names).
fn flange() -> (Document<MockKernel>, Vec<String>, Value) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 100, "height": 100},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    let body: BodyUid = "F2.b0".parse().unwrap();
    let names: Vec<String> = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect();
    assert!(names.len() >= 7, "{names:?}");
    place_holes(&doc, &names, 30.0);
    doc.kernel()
        .edge_curves
        .borrow_mut()
        .insert(names[6].clone(), line([0.0, 0.0], [100.0, 0.0]));
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [200, 150], "scale": "1:1"}),
    );
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "center_pattern", "picks": [{"body": "F2.b0", "edge": names[0]}]}),
    );
    let pattern = added["annotation"].clone();
    let entry = annotation(&doc, &pattern);
    assert_eq!(entry["broken"], false, "{entry}");
    assert_eq!(entry["picks"].as_array().unwrap().len(), 6, "{entry}");
    (doc, names, pattern)
}

/// Where a point of the block's top is on the sheet.
fn on_top(doc: &Document<MockKernel>, at: [f64; 2]) -> [f64; 2] {
    let view = sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == 1)
        .cloned()
        .expect("the top view");
    let place = point(&view["place"]);
    [place[0] + at[0], place[1] + at[1]]
}

/// What a click at a point of the top picks for an annotation `type`
/// (none: a dimension whose picks choose its type).
fn pick_at(doc: &Document<MockKernel>, at: [f64; 2], kind: Option<&str>) -> Value {
    let mut asked = json!({"query": "drawing_pick", "drawing": "D1", "view": 1,
                           "at": on_top(doc, at)});
    if let Some(kind) = kind {
        asked["type"] = json!(kind);
    }
    query(doc, asked)
}

/// An annotation of clicks on the top, placed at a point of it; its entry.
fn added(
    doc: &mut Document<MockKernel>,
    kind: &str,
    clicks: &[[f64; 2]],
    placed: [f64; 2],
) -> Value {
    let picks: Vec<[f64; 2]> = clicks.iter().map(|c| on_top(&*doc, *c)).collect();
    let at = on_top(&*doc, placed);
    let added = command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": kind,
               "picks": picks, "at": at}),
    );
    let entry = annotation(doc, &added["annotation"]);
    assert_eq!(entry["broken"], false, "{entry}");
    entry
}

#[test]
fn a_centre_patterns_circle_is_dimensioned_as_the_bolt_circle() {
    let (mut doc, _, pattern) = flange();
    // On the pattern's circle halfway between two holes: no edge within
    // reach.
    let on_circle = polar(30.0, 30.0);
    let hovered = pick_at(&doc, on_circle, None);
    assert_eq!(hovered["seen"], "circle", "{hovered}");
    assert_eq!(hovered["pattern_circle"], true, "{hovered}");
    assert_eq!(
        hovered["pick"]["drawn"]["kind"], "pattern_circle",
        "{hovered}"
    );
    assert_eq!(hovered["pick"]["drawn"]["pattern"], pattern, "{hovered}");
    assert_eq!(hovered["alone"], true, "{hovered}");
    let circle = &hovered["items"][0]["circle"];
    assert!(near(circle["radius"].as_f64().unwrap(), 30.0), "{hovered}");
    let centre = on_top(&doc, CENTER);
    assert!(near(point(&circle["center"])[0], centre[0]), "{hovered}");
    assert!(near(point(&circle["center"])[1], centre[1]), "{hovered}");
    // The Radius and Diameter types pick it too; a centre mark does not.
    for kind in ["radius", "diameter"] {
        let picked = pick_at(&doc, on_circle, Some(kind));
        assert_eq!(picked["pattern_circle"], true, "{kind}: {picked}");
    }
    let mark = pick_at(&doc, on_circle, Some("center_mark"));
    assert!(mark["pick"].is_null(), "{mark}");
    // Auto: its diameter, written as the bolt circle's.
    let auto = added(&mut doc, "dimension", &[on_circle], [95.0, 95.0]);
    assert_eq!(auto["type"], "diameter", "{auto}");
    assert!(near(value(&auto), 60.0), "{auto}");
    assert_eq!(auto["label"], "Ø60 B.C.", "{auto}");
    assert_eq!(auto["format"]["after"], "B.C.", "{auto}");
    // Diameter, the same; its text edited as any dimension's.
    let diameter = added(&mut doc, "diameter", &[on_circle], [5.0, 95.0]);
    assert!(near(value(&diameter), 60.0), "{diameter}");
    assert_eq!(diameter["label"], "Ø60 B.C.", "{diameter}");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": diameter["id"],
               "format": {"after": "PCD"}}),
    );
    assert_eq!(annotation(&doc, &diameter["id"])["label"], "Ø60 PCD");
    // A diameter of a hole is no bolt circle.
    let hole_rim = polar(0.0, 30.0 + HOLE);
    let one = added(&mut doc, "diameter", &[hole_rim], [95.0, 5.0]);
    assert_eq!(one["label"], "Ø8", "{one}");
}

#[test]
fn an_angle_is_between_centre_lines_and_edges() {
    let (mut doc, names, pattern) = flange();
    // The radial lines of the holes at 60 and 120 degrees, outside their
    // rims: the Angle type picks them as lines.
    let (at60, at120) = (polar(60.0, 35.5), polar(120.0, 35.5));
    let hovered = pick_at(&doc, at60, Some("angle"));
    assert_eq!(hovered["seen"], "line", "{hovered}");
    assert_eq!(hovered["center_line"], true, "{hovered}");
    assert_eq!(
        hovered["pick"]["drawn"]["kind"], "pattern_line",
        "{hovered}"
    );
    assert_eq!(hovered["pick"]["drawn"]["pattern"], pattern, "{hovered}");
    assert_eq!(hovered["pick"]["edge"], names[1].as_str(), "{hovered}");
    // An edge extension takes no centre line there.
    let extension = pick_at(&doc, at60, Some("edge_extension"));
    assert!(extension["pick"].is_null(), "{extension}");
    // The angle between the two: 60 degrees, an arc about the pattern's
    // centre.
    let angle = added(&mut doc, "angle", &[at60, at120], polar(90.0, 45.0));
    assert_eq!(angle["type"], "angle", "{angle}");
    assert!(near(value(&angle), PI / 3.0), "{angle}");
    // A radial line and the front edge: 60 degrees.
    let edge = added(&mut doc, "angle", &[at60, [70.0, 0.5]], [80.0, 10.0]);
    assert!(near(value(&edge), PI / 3.0), "{edge}");
    // Picks as an agent gives them: the hole at 60 degrees' horizontal
    // centre mark line toward the right and its radial line, 120 degrees
    // apart about the hole's centre.
    let hole60 = json!({"body": "F2.b0", "edge": names[1]});
    let mut mark = hole60.clone();
    mark["drawn"] = json!({"kind": "center_mark", "arm": "right"});
    let mut radial = hole60.clone();
    radial["drawn"] = json!({"kind": "pattern_line", "pattern": pattern});
    let given = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "angle",
               "picks": [mark, radial]}),
    );
    let marked = annotation(&doc, &given["annotation"]);
    assert_eq!(marked["broken"], false, "{marked}");
    assert!(near(value(&marked), 2.0 * PI / 3.0), "{marked}");
    // A centreline through the holes at 0 and 60 degrees (its line along
    // 120 degrees) and the front edge: 60 degrees.
    let centerline = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "centerline",
               "picks": [{"body": "F2.b0", "edge": names[0]}, {"body": "F2.b0", "edge": names[1]}]}),
    );
    let middle = {
        let (a, b) = (polar(0.0, 30.0), polar(60.0, 30.0));
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    };
    let on_line = pick_at(&doc, middle, Some("angle"));
    assert_eq!(on_line["pick"]["drawn"]["kind"], "centerline", "{on_line}");
    assert_eq!(
        on_line["pick"]["drawn"]["centerline"], centerline["annotation"],
        "{on_line}"
    );
    let crossing = added(&mut doc, "angle", &[middle, [30.0, 0.5]], [20.0, 10.0]);
    assert!(near(value(&crossing), PI / 3.0), "{crossing}");
    // A centre pattern or a centreline takes no centre line as its pick.
    let refused = doc
        .command(
            &json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                    "type": "centerline", "picks": [radial]})
            .to_string(),
        )
        .expect_err("a centreline of a centre line");
    assert!(refused.to_string().contains("center line"), "{refused}");
}

#[test]
fn the_bolt_circle_and_its_angles_follow_the_patterns_radius() {
    let (mut doc, names, pattern) = flange();
    let diameter = added(&mut doc, "diameter", &[polar(30.0, 30.0)], [95.0, 95.0]);
    let angle = added(
        &mut doc,
        "angle",
        &[polar(60.0, 35.5), polar(120.0, 35.5)],
        polar(90.0, 45.0),
    );
    assert!(near(value(&diameter), 60.0), "{diameter}");
    // The holes moved out onto a circle of 40.
    place_holes(&doc, &names, 40.0);
    let diameter = annotation(&doc, &diameter["id"]);
    assert_eq!(diameter["broken"], false, "{diameter}");
    assert!(near(value(&diameter), 80.0), "{diameter}");
    assert_eq!(diameter["label"], "Ø80 B.C.", "{diameter}");
    let angle = annotation(&doc, &angle["id"]);
    assert_eq!(angle["broken"], false, "{angle}");
    assert!(near(value(&angle), PI / 3.0), "{angle}");
    // The centre pattern deleted: what was drawn by it is broken.
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": pattern}),
    );
    let gone = annotation(&doc, &diameter["id"]);
    assert_eq!(gone["broken"], true, "{gone}");
    assert!(
        gone["error"]
            .as_str()
            .is_some_and(|e| e.contains("center pattern")),
        "{gone}"
    );
}
