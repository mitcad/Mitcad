// SPDX-License-Identifier: MIT
//! Dimensions whose picks choose their type (mitcad#1272) on the mock
//! kernel: `"type": "dimension"` makes a diameter of one circle, a radius
//! of one arc, the length of one straight edge, an angle of two edges that
//! are not parallel and a linear dimension of any other two, horizontal,
//! vertical or aligned as its placing click chooses; the same pick twice is
//! one pick and never gives 0 mm; `drawing_snap` says what a placing click
//! makes; a centerline of a curved slot's two sides is its centre arc; and
//! a line or an arc drawn outside the model snaps to ends, middles and
//! centres (`"want": "snap"`); a section's cut edge alone is the length of
//! the piece of its cut line under the click (mitcad#1361).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

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

/// The group `A<id>` of the sheet.
fn group(doc: &Document<MockKernel>, id: &Value) -> Value {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == format!("A{id}").as_str())
        .cloned()
        .unwrap_or_else(|| panic!("no group of annotation {id}"))
}

fn line(start: [f64; 2], end: [f64; 2]) -> CurveGeometry {
    CurveGeometry::Line {
        start: [start[0], start[1], 20.0],
        end: [end[0], end[1], 20.0],
    }
}

/// An arc on the block's top about `center`, counter-clockwise seen from
/// above from `from` to `to` degrees; a whole circle when they are equal.
fn arc(center: [f64; 2], radius: f64, from: f64, to: f64) -> CurveGeometry {
    let at = |degrees: f64| {
        let (s, c) = degrees.to_radians().sin_cos();
        [center[0] + radius * c, center[1] + radius * s, 20.0]
    };
    CurveGeometry::Circle {
        center: [center[0], center[1], 20.0],
        normal: [0.0, 0.0, 1.0],
        radius,
        start: at(from),
        end: at(to),
    }
}

/// The curved slot's centre: its sides are arcs of radius 8 and 12 about
/// it from 30 to 90 degrees, its ends' centres on the radius 10.
const SLOT: [f64; 2] = [28.0, 10.0];

/// A point at an angle (degrees) and a distance from the slot's centre.
fn slot_at(degrees: f64, radius: f64) -> [f64; 2] {
    let (s, c) = degrees.to_radians().sin_cos();
    [SLOT[0] + radius * c, SLOT[1] + radius * s]
}

/// A plate like the report's on the top of a 40 x 30 x 20 block (F2.b0),
/// seen in the top view (view 2: the model's x and y at 1:1) of a drawing
/// whose front view is view 1. Its top's edges: 0 the front edge (y 0), 1
/// the right edge (x 40), 2 a slanted edge from (0, 22) to (12, 30), 3 a
/// hole of radius 3 at (10, 8), 4 and 5 the curved slot's outer and inner
/// sides, 6 the back edge (y 30), 7 the slot's end cap at 30 degrees.
fn plate() -> (Document<MockKernel>, Vec<String>) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
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
    assert!(names.len() >= 8, "{names:?}");
    let end = slot_at(30.0, 10.0);
    let curves = [
        line([0.0, 0.0], [40.0, 0.0]),
        line([40.0, 0.0], [40.0, 30.0]),
        line([0.0, 22.0], [12.0, 30.0]),
        arc([10.0, 8.0], 3.0, 0.0, 0.0),
        arc(SLOT, 12.0, 30.0, 90.0),
        arc(SLOT, 8.0, 30.0, 90.0),
        line([0.0, 30.0], [40.0, 30.0]),
        arc(end, 2.0, 210.0, 30.0),
    ];
    {
        let mut edge_curves = doc.kernel().edge_curves.borrow_mut();
        for (name, curve) in names.iter().zip(curves) {
            edge_curves.insert(name.clone(), curve);
        }
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "above"}),
    );
    (doc, names)
}

/// Where a point of the plate's top (the top view's) is on the sheet.
fn on_top(doc: &Document<MockKernel>, at: [f64; 2]) -> [f64; 2] {
    let view = sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == 2)
        .cloned()
        .expect("the top view");
    let place = point(&view["place"]);
    [place[0] + at[0], place[1] + at[1]]
}

/// A dimension whose picks choose its type, of clicks on the plate's top,
/// placed at a point of it; its annotation.
fn dimension(doc: &mut Document<MockKernel>, clicks: &[[f64; 2]], placed: [f64; 2]) -> Value {
    let picks: Vec<[f64; 2]> = clicks.iter().map(|c| on_top(&*doc, *c)).collect();
    let at = on_top(&*doc, placed);
    let added = command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "dimension",
               "picks": picks, "at": at}),
    );
    let entry = annotation(doc, &added["annotation"]);
    assert_eq!(entry["broken"], false, "{entry}");
    entry
}

fn value(entry: &Value) -> f64 {
    entry["value"].as_f64().unwrap()
}

#[test]
fn one_pick_makes_a_diameter_a_radius_or_an_edges_length() {
    let (mut doc, names) = plate();
    // The hole: its diameter.
    let hole = dimension(&mut doc, &[[13.0, 8.0]], [20.0, 20.0]);
    assert_eq!(hole["type"], "diameter", "{hole}");
    assert!(near(value(&hole), 6.0), "{hole}");
    assert_eq!(hole["label"], "Ø6");
    // The slot's outer side: its radius.
    let side = dimension(&mut doc, &[slot_at(60.0, 12.0)], [10.0, 28.0]);
    assert_eq!(side["type"], "radius", "{side}");
    assert!(near(value(&side), 12.0), "{side}");
    // The right edge: its length from its start to its end, along it
    // wherever it is placed.
    let right = dimension(&mut doc, &[[40.0, 25.0]], [50.0, 15.0]);
    assert_eq!(right["type"], "linear", "{right}");
    assert_eq!(right["along"], "vertical", "{right}");
    assert!(near(value(&right), 30.0), "{right}");
    assert_eq!(right["picks"][0]["edge"], names[1].as_str());
    assert_eq!(right["picks"][0]["point"], "start");
    assert_eq!(right["picks"][1]["edge"], names[1].as_str());
    assert_eq!(right["picks"][1]["point"], "end");
    // The slanted edge, 12 along x and 8 along y: above it horizontal,
    // beside it vertical, between or beyond its ends both ways aligned.
    let slanted = (12.0f64).hypot(8.0);
    for (placed, along, length) in [
        ([6.0, 40.0], "horizontal", 12.0),
        ([20.0, 26.0], "vertical", 8.0),
        ([7.0, 25.0], "aligned", slanted),
        ([20.0, 40.0], "aligned", slanted),
    ] {
        let edge = dimension(&mut doc, &[[6.0, 26.0]], placed);
        assert_eq!(edge["along"], along, "placed at {placed:?}: {edge}");
        assert!(near(value(&edge), length), "placed at {placed:?}: {edge}");
    }
    // A point alone measures nothing.
    let centre = on_top(&doc, [10.0, 8.3]);
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "dimension",
               "picks": [centre], "at": centre}),
    );
    assert!(error.contains("a point alone"), "{error}");
}

#[test]
fn two_picks_make_an_angle_or_a_distance_the_placing_click_directs() {
    let (mut doc, _) = plate();
    // The right and the slanted edge: the angle between them.
    let angle = dimension(&mut doc, &[[40.0, 25.0], [6.0, 26.0]], [30.0, 35.0]);
    assert_eq!(angle["type"], "angle", "{angle}");
    assert!(near(value(&angle), (12.0f64).atan2(8.0)), "{angle}");
    // The front and the back edge, parallel: the distance between them,
    // their middles one above the other.
    let across = dimension(&mut doc, &[[20.0, 0.0], [25.0, 30.0]], [45.0, 15.0]);
    assert_eq!(across["type"], "linear", "{across}");
    assert_eq!(across["along"], "vertical", "{across}");
    assert!(near(value(&across), 30.0), "{across}");
    // The hole and the right edge placed between them: aligned, square to
    // the edge, the hole's centre's distance from it.
    let to_edge = dimension(&mut doc, &[[13.0, 8.0], [40.0, 25.0]], [25.0, 12.0]);
    assert_eq!(to_edge["along"], "aligned", "{to_edge}");
    assert!(near(value(&to_edge), 30.0), "{to_edge}");
    // The slot's centre, picked where it is, and the hole: the radial
    // distance placed between them; above them horizontal, beside them
    // vertical.
    let radial = (18.0f64).hypot(2.0);
    for (placed, along, length) in [
        ([20.0, 9.0], "aligned", radial),
        ([20.0, 25.0], "horizontal", 18.0),
        ([35.0, 9.0], "vertical", 2.0),
    ] {
        let entry = dimension(&mut doc, &[[28.0, 10.3], [13.0, 8.0]], placed);
        assert_eq!(entry["type"], "linear", "{entry}");
        assert_eq!(entry["picks"][0]["point"], "center", "{entry}");
        assert_eq!(entry["along"], along, "placed at {placed:?}: {entry}");
        assert!(near(value(&entry), length), "placed at {placed:?}: {entry}");
    }
    // Three picks are a baseline's or a chain's.
    let picks: Vec<[f64; 2]> = [[13.0, 8.0], [40.0, 25.0], [20.0, 0.0]]
        .iter()
        .map(|c| on_top(&doc, *c))
        .collect();
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "dimension",
               "picks": picks}),
    );
    assert!(error.contains("one pick"), "{error}");
}

#[test]
fn the_same_pick_twice_never_gives_zero() {
    let (mut doc, names) = plate();
    // The hole clicked twice: one pick, its diameter.
    let hole = dimension(&mut doc, &[[13.0, 8.0], [10.0, 5.0]], [20.0, 20.0]);
    assert_eq!(hole["type"], "diameter", "{hole}");
    assert_eq!(hole["picks"].as_array().unwrap().len(), 1, "{hole}");
    assert!(near(value(&hole), 6.0), "{hole}");
    // The right edge clicked twice: its length.
    let right = dimension(&mut doc, &[[40.0, 25.0], [40.0, 20.0]], [50.0, 15.0]);
    assert!(near(value(&right), 30.0), "{right}");
    // The hole and its own centre lie on one point: refused, not 0.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "dimension",
               "picks": [{"body": "F2.b0", "edge": names[3]},
                         {"body": "F2.b0", "edge": names[3], "point": "center"}]}),
    );
    assert!(error.contains("one point"), "{error}");
    // Nothing of these was kept.
    let kept = sheet(&doc)["annotations"].as_array().unwrap().len();
    assert_eq!(kept, 2);
}

#[test]
fn a_dimensions_placing_click_says_what_it_makes() {
    let (doc, _) = plate();
    let picks = [on_top(&doc, [28.0, 10.3]), on_top(&doc, [13.0, 8.0])];
    let snap = |placed: [f64; 2]| {
        query(
            &doc,
            json!({"query": "drawing_snap", "free": true,
                   "command": {"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
                               "type": "dimension", "picks": picks,
                               "at": on_top(&doc, placed)}}),
        )
    };
    // Between them: aligned, along the line from the slot's centre to the
    // hole's, from those two points on the sheet.
    let between = snap([20.0, 9.0]);
    assert_eq!(between["type"], "linear", "{between}");
    assert_eq!(between["along"], "aligned", "{between}");
    let d = point(&between["direction"]);
    let radial = (18.0f64).hypot(2.0);
    assert!(
        near(d[0].abs(), 18.0 / radial) && near(d[1].abs(), 2.0 / radial),
        "{between}"
    );
    let ends = &between["ends"];
    let (slot, hole) = (on_top(&doc, SLOT), on_top(&doc, [10.0, 8.0]));
    assert!(
        near(point(&ends[0])[0], slot[0]) && near(point(&ends[0])[1], slot[1]),
        "{between}"
    );
    assert!(
        near(point(&ends[1])[0], hole[0]) && near(point(&ends[1])[1], hole[1]),
        "{between}"
    );
    // Above them: horizontal.
    let above = snap([20.0, 25.0]);
    assert_eq!(above["along"], "horizontal", "{above}");
    assert_eq!(point(&above["direction"]), [1.0, 0.0], "{above}");
    // One circle: a diameter, nothing to measure along.
    let one = query(
        &doc,
        json!({"query": "drawing_snap", "free": true,
               "command": {"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
                           "type": "dimension", "picks": [on_top(&doc, [13.0, 8.0])],
                           "at": on_top(&doc, [20.0, 20.0])}}),
    );
    assert_eq!(one["type"], "diameter", "{one}");
    assert!(one.get("direction").is_none(), "{one}");
}

#[test]
fn a_centerline_of_a_curved_slots_sides_is_its_centre_arc() {
    let (mut doc, _) = plate();
    let picks = [
        on_top(&doc, slot_at(60.0, 12.0)),
        on_top(&doc, slot_at(60.0, 8.0)),
    ];
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "centerline",
               "picks": picks}),
    );
    let id = &added["annotation"];
    assert_eq!(annotation(&doc, id)["broken"], false);
    let line = group(&doc, id);
    let items = line["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{line}");
    assert_eq!(items[0]["pen"], "chain", "{line}");
    let arc = &items[0]["arc"];
    let (center, radius) = (point(&arc["center"]), arc["radius"].as_f64().unwrap());
    let slot = on_top(&doc, SLOT);
    assert!(
        near(center[0], slot[0]) && near(center[1], slot[1]),
        "{line}"
    );
    assert!(near(radius, 10.0), "{line}");
    // From one end's centre to the other's, run on a little beyond each.
    let (start, end) = (arc["start"].as_f64().unwrap(), arc["end"].as_f64().unwrap());
    let (first, last) = (30f64.to_radians(), 90f64.to_radians());
    assert!(start < first && start > first - 0.6, "{line}");
    assert!(end > last && end < last + 0.6, "{line}");
}

#[test]
fn lines_and_arcs_snap_to_ends_middles_and_centres() {
    let (doc, _) = plate();
    let snap = |at: [f64; 2]| {
        query(
            &doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": 2,
                   "at": on_top(&doc, at), "want": "snap"}),
        )
    };
    let at = |answer: &Value| point(&answer["at"]);
    // The right edge's middle.
    let middle = snap([40.4, 15.6]);
    assert_eq!(middle["pick"]["point"], "center", "{middle}");
    let expected = on_top(&doc, [40.0, 15.0]);
    assert!(near(at(&middle)[0], expected[0]) && near(at(&middle)[1], expected[1]));
    // The slot's end's centre, and the centre of its sides.
    let end = slot_at(30.0, 10.0);
    let cap = snap([end[0] + 0.2, end[1] + 0.2]);
    let expected = on_top(&doc, end);
    assert!(
        near(at(&cap)[0], expected[0]) && near(at(&cap)[1], expected[1]),
        "{cap}"
    );
    // Both within reach, the nearer wins whatever its kind: 0.6 from the
    // end's centre and 1.4 from the inner side's end the centre, the other
    // way round the end.
    let inner = slot_at(30.0, 8.0);
    let toward = |share: f64| {
        [
            end[0] + (inner[0] - end[0]) * share,
            end[1] + (inner[1] - end[1]) * share,
        ]
    };
    let near_centre = snap(toward(0.3));
    assert_eq!(near_centre["pick"]["point"], "center", "{near_centre}");
    assert!(
        near(at(&near_centre)[0], expected[0]) && near(at(&near_centre)[1], expected[1]),
        "{near_centre}"
    );
    let near_end = snap(toward(0.7));
    let side_end = on_top(&doc, inner);
    assert!(
        near(at(&near_end)[0], side_end[0]) && near(at(&near_end)[1], side_end[1]),
        "{near_end}"
    );
    let centre = snap([SLOT[0] + 0.5, SLOT[1] - 0.5]);
    let expected = on_top(&doc, SLOT);
    assert!(
        near(at(&centre)[0], expected[0]) && near(at(&centre)[1], expected[1]),
        "{centre}"
    );
    // The front edge's end.
    let corner = snap([39.0, 0.5]);
    let expected = on_top(&doc, [40.0, 0.0]);
    assert!(
        near(at(&corner)[0], expected[0]) && near(at(&corner)[1], expected[1]),
        "{corner}"
    );
    // Away from them all: nothing, though an edge is near.
    let none = snap([32.0, 29.5]);
    assert!(none["pick"].is_null(), "{none}");
    // A circle hovered for a dimension shows its centre too.
    let hole = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": 2,
               "at": on_top(&doc, [13.0, 8.0]), "type": "diameter"}),
    );
    let items = hole["items"].as_array().unwrap();
    let centre = on_top(&doc, [10.0, 8.0]);
    assert!(
        items.iter().any(|i| i
            .get("point")
            .is_some_and(|p| near(point(p)[0], centre[0]) && near(point(p)[1], centre[1]))),
        "{hole}"
    );
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    near(a[0], b[0]) && near(a[1], b[1])
}

#[test]
fn a_corner_is_dimensioned_to_a_centre_marks_line() {
    let (mut doc, _) = plate();
    // On the line through the slot's centre, left of its sides: no edge or
    // point within reach, and no mark there yet (its sides turn less than
    // half way round: no automatic mark).
    let on_line = [18.0, 10.0];
    let hover = |doc: &Document<MockKernel>| {
        query(
            doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": 2,
                   "at": on_top(doc, on_line)}),
        )
    };
    let before = hover(&doc);
    assert!(before["pick"].is_null(), "{before}");
    // A centre mark on the slot's outer side: its horizontal line runs 12
    // and 2.5 mm each way from the slot's centre (mitcad#1338).
    let side = on_top(&doc, slot_at(60.0, 12.0));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
               "type": "center_mark", "picks": [side]}),
    );
    let hovered = hover(&doc);
    assert_eq!(hovered["seen"], "point", "{hovered}");
    assert_eq!(hovered["center_line"], true, "{hovered}");
    assert_eq!(hovered["pick"]["point"], "center", "{hovered}");
    let centre = on_top(&doc, SLOT);
    assert!(same(point(&hovered["at"]), centre), "{hovered}");
    // The mark's line, from the centre leftward through the click, first;
    // then the centre.
    let items = hovered["items"].as_array().unwrap();
    let line = &items[0]["line"];
    assert!(same(point(&line[0]), centre), "{hovered}");
    assert!(
        same(point(&line[1]), [centre[0] - 14.5, centre[1]]),
        "{hovered}"
    );
    assert!(
        items
            .iter()
            .any(|i| i.get("point").is_some_and(|p| same(point(p), centre))),
        "{hovered}"
    );
    // The plate's corner (40, 0) and the mark's line, in both orders:
    // above them horizontal (12 to its vertical line), beside them
    // vertical (10 to its horizontal line).
    for clicks in [[[40.0, 0.0], on_line], [on_line, [40.0, 0.0]]] {
        for (placed, along, length) in [
            ([34.0, 35.0], "horizontal", 12.0),
            ([45.0, 5.0], "vertical", 10.0),
        ] {
            let entry = dimension(&mut doc, &clicks, placed);
            assert_eq!(
                entry["type"], "linear",
                "{clicks:?} placed at {placed:?}: {entry}"
            );
            assert_eq!(
                entry["along"], along,
                "{clicks:?} placed at {placed:?}: {entry}"
            );
            assert!(
                near(value(&entry), length),
                "{clicks:?} placed at {placed:?}: {entry}"
            );
            let points: Vec<&Value> = entry["picks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| &p["point"])
                .collect();
            assert!(points.contains(&&json!("center")), "{entry}");
        }
    }
}

#[test]
fn the_corners_of_a_view_are_its_visible_edge_ends() {
    let (doc, _) = plate();
    let corners = query(&doc, json!({"query": "drawing_corners", "drawing": "D1"}));
    let views = corners["views"].as_array().unwrap();
    assert_eq!(views.len(), 2, "{corners}");
    let top = views
        .iter()
        .find(|v| v["view"] == 2)
        .unwrap_or_else(|| panic!("no top view: {corners}"));
    let points: Vec<[f64; 2]> = top["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(point)
        .collect();
    // The plate's corners and the slanted edge's ends, on the lines the
    // view draws.
    for corner in [
        [0.0, 0.0],
        [40.0, 0.0],
        [40.0, 30.0],
        [0.0, 30.0],
        [0.0, 22.0],
        [12.0, 30.0],
    ] {
        let at = on_top(&doc, corner);
        assert!(
            points.iter().any(|p| same(*p, at)),
            "{corner:?} is not among {points:?}"
        );
    }
    // Each place once.
    for (i, p) in points.iter().enumerate() {
        assert!(
            !points[i + 1..].iter().any(|q| same(*p, *q)),
            "{p:?} twice in {points:?}"
        );
    }
    // Not the slot's sides' ends, which the view does not draw, nor a
    // centre.
    for hidden in [slot_at(30.0, 12.0), slot_at(90.0, 8.0), SLOT, [10.0, 8.0]] {
        let at = on_top(&doc, hidden);
        assert!(
            !points.iter().any(|p| same(*p, at)),
            "{hidden:?} is among {points:?}"
        );
    }
    // A corner hovered is one of them: the same point a click picks.
    let hovered = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": 2,
               "at": on_top(&doc, [39.5, 0.4])}),
    );
    assert_eq!(hovered["seen"], "point", "{hovered}");
    let at = point(&hovered["at"]);
    assert!(points.iter().any(|p| same(*p, at)), "{hovered}");
    // A sheet that is not there is refused.
    let error = doc
        .query(&json!({"query": "drawing_corners", "drawing": "D1", "sheet": 3}).to_string())
        .expect_err("no sheet 3");
    assert!(error.to_string().contains("no sheet 3"), "{error}");
}

/// The bottom face of the plate (F2.b0's start), z 0.
fn bottom() -> String {
    format!("F2:start({R})")
}

/// The plate cut by a section at y 15 of its top view, seen from the
/// front (its x the model's x, its y the model's z, at 1:1), with the cut
/// faces of a stepped part with a hole as the kernel gives them
/// (mitcad#1361): a base 40 wide and 10 high, a boss 20 wide and 10 high
/// on its left half, and a hole across both from x 6 to x 10, so that the
/// bottom's cut is two pieces, x 0..6 and 10..40. The section's id and
/// where the model's x 0, z 0 is on the sheet (its box's low corner).
fn stepped_section(doc: &mut Document<MockKernel>) -> (u64, [f64; 2]) {
    doc.kernel().cuts_sections.set(true);
    doc.kernel().surfaces.borrow_mut().insert(
        bottom(),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    let at = |x: f64, z: f64| [x, 15.0, z];
    *doc.kernel().drawn_cuts.borrow_mut() = vec![
        vec![vec![
            at(0.0, 0.0),
            at(6.0, 0.0),
            at(6.0, 20.0),
            at(0.0, 20.0),
        ]],
        vec![vec![
            at(10.0, 0.0),
            at(40.0, 0.0),
            at(40.0, 10.0),
            at(20.0, 10.0),
            at(20.0, 20.0),
            at(10.0, 20.0),
        ]],
    ];
    let line = [on_top(&*doc, [-5.0, 15.0]), on_top(&*doc, [45.0, 15.0])];
    let section = command(
        doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 2,
               "line": line, "flip": true, "at": [300, 200]}),
    )["view"]
        .as_u64()
        .unwrap();
    let shown = sheet(&*doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == section)
        .cloned()
        .expect("the section");
    (section, point(&shown["box"][0]))
}

/// How long a pick's line is on the sheet (its first item's).
fn line_length(picked: &Value) -> f64 {
    let ends = &picked["items"][0]["line"];
    let (a, b) = (point(&ends[0]), point(&ends[1]));
    (b[0] - a[0]).hypot(b[1] - a[1])
}

#[test]
fn a_sections_cut_edge_alone_is_the_length_of_the_piece_under_the_click() {
    let (mut doc, _) = plate();
    let (section, lo) = stepped_section(&mut doc);
    let on = |x: f64, z: f64| [lo[0] + x, lo[1] + z];
    let hover = |doc: &Document<MockKernel>, at: [f64; 2]| {
        query(
            doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": section, "at": at}),
        )
    };
    // The bottom's cut beside the hole: the piece under the click, 30
    // long, not the line across the hole (40); a dimension alone.
    let long = hover(&doc, on(25.0, 0.2));
    assert_eq!(long["pick"]["face"], bottom().as_str(), "{long}");
    assert!(long["pick"]["on_cut"].is_array(), "{long}");
    assert_eq!(long["seen"], "line", "{long}");
    assert!(near(line_length(&long), 30.0), "{long}");
    assert_eq!(long["alone"], true, "{long}");
    // The piece left of the hole: 6 long.
    let short = hover(&doc, on(3.0, 0.2));
    assert!(near(line_length(&short), 6.0), "{short}");
    assert_ne!(long["pick"], short["pick"]);
    // A corner alone waits for a second pick, and says why.
    let corner = hover(&doc, on(10.0, 0.0));
    assert_eq!(corner["seen"], "point", "{corner}");
    assert_eq!(corner["alone"], false, "{corner}");
    assert!(
        corner["next"].as_str().unwrap().contains("a point alone"),
        "{corner}"
    );

    // Picked alone: its length, from the piece's start to its end, 30.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "dimension", "picks": [on(25.0, 0.2)], "at": on(25.0, -15.0)}),
    );
    let entry = annotation(&doc, &added["annotation"]);
    assert_eq!(entry["broken"], false, "{entry}");
    assert_eq!(entry["type"], "linear", "{entry}");
    assert_eq!(entry["along"], "horizontal", "{entry}");
    assert!((value(&entry) - 30.0).abs() < 1e-9, "{entry}");
    assert_eq!(entry["label"], "30", "{entry}");
    let picks = entry["picks"].as_array().unwrap();
    assert_eq!(picks.len(), 2, "{entry}");
    for (pick, end) in picks.iter().zip(["start", "end"]) {
        assert_eq!(pick["face"], bottom().as_str(), "{entry}");
        assert_eq!(pick["on_cut"], long["pick"]["on_cut"], "{entry}");
        assert_eq!(pick["point"], end, "{entry}");
    }

    // The piece's corners are its ends: picked as points, the same 30
    // between them; the hole's corner and the plate's left one, 10.
    for (clicks, length) in [
        ([on(10.0, 0.3), on(40.0, 0.3)], 30.0),
        ([on(10.0, 0.3), on(0.0, 0.3)], 10.0),
    ] {
        let added = command(
            &mut doc,
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
                   "type": "dimension", "picks": clicks, "at": on(20.0, -25.0)}),
        );
        let entry = annotation(&doc, &added["annotation"]);
        assert_eq!(entry["broken"], false, "{entry}");
        assert!(near(value(&entry), length), "{clicks:?}: {entry}");
        for pick in entry["picks"].as_array().unwrap() {
            assert!(pick["on_cut"].is_array(), "{entry}");
            assert!(
                pick["point"] == "start" || pick["point"] == "end",
                "{entry}"
            );
        }
    }
    // The section's corners show them (mitcad#1338), the hole's too.
    let corners = query(&doc, json!({"query": "drawing_corners", "drawing": "D1"}));
    let points: Vec<[f64; 2]> = corners["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["view"] == section)
        .unwrap_or_else(|| panic!("no section: {corners}"))["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(point)
        .collect();
    for x in [0.0, 6.0, 10.0, 40.0] {
        assert!(
            points.iter().any(|p| same(*p, on(x, 0.0))),
            "{x} is not among {points:?}"
        );
    }

    // The face picked whole spans the hole: no length of its own.
    let whole = json!({"body": "F2.b0", "face": bottom()});
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "dimension", "picks": [whole], "at": on(25.0, -15.0)}),
    );
    assert!(error.contains("spans its gaps"), "{error}");
    // A point on the cut line in the hole lies on no piece: why it is
    // refused. The face's own coordinates go along the line as the model's
    // x does: x 8 lies 17/22 of the way from the long piece's middle (25)
    // to the short one's (3).
    let (a, b) = (
        point(&long["pick"]["on_cut"]),
        point(&short["pick"]["on_cut"]),
    );
    let f = 17.0 / 22.0;
    let gap = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "dimension", "at": on(25.0, -15.0),
               "picks": [{"body": "F2.b0", "face": bottom(), "on_cut": gap}]}),
    );
    assert!(error.contains("lies on no piece"), "{error}");
}

/// A 40 x 30 x 20 block (F2.b0) standing for a square tube seen from the
/// side (mitcad#1691), 40 long along x, its front top corner rounded: the
/// round face `side(c1…)` of radius 3 about the axis at y 3, z 17 along x,
/// bounded by its own quarter circles at x 0 and 40, beside the straight
/// edge where the round meets the top, from (0, 3, 20) to (40, 3, 20) (the
/// name returned). A drawing with its front view at 1:1 (view 1: the
/// model's x and z), whose top line, z 20, is both that edge and the round
/// face's outline.
fn rounded_tube() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
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
    let side = "F2:side(c1[c4,c2])".to_owned();
    let find = |with: &str| {
        names
            .iter()
            .find(|n| n.contains(&side) && n.contains(with))
            .cloned()
            .unwrap_or_else(|| panic!("no edge of {side} and {with}: {names:?}"))
    };
    let (first, last) = (find(":start("), find(":end("));
    let straight = names
        .iter()
        .find(|n| !n.contains("side(c1["))
        .cloned()
        .expect("an edge away from the round face");
    let quarter = |x: f64| CurveGeometry::Circle {
        center: [x, 3.0, 17.0],
        normal: [1.0, 0.0, 0.0],
        radius: 3.0,
        start: [x, 3.0, 20.0],
        end: [x, 0.0, 17.0],
    };
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(first, quarter(0.0));
        curves.insert(last, quarter(40.0));
        curves.insert(
            straight.clone(),
            CurveGeometry::Line {
                start: [0.0, 3.0, 20.0],
                end: [40.0, 3.0, 20.0],
            },
        );
    }
    doc.kernel().surfaces.borrow_mut().insert(
        side,
        SurfaceGeometry::Cylinder {
            origin: [0.0, 3.0, 17.0],
            axis: [1.0, 0.0, 0.0],
            radius: 3.0,
        },
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    (doc, straight)
}

/// Where a point of the tube's front (the model's x and z) is on the
/// sheet.
fn on_front(doc: &Document<MockKernel>, x: f64, z: f64) -> [f64; 2] {
    let view = sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == 1)
        .cloned()
        .expect("the front view");
    let place = point(&view["place"]);
    [place[0] + x, place[1] + z]
}

#[test]
fn a_click_on_a_rounded_tubes_long_line_dimensions_its_length() {
    let (mut doc, straight) = rounded_tube();
    let pick = |doc: &Document<MockKernel>, at: [f64; 2], extra: Value| {
        let mut asked = json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at});
        if let (Some(asked), Some(extra)) = (asked.as_object_mut(), extra.as_object()) {
            asked.extend(extra.clone());
        }
        query(doc, asked)
    };
    // On the top line, a fifth of a millimetre below it: the straight edge
    // along it, not the round face whose outline the line is too; alone,
    // its length.
    let on_line = on_front(&doc, 20.0, 19.8);
    let picked = pick(&doc, on_line, json!({}));
    assert_eq!(picked["seen"], "line", "{picked}");
    assert_eq!(picked["pick"]["edge"], straight.as_str(), "{picked}");
    assert_eq!(picked["alone"], true, "{picked}");
    // On the round face's axis (z 17): the round face.
    let axis = pick(&doc, on_front(&doc, 20.0, 17.2), json!({}));
    assert_eq!(axis["seen"], "axis", "{axis}");
    assert_eq!(axis["pick"]["face"], "F2:side(c1[c4,c2])", "{axis}");
    // A radius wants the round face, on its outline too.
    let radius = pick(&doc, on_line, json!({"type": "radius"}));
    assert_eq!(radius["pick"]["face"], "F2:side(c1[c4,c2])", "{radius}");
    // The placing click's preview says what the dimension writes: the
    // value it gets.
    let placed = on_front(&doc, 20.0, 30.0);
    let snap = query(
        &doc,
        json!({"query": "drawing_snap", "free": true,
               "command": {"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                           "type": "dimension", "picks": [on_line], "at": placed}}),
    );
    assert_eq!(snap["type"], "linear", "{snap}");
    assert!(near(snap["value"].as_f64().unwrap(), 40.0), "{snap}");
    // Auto: the tube's length, as the preview said.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "dimension",
               "picks": [on_line], "at": placed}),
    );
    let entry = annotation(&doc, &added["annotation"]);
    assert_eq!(entry["broken"], false, "{entry}");
    assert_eq!(entry["type"], "linear", "{entry}");
    assert!(near(value(&entry), 40.0), "{entry}");
    assert_eq!(snap["label"], entry["label"], "{snap} {entry}");
    // The hover's outline (mitcad#1691): the visible lines the view draws
    // of the picked body, on the sheet, and its name.
    let outlined = pick(&doc, on_line, json!({"outline": true}));
    let outline = &outlined["outline"];
    assert!(
        outline["body"].as_str().is_some_and(|b| !b.is_empty()),
        "{outlined}"
    );
    let lines = outline["lines"].as_array().expect("the body's lines");
    assert!(!lines.is_empty(), "{outlined}");
    let (low, high) = (on_front(&doc, 0.0, 0.0), on_front(&doc, 40.0, 20.0));
    for p in lines.iter().flat_map(|l| l.as_array().unwrap()) {
        let p = point(p);
        assert!(
            p[0] >= low[0] - 1e-6
                && p[0] <= high[0] + 1e-6
                && p[1] >= low[1] - 1e-6
                && p[1] <= high[1] + 1e-6,
            "{p:?} is off the tube's front: {outlined}"
        );
    }
    // Without `outline`, none.
    assert!(picked.get("outline").is_none(), "{picked}");
}
