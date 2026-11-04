// SPDX-License-Identifier: MIT
//! Sketch offsets leave out what vanishes at their distance (mitcad#1234),
//! with the mock kernel. A hexagon with 2 mm corner arcs projected from a
//! face, offset 2 mm inward, failed with "the offset is larger than an
//! arc's radius": the arcs collapse to their centres there. Now the arcs
//! whose offset radius is not above zero, and the curves whose offsets
//! turn inside out or fold over, are left out (the offset's `dropped`), and
//! the offsets of the curves on either side meet where they cross; where
//! those do not meet (a slot's parallel sides) the offset fails naming the
//! curve. An edit of the distance makes the offset again with the curves it
//! needs: kept curves keep their ids, a curve that comes back gets a new
//! one.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let p = &entity(s, id)["at"];
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

/// Whether the sketch has entity `id`.
fn exists(s: &Value, id: &str) -> bool {
    exists_in(&s["entities"], id)
}

/// Whether a list of entities, constraints or dimensions has `id`.
fn exists_in(list: &Value, id: &str) -> bool {
    list.as_array().unwrap().iter().any(|e| e["id"] == id)
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// The circumradius of the hexagon and its corner arcs' radius.
const HEX: f64 = 16.0;
const CORNER: f64 = 2.0;
/// The hexagon's centre on the face.
const MIDDLE: [f64; 2] = [56.0, 20.0];

/// A regular hexagon about [`MIDDLE`] with corners rounded by `corner`,
/// at z = 20, counter-clockwise: a side, then the arc at its end corner.
fn hexagon(corner: f64) -> Vec<Curve3> {
    let vertex = |k: usize| {
        let a = k as f64 * PI / 3.0;
        [MIDDLE[0] + HEX * a.cos(), MIDDLE[1] + HEX * a.sin()]
    };
    // A corner arc's centre lies inward of its vertex by r / sin 60.
    let center = |k: usize| {
        let a = k as f64 * PI / 3.0;
        let inward = corner / (PI / 3.0).sin();
        let v = vertex(k);
        [v[0] - inward * a.cos(), v[1] - inward * a.sin()]
    };
    let on = |k: usize, angle: f64| {
        let c = center(k);
        [c[0] + corner * angle.cos(), c[1] + corner * angle.sin()]
    };
    let mut out = Vec::new();
    for k in 0..6 {
        let next = (k + 1) % 6;
        let (a, b) = (k as f64 * PI / 3.0, next as f64 * PI / 3.0);
        let (s, e) = (on(k, a + PI / 6.0), on(next, b - PI / 6.0));
        out.push(Curve3::Line {
            start: [s[0], s[1], 20.0],
            end: [e[0], e[1], 20.0],
        });
        let c = center(next);
        out.push(Curve3::Conic {
            center: [c[0], c[1], 20.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: corner,
            minor: corner,
            start: b - PI / 6.0,
            end: b + PI / 6.0,
            closed: false,
        });
    }
    out
}

/// A 112 x 40 x 20 block and a sketch on its top face (F3) with
/// [`hexagon`] projected (linked, fixed); its twelve curves.
fn projected_hexagon() -> (Document<MockKernel>, Vec<String>) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 112, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    *d.kernel().curves.borrow_mut() = hexagon(CORNER);
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    let outline: Vec<String> = names(&projected["entities"])
        .into_iter()
        .filter(|e| e.starts_with('c'))
        .collect();
    assert_eq!(outline.len(), 12);
    (d, outline)
}

/// The area of the hexagon offset `d` inward: its sides' apothem less `d`,
/// its corners rounded by what is left of the arcs' radius.
fn inner_area(d: f64) -> f64 {
    area_inside(d, CORNER)
}

/// [`inner_area`] of a hexagon whose corner arcs have radius `corner`.
fn area_inside(d: f64, corner: f64) -> f64 {
    let apothem = HEX * (PI / 6.0).cos() - d;
    let sharp = 2.0 * 3f64.sqrt() * apothem * apothem;
    let rho = (corner - d).max(0.0);
    sharp - 6.0 * rho * rho * ((PI / 6.0).tan() - PI / 6.0)
}

fn has_area(s: &Value, area: f64) -> bool {
    s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| (r["area"].as_f64().unwrap() - area).abs() < 1e-6 * area)
}

/// The owner's case: the hexagon offset 1.5 mm inward keeps its arcs
/// (0.5 mm), 2 mm and 3 mm inward leave them out and the sides meet at
/// sharp corners. Each result is closed, a profile of the inner hexagon's
/// area, and the sketch fully constrained as its projected source is.
#[test]
fn a_hexagon_with_corner_arcs_offset_inward_past_their_radius_is_sharp() {
    for (distance, arcs) in [(1.5, 6), (2.0, 0), (3.0, 0)] {
        let (mut d, outline) = projected_hexagon();
        let made = command(
            &mut d,
            json!({"cmd": "sketch.offset", "sketch": "F3", "curves": outline,
                   "distance": -distance}),
        );
        let s = sketch(&d, "F3");
        let offset = &s["offsets"][0];
        let results = names(&offset["results"]);
        let dropped = names(&offset["dropped"]);
        assert_eq!(results.len(), 6 + arcs, "{distance}: {s}");
        assert_eq!(dropped.len(), 6 - arcs, "{distance}: {s}");
        for c in &dropped {
            assert_eq!(entity(&s, c)["type"], "arc", "{distance}: {s}");
        }
        let notes = names(&made["notes"]);
        if arcs == 0 {
            assert_eq!(notes.len(), 1, "{distance}: {made}");
            let note = &notes[0];
            assert!(
                note.starts_with("corner arcs ") && note.ends_with(" vanish at this distance"),
                "{distance}: {made}"
            );
        } else {
            assert!(notes.is_empty(), "{distance}: {made}");
        }
        // Closed: each corner point is shared by two results.
        let mut corners: Vec<String> = Vec::new();
        for r in &results {
            for end in ["start", "end"] {
                corners.push(entity(&s, r)[end].as_str().unwrap().to_owned());
            }
        }
        corners.sort();
        let shared = corners.len();
        corners.dedup();
        assert_eq!(corners.len() * 2, shared, "{distance}: {s}");
        assert!(has_area(&s, inner_area(distance)), "{distance}: {s}");
        assert_eq!(s["dof"], 0, "{distance}: {s}");
        assert_eq!(s["redundant"], json!([]), "{distance}: {s}");
        assert_eq!(s["warnings"], json!([]), "{distance}: {s}");
    }
}

/// An edit of the distance from 1 to 3 mm and back makes the offset again:
/// the sides' results keep their ids, the arcs' go at 3 mm and come back
/// with new ids at 1 mm; a driven dimension on a side's result stays, one
/// on an arc's result is removed with a warning naming it.
#[test]
fn a_distance_edited_past_the_arcs_radius_and_back_keeps_the_sides_ids() {
    let (mut d, outline) = projected_hexagon();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": outline, "distance": -1}),
    );
    let offset = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F3");
    let before = names(&s["offsets"][0]["results"]);
    assert_eq!(before.len(), 12, "{s}");
    let kind = |s: &Value, c: &str| entity(s, c)["type"].as_str().unwrap().to_owned();
    let sides: Vec<String> = before
        .iter()
        .filter(|c| kind(&s, c) == "line")
        .cloned()
        .collect();
    let arcs: Vec<String> = before
        .iter()
        .filter(|c| kind(&s, c) == "arc")
        .cloned()
        .collect();
    assert_eq!((sides.len(), arcs.len()), (6, 6), "{s}");
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "length", "line": sides[0]}, "driven": true}),
    );
    let length = added["dimensions"][0].as_str().unwrap().to_owned();
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "radius", "curve": arcs[0]}, "driven": true}),
    );
    let radius = added["dimensions"][0].as_str().unwrap().to_owned();

    // 3 mm: the arcs vanish, the sides keep their ids.
    let edited = command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F3", "offset": offset, "distance": 3}),
    );
    let s = sketch(&d, "F3");
    assert_eq!(names(&s["offsets"][0]["results"]), sides, "{s}");
    assert_eq!(names(&s["offsets"][0]["dropped"]).len(), 6, "{s}");
    assert_eq!(s["offsets"][0]["id"], offset.as_str(), "{s}");
    for arc in &arcs {
        assert!(!exists(&s, arc), "{arc}: {s}");
    }
    assert!(exists_in(&s["dimensions"], &length), "{s}");
    assert!(!exists_in(&s["dimensions"], &radius), "{s}");
    let warnings = names(&edited["warnings"]);
    let removed = format!("{radius} is removed");
    let warning = warnings.iter().find(|w| w.starts_with(&removed));
    assert!(warning.is_some_and(|w| w.contains(&arcs[0])), "{edited}");
    assert!(
        names(&edited["notes"]).iter().any(|n| n.contains("vanish")),
        "{edited}"
    );
    assert!(has_area(&s, inner_area(3.0)), "{s}");
    assert_eq!(s["dof"], 0, "{s}");

    // Back to 1 mm: the arcs come back with new ids, the sides keep theirs.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F3", "offset": offset, "distance": 1}),
    );
    let s = sketch(&d, "F3");
    let after = names(&s["offsets"][0]["results"]);
    assert_eq!(after.len(), 12, "{s}");
    assert!(names(&s["offsets"][0]["dropped"]).is_empty(), "{s}");
    let back_sides: Vec<String> = after
        .iter()
        .filter(|c| kind(&s, c) == "line")
        .cloned()
        .collect();
    let back_arcs: Vec<String> = after
        .iter()
        .filter(|c| kind(&s, c) == "arc")
        .cloned()
        .collect();
    assert_eq!(back_sides, sides, "{s}");
    assert_eq!(back_arcs.len(), 6, "{s}");
    assert!(back_arcs.iter().all(|c| !arcs.contains(c)), "{s}");
    assert!(has_area(&s, inner_area(1.0)), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
}

/// An open chain of a line, a 1 mm arc and a line, offset 2 mm into its
/// turn: the arc vanishes, the lines' offsets are cut where they cross,
/// and the chain's ends keep their places.
#[test]
fn an_open_chain_with_an_arc_that_collapses_is_trimmed_keeping_its_ends() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    // c1 from p2 to p3, c4 from p5 to p6.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [11, 1], "end": [11, 11]}),
    );
    let through = [10.0 + 0.5f64.sqrt(), 1.0 - 0.5f64.sqrt()];
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": "p3", "through": through, "end": "p5"}),
    );
    let arc = names(&arc["made"])
        .into_iter()
        .find(|c| c.starts_with('c'))
        .unwrap();
    // Into the turn: to the left of the chain from c1 on.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", arc, "c4"],
               "distance": -2}),
    );
    let s = sketch(&d, "F1");
    let offset = &s["offsets"][0];
    assert_eq!(names(&offset["dropped"]), vec![arc.clone()], "{s}");
    let results = names(&offset["results"]);
    assert_eq!(results.len(), 2, "{s}");
    assert!(
        names(&made["notes"]).iter().any(|n| n.contains(&arc)),
        "{made}"
    );
    let ends = |c: &str| {
        let e = entity(&s, c);
        (
            e["start"].as_str().unwrap().to_owned(),
            e["end"].as_str().unwrap().to_owned(),
        )
    };
    let (a0, a1) = ends(&results[0]);
    let (b0, b1) = ends(&results[1]);
    // They share the corner where their lines cross, (9, 2).
    let shared = [&a0, &a1]
        .into_iter()
        .find(|p| **p == b0 || **p == b1)
        .unwrap_or_else(|| panic!("no shared corner: {s}"));
    assert!(near(at(&s, shared), [9.0, 2.0]), "{s}");
    let points: Vec<[f64; 2]> = [&a0, &a1, &b0, &b1]
        .into_iter()
        .filter(|p| *p != shared)
        .map(|p| at(&s, p))
        .collect();
    assert!(
        points.iter().any(|p| near(*p, [0.0, 2.0])),
        "{points:?}: {s}"
    );
    assert!(
        points.iter().any(|p| near(*p, [9.0, 11.0])),
        "{points:?}: {s}"
    );
}

/// A slot offset inward past its end arcs' radius: the arcs vanish, and
/// its parallel sides' offsets never meet, so the offset fails naming the
/// arc; outward it is made.
#[test]
fn a_slot_offset_inward_past_its_arcs_fails_naming_the_arc() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "center_to_center",
               "a": [0, 0], "b": [20, 0], "width": 4}),
    );
    let s = sketch(&d, "F1");
    let outline: Vec<String> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| (e["type"] == "line" || e["type"] == "arc") && e["construction"] != true)
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(outline.len(), 4, "{s}");
    let arcs: Vec<&String> = outline
        .iter()
        .filter(|c| entity(&s, c)["type"] == "arc")
        .collect();
    let mut failed = Vec::new();
    let mut made = 0;
    for distance in [3.0, -3.0] {
        let offset = json!({"cmd": "sketch.offset", "sketch": "F1", "curves": outline,
                            "distance": distance});
        match d.command(&offset.to_string()) {
            Ok(_) => made += 1,
            Err(e) => failed.push(e.to_string()),
        }
    }
    assert_eq!((made, failed.len()), (1, 1), "{failed:?}");
    let message = &failed[0];
    assert!(
        message.contains("vanish") && message.contains("do not meet"),
        "{message}"
    );
    let vanishing: Vec<String> = arcs.iter().map(|a| format!("{a} vanishes")).collect();
    assert!(vanishing.iter().any(|v| message.contains(v)), "{message}");
}

/// A chain of two lines and a tight spline between them, offset 3 mm into
/// the V they make: the spline's offset folds over and loops behind the
/// lines', so it is left out and the lines' offsets meet where they cross,
/// (0, 3 sqrt 2). At 0.1 mm the spline's offset comes back, a new curve,
/// and the lines' keep their ids.
#[test]
fn a_spline_chain_whose_offset_loops_is_trimmed() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    // c1 from p2 to p3, c4 from p5 to p6, then the spline from p3 to p5.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-10, 10], "end": [-1, 1]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [1, 1], "end": [10, 10]}),
    );
    let spline = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": ["p3", [0, 0], "p5"]}),
    );
    let spline = names(&spline["made"])
        .into_iter()
        .find(|c| c.starts_with('c'))
        .unwrap();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", spline, "c4"],
               "distance": -3}),
    );
    let offset = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    assert_eq!(s["offsets"][0]["derived"], true, "{s}");
    assert_eq!(
        names(&s["offsets"][0]["dropped"]),
        vec![spline.clone()],
        "{s}"
    );
    let lines = names(&s["offsets"][0]["results"]);
    assert_eq!(lines.len(), 2, "{s}");
    let end = entity(&s, &lines[0])["end"].as_str().unwrap().to_owned();
    assert_eq!(entity(&s, &lines[1])["start"], end.as_str(), "{s}");
    assert!(near(at(&s, &end), [0.0, 3.0 * 2f64.sqrt()]), "{s}");
    assert!(
        names(&made["notes"]).iter().any(|n| n.contains(&spline)),
        "{made}"
    );

    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 0.1}),
    );
    let s = sketch(&d, "F1");
    let results = names(&s["offsets"][0]["results"]);
    assert_eq!(results.len(), 3, "{s}");
    assert!(names(&s["offsets"][0]["dropped"]).is_empty(), "{s}");
    assert_eq!((&results[0], &results[2]), (&lines[0], &lines[1]), "{s}");
    assert!(!lines.contains(&results[1]), "{s}");
    assert_eq!(entity(&s, &results[1])["type"], "spline", "{s}");
}

/// A curve's start and end points.
fn ends_of(s: &Value, c: &str) -> [Value; 2] {
    let e = entity(s, c);
    [e["start"].clone(), e["end"].clone()]
}

/// The lines and the arcs of a list of curves, each in its order.
fn lines_and_arcs(s: &Value, curves: &[String]) -> (Vec<String>, Vec<String>) {
    let of = |kind: &str| -> Vec<String> {
        curves
            .iter()
            .filter(|c| entity(s, c)["type"] == kind)
            .cloned()
            .collect()
    };
    (of("line"), of("arc"))
}

/// The feature's information (`infos`), as the timeline shows it.
fn infos(doc: &Document<MockKernel>, uid: &str) -> Vec<String> {
    let feature: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    names(&feature["infos"])
}

/// The offset's distance, a parameter of the sketch, set from 1 to 3 and
/// back to 1 through the parameters (not an edit of the sketch): the
/// sketch's evaluation makes the offset again with the curves it needs, the
/// arcs vanish and come back, the sides keep their ids, and the feature's
/// information names the arcs that vanish.
#[test]
fn a_distance_parameter_set_outside_the_sketch_makes_the_offset_again() {
    let (mut d, outline) = projected_hexagon();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": outline, "distance": -1}),
    );
    let param = made["parameters"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F3");
    let (sides, arcs) = lines_and_arcs(&s, &names(&s["offsets"][0]["results"]));
    assert_eq!((sides.len(), arcs.len()), (6, 6), "{s}");

    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": param, "value": 3}),
    );
    let s = sketch(&d, "F3");
    assert_eq!(names(&s["offsets"][0]["results"]), sides, "{s}");
    assert_eq!(names(&s["offsets"][0]["dropped"]).len(), 6, "{s}");
    for arc in &arcs {
        assert!(!exists(&s, arc), "{arc}: {s}");
    }
    assert!(has_area(&s, inner_area(3.0)), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    let told = infos(&d, "F3");
    assert!(told.iter().any(|i| i.contains("vanish")), "{told:?}");

    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": param, "value": 1}),
    );
    let s = sketch(&d, "F3");
    let results = names(&s["offsets"][0]["results"]);
    let (back_sides, back_arcs) = lines_and_arcs(&s, &results);
    assert_eq!(back_sides, sides, "{s}");
    assert_eq!(back_arcs.len(), 6, "{s}");
    assert!(names(&s["offsets"][0]["dropped"]).is_empty(), "{s}");
    assert!(has_area(&s, inner_area(1.0)), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
}

/// The projected source moves on a recompute: the face's corners are
/// rounded by 1 mm instead of 2 mm, so that the offset 1.5 mm inward loses
/// its arcs. The sketch's evaluation makes it again, the sides keeping
/// their ids, without an edit of the sketch.
#[test]
fn a_projected_source_that_moves_makes_the_offset_again() {
    let (mut d, outline) = projected_hexagon();
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": outline, "distance": -1.5}),
    );
    let s = sketch(&d, "F3");
    let (sides, arcs) = lines_and_arcs(&s, &names(&s["offsets"][0]["results"]));
    assert_eq!((sides.len(), arcs.len()), (6, 6), "{s}");
    // The block 1 mm wider: its top face's outline (the mock kernel's
    // curves) now has 1 mm corners, and the linked projection follows it.
    *d.kernel().curves.borrow_mut() = hexagon(1.0);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 113}),
    );
    let s = sketch(&d, "F3");
    assert_eq!(names(&s["offsets"][0]["results"]), sides, "{s}");
    assert_eq!(names(&s["offsets"][0]["dropped"]).len(), 6, "{s}");
    for arc in &arcs {
        assert!(!exists(&s, arc), "{arc}: {s}");
    }
    assert!(has_area(&s, area_inside(1.5, 1.0)), "{s}");
}

/// A loop away from the joins (Wanted 1): a square whose corner is cut by a
/// neck 1.41 mm wide and 4.2 mm deep along the diagonal, offset 1 mm out.
/// The neck's bottom turns inside out, and its walls' offsets pass each
/// other: they lie wholly nearer the source than the distance, so they go
/// too, and the sides' offsets meet at the square's corner (11, 11).
#[test]
fn the_walls_of_a_neck_narrower_than_twice_the_distance_are_trimmed_away() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    // c1 (p2 to p3), c4 (to p5), c6 (to p7), c8 (to p9), c10 (to p11), c12
    // (to p13), c14 back to p2: counter-clockwise.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    for (start, end) in [
        ("p3", [10.0, 9.0]),
        ("p5", [7.0, 6.0]),
        ("p7", [6.0, 7.0]),
        ("p9", [9.0, 10.0]),
        ("p11", [0.0, 10.0]),
    ] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
        );
    }
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p13", "end": "p2"}),
    );
    let curves = ["c1", "c4", "c6", "c8", "c10", "c12", "c14"];
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": curves, "distance": 1}),
    );
    let s = sketch(&d, "F1");
    let offset = &s["offsets"][0];
    assert_eq!(names(&offset["dropped"]), ["c6", "c8", "c10"], "{s}");
    let results = names(&offset["results"]);
    assert_eq!(results.len(), 4, "{s}");
    assert!(
        names(&made["notes"]).iter().any(|n| n.contains("c8")),
        "{made}"
    );
    // The right side's offset and the top's share their corner.
    let (right, top) = (ends_of(&s, &results[1]), ends_of(&s, &results[2]));
    let corner = right
        .iter()
        .find(|p| top.contains(p))
        .and_then(|p| p.as_str())
        .unwrap_or_else(|| panic!("no shared corner: {s}"));
    assert!(near(at(&s, corner), [11.0, 11.0]), "{s}");
}
