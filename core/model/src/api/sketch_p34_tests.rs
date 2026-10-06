// SPDX-License-Identifier: MIT
//! Sketch patterns bound to their originals, offsets of ellipses, splines
//! and mixed chains, and text profiles, with the mock kernel.

use std::f64::consts::PI;

use serde_json::{Value, json};

use crate::Document;
use crate::sketch::geometry::{Curve2, Nurbs};
use crate::testing::MockKernel;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn areas(doc: &Document<MockKernel>) -> Vec<f64> {
    let mut areas: Vec<f64> = sketch(doc)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    areas.sort_by(f64::total_cmp);
    areas
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in {}", s["entities"]))
        .clone()
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let p = entity(s, id)["at"].clone();
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    within(a, b, 1e-7)
}

fn within(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < tol
}

fn ellipse_center(s: &Value, id: &str) -> [f64; 2] {
    match curve(s, id) {
        Curve2::Ellipse { center, .. } => center,
        other => panic!("{id} is not an ellipse: {other:?}"),
    }
}

fn count_curves(s: &Value) -> usize {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] != "point")
        .count()
}

/// The solved curve of an entity of the `sketch` query.
fn curve(s: &Value, id: &str) -> Curve2 {
    let e = entity(s, id);
    let g = &e["geometry"];
    let p = |v: &Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
    let f = |v: &Value| v.as_f64().unwrap();
    match e["type"].as_str().unwrap() {
        "line" => Curve2::Line {
            a: p(&g["start"]),
            b: p(&g["end"]),
        },
        "circle" => Curve2::Circle {
            center: p(&g["center"]),
            radius: f(&g["radius"]),
        },
        "arc" => Curve2::Arc {
            center: p(&g["center"]),
            radius: f(&g["radius"]),
            start: f(&g["start_angle"]),
            end: f(&g["end_angle"]),
        },
        "ellipse" => Curve2::Ellipse {
            center: p(&g["center"]),
            major: f(&g["major_radius"]),
            minor: f(&g["minor_radius"]),
            rotation: f(&g["rotation"]),
        },
        _ => Curve2::Nurbs(Nurbs {
            degree: g["degree"].as_u64().unwrap() as usize,
            control: g["control"].as_array().unwrap().iter().map(p).collect(),
            weights: g["weights"].as_array().unwrap().iter().map(f).collect(),
            knots: g["knots"].as_array().unwrap().iter().map(f).collect(),
        }),
    }
}

/// The largest and smallest distance of points along `of` from `from`.
fn distances(of: &Curve2, from: &Curve2) -> (f64, f64) {
    let (lo, hi) = of.domain();
    let mut range = (f64::INFINITY, 0.0f64);
    for i in 0..=200 {
        let t = lo + (hi - lo) * i as f64 / 200.0;
        let (_, d) = from.closest(of.point(t));
        range = (range.0.min(d), range.1.max(d));
    }
    range
}

#[test]
fn circular_pattern_copies_follow_the_original() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    ); // p1
    // A 6 x 4 rectangle, c2..c5 on p6..p9, its first corner fixed (d1, d2).
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, -2],
               "width": 6, "height": 4}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1",
               "entities": ["c2", "c3", "c4", "c5"], "center": "p1", "count": 4}),
    );
    let pattern = made["constraints"][0].as_str().unwrap().to_owned();
    assert_eq!(made["made"].as_array().unwrap().len(), 12);
    assert_eq!(made["parameters"], json!(["d3"]), "the angle");
    assert_eq!(areas(&d).len(), 4);
    assert!(areas(&d).iter().all(|a| close(*a, 24.0, 1e-9)));
    let s = sketch(&d);
    assert_eq!(s["dof"], 0, "copies add no freedom");
    assert_eq!(s["redundant"], json!([]));
    assert_eq!(s["patterns"][0]["type"], "circular");
    assert_eq!(s["patterns"][0]["angle"], "d3");
    let copy = |s: &Value, k: usize, of: &str| -> String {
        s["patterns"][0]["copies"][k]["entities"][of]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // The first copy is a quarter turn round.
    let p6 = copy(&s, 0, "p6");
    assert!(near(at(&s, &p6), [2.0, 20.0]), "{:?}", at(&s, &p6));
    // The original changes: the copies follow.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 10}),
    );
    assert_eq!(areas(&d).len(), 4);
    assert!(areas(&d).iter().all(|a| close(*a, 40.0, 1e-9)));
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "p6", "to": [25, -2]}),
    );
    let s = sketch(&d);
    assert!(
        near(at(&s, &copy(&s, 1, "p6")), [-20.0, 2.0]),
        "fixed corner"
    );
    // More instances over half a turn: 36 degrees apart.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern, "count": 6,
               "angle": "180 deg"}),
    );
    let s = sketch(&d);
    assert_eq!(s["patterns"][0]["copies"].as_array().unwrap().len(), 5);
    assert_eq!(copy(&s, 0, "p6"), p6, "instances that stay keep their ids");
    let (sn, cs) = (PI / 5.0).sin_cos();
    assert!(near(
        at(&s, &p6),
        [20.0 * cs + 2.0 * sn, 20.0 * sn - 2.0 * cs]
    ));
    assert_eq!(areas(&d).len(), 6);
    // The angle is a parameter.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d3", "value": "90 deg"}),
    );
    let s = sketch(&d);
    let (sn, cs) = (PI / 10.0).sin_cos();
    assert!(near(
        at(&s, &p6),
        [20.0 * cs + 2.0 * sn, 20.0 * sn - 2.0 * cs]
    ));
    // A copy cannot be patterned, and its pattern is listed.
    let c = copy(&s, 0, "c2");
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": [c],
                   "center": "p1", "count": 3})
        )
        .contains("is a copy of a pattern")
    );
    // Removing an original takes its copies; the others stay bound.
    let before = count_curves(&sketch(&d));
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["c2"]}),
    );
    let s = sketch(&d);
    assert_eq!(count_curves(&s), before - 6);
    let dof = s["dof"].as_u64().unwrap();
    // Removing the pattern leaves the copies as free geometry.
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [pattern]}),
    );
    assert_eq!(removed["removed"], json!([pattern]));
    let s = sketch(&d);
    assert_eq!(count_curves(&s), before - 6);
    assert!(s["dof"].as_u64().unwrap() > dof);
    assert_eq!(s["patterns"], json!([]));
}

#[test]
fn rectangular_pattern_spacing_and_count_change_afterwards() {
    let mut d = with_sketch();
    // A circle of fixed centre (p2) and diameter d1.
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 4}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1"],
               "count": [3, 2], "spacing": [10, 8]}),
    );
    let pattern = made["constraints"][0].as_str().unwrap().to_owned();
    assert_eq!(made["parameters"], json!(["d2", "d3"]));
    assert_eq!(areas(&d).len(), 6);
    let s = sketch(&d);
    assert_eq!(s["dof"], 0);
    let copy_at = |s: &Value, index: [u32; 2]| -> String {
        s["patterns"][0]["copies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["index"] == json!(index))
            .unwrap()["entities"]["p2"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let kept = copy_at(&s, [1, 0]);
    assert!(near(at(&s, &copy_at(&s, [2, 1])), [20.0, 8.0]));
    // Four columns, the spacing an expression of the diameter.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "count": [4, 2], "spacing": [12, "d1 * 2"]}),
    );
    let s = sketch(&d);
    assert_eq!(areas(&d).len(), 8);
    assert_eq!(copy_at(&s, [1, 0]), kept);
    assert!(near(at(&s, &copy_at(&s, [3, 1])), [36.0, 8.0]));
    // The copies keep the original's size.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 3}),
    );
    assert!(areas(&d).iter().all(|a| close(*a, PI * 2.25, 1e-9)));
    assert!(near(at(&sketch(&d), &copy_at(&s, [3, 1])), [36.0, 6.0]));
    // A copy does not move by itself.
    let p = copy_at(&s, [1, 1]);
    let before = at(&sketch(&d), &p);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": [50, 50]}),
    );
    assert!(near(at(&sketch(&d), &p), before));
    // Fewer instances remove the copies of the rest.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "count": [2, 1]}),
    );
    assert_eq!(areas(&d).len(), 2);
    assert_eq!(count_curves(&sketch(&d)), 2);
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
                   "spacing": [0, 5]})
        )
        .contains("greater than zero")
    );
}

#[test]
fn mirrored_copies_follow_the_original() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, -20], "end": [0, 20],
               "construction": true}),
    ); // c1
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [10, 0],
               "radius": 3}),
    ); // c4 on p5
    command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": ["c4"], "axis": "c1"}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "c4", "radius": 5}),
    );
    assert!(areas(&d).iter().all(|a| close(*a, PI * 25.0, 1e-9)));
}

#[test]
fn ellipse_and_spline_offsets_follow_their_source() {
    // An ellipse offset out by d: the area grows by P d + pi d^2 (Steiner),
    // P the ellipse's perimeter.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 0], "major": [10, 0],
               "minor_radius": 5}),
    ); // c1
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1"], "distance": 2}),
    );
    let offset = made["constraints"][0].as_str().unwrap().to_owned();
    let result = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["derived"], true);
    assert_eq!(entity(&s, &result)["type"], "spline");
    // The query marks what the offset computes, its control points too.
    assert_eq!(entity(&s, &result)["derived"], true);
    let control = entity(&s, &result)["control"][1]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(entity(&s, &control)["derived"], true);
    assert!(entity(&s, "c1").get("derived").is_none());
    let (lo, hi) = distances(&curve(&s, &result), &curve(&s, "c1"));
    assert!(lo > 2.0 - 1e-4 && hi < 2.0 + 1e-4, "{lo} {hi}");
    let (a, b) = (10.0f64, 5.0f64);
    let h = ((a - b) / (a + b)).powi(2);
    let perimeter = PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
    let total = |d: &Document<MockKernel>| areas(d).iter().sum::<f64>();
    assert!(close(
        total(&d),
        PI * 50.0 + 2.0 * perimeter + 4.0 * PI,
        1e-5 * 50.0
    ));
    // The source changes: the offset follows.
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "minor_radius", "ellipse": "c1"}, "value": 4}),
    );
    let s = sketch(&d);
    let (lo, hi) = distances(&curve(&s, &result), &curve(&s, "c1"));
    assert!(lo > 2.0 - 1e-4 && hi < 2.0 + 1e-4, "{lo} {hi}");
    // The distance changes afterwards, and the side flips.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 1.5}),
    );
    let s = sketch(&d);
    let (lo, hi) = distances(&curve(&s, &result), &curve(&s, "c1"));
    assert!(lo > 1.5 - 1e-4 && hi < 1.5 + 1e-4, "{lo} {hi}");
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "flip": true}),
    );
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["id"], offset);
    assert_eq!(s["offsets"][0]["left"], true);
    let inner = s["offsets"][0]["results"][0].as_str().unwrap().to_owned();
    let (lo, hi) = distances(&curve(&s, &inner), &curve(&s, "c1"));
    assert!(lo > 1.5 - 1e-4 && hi < 1.5 + 1e-4, "{lo} {hi}");
    assert_eq!(areas(&d).len(), 2);
    assert!(close(areas(&d)[0] + areas(&d)[1], PI * 40.0, 1e-9));
    // Offset curves follow their source only: a drag of one of their
    // points moves the source by as much.
    let point = entity(&s, &inner)["control"][3]
        .as_str()
        .unwrap()
        .to_owned();
    let from = at(&s, &point);
    let before = ellipse_center(&s, "c1");
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": point, "to": [0, 0]}),
    );
    let s = sketch(&d);
    let center = ellipse_center(&s, "c1");
    let moved = [before[0] - from[0], before[1] - from[1]];
    assert!(within(center, moved, 1e-6), "{center:?} {moved:?}");
    assert!(within(at(&s, &point), [0.0, 0.0], 1e-6));
    let (lo, hi) = distances(&curve(&s, &inner), &curve(&s, "c1"));
    assert!(lo > 1.5 - 1e-4 && hi < 1.5 + 1e-4, "{lo} {hi}");
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [30, 30]}),
    );
    let free = sketch(&d)["entities"].as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                   "constraint": {"type": "coincident", "point": free, "entity": inner}})
        )
        .contains("follows an offset")
    );

    // An open spline through points, offset to its left.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1",
               "points": [[0, 0], [10, 6], [20, 4], [30, 10]]}),
    );
    let s = sketch(&d);
    let spline = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "fitted_spline")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [spline], "distance": -3}),
    );
    let result = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    let (lo, hi) = distances(&curve(&s, &result), &curve(&s, &spline));
    assert!(lo > 3.0 - 1e-4 && hi < 3.0 + 1e-4, "{lo} {hi}");
    // To the left of the spline's direction: above its start.
    let start = curve(&s, &result).point(0.0);
    assert!(start[0] < 0.0 && start[1] > 0.0, "{start:?}");
    // A fit point moves: the offset follows.
    let fit = entity(&s, &spline)["points"][2]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": fit, "to": [20, 1]}),
    );
    let s = sketch(&d);
    let (lo, hi) = distances(&curve(&s, &result), &curve(&s, &spline));
    assert!(lo > 3.0 - 1e-4 && hi < 3.0 + 1e-4, "{lo} {hi}");
    assert_eq!(s["dof"].as_u64().unwrap(), 8, "the source's freedom only");
    // Too far: the offset would fold over the bend.
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [spline], "distance": 40})
        )
        .contains("radius of curvature")
    );
}

#[test]
fn offset_chain_of_lines_and_a_spline_trims_and_rounds_corners() {
    // A convex outline: three lines and a spline arching over the top,
    // counter-clockwise.
    let mut d = with_sketch();
    let line = |d: &mut Document<MockKernel>, start: Value, end: Value| {
        command(
            d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
        )["made"][0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let bottom = line(&mut d, json!([0, 0]), json!([40, 0])); // c1: p2 -> p3
    let right = line(&mut d, json!("p3"), json!([40, 20])); // c4 -> p5
    let top = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1",
               "points": ["p5", [20, 28], [0, 20]]}),
    )["made"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let s = sketch(&d);
    let left_top = entity(&s, &top)["points"][2].as_str().unwrap().to_owned();
    let left = line(&mut d, json!(left_top), json!("p2"));
    let outline = areas(&d);
    assert_eq!(outline.len(), 1);
    let area = outline[0];
    // Outward: round corners, and the area grows by P d + pi d^2.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1",
               "curves": [bottom, right, top, left], "distance": 2}),
    );
    let offset = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["corners"].as_array().unwrap().len(), 4);
    let grown = |d: &Document<MockKernel>| areas(d).iter().sum::<f64>() - area;
    let p2 = (grown(&d) - 4.0 * PI) / 2.0;
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 4}),
    );
    let p4 = (grown(&d) - 16.0 * PI) / 4.0;
    assert!(close(p2, p4, 1e-4), "{p2} {p4}");
    // Inward: trimmed corners, no arcs.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "flip": true}),
    );
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["corners"], Value::Null);
    assert_eq!(areas(&d).len(), 2);
    let results: Vec<String> = s["offsets"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap().to_owned())
        .collect();
    // The region the offset bounds: trimmed corners leave a little more
    // than A - P d.
    let inner = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["loops"][0].as_array().unwrap().iter().all(|k| {
                let k = k.as_str().unwrap();
                results
                    .iter()
                    .any(|c| k == c || k.starts_with(&format!("{c}[")))
            })
        })
        .unwrap()["area"]
        .as_f64()
        .unwrap();
    assert!(
        inner > area - 4.0 * p2 && inner < area - 4.0 * p2 + 160.0,
        "{inner}"
    );
    // Every inner curve is 4 from its source.
    for (result, source) in results.iter().zip([&bottom, &right, &top, &left]) {
        let (lo, hi) = distances(&curve(&s, result), &curve(&s, source));
        assert!(lo > 4.0 - 1e-4 && hi < 4.0 + 1e-4, "{result}: {lo} {hi}");
    }
    // A drag of one offset curve moves the whole outline with it, and the
    // offset keeps its shape and distance.
    let start = |s: &Value, id: &str| curve(s, id).point(curve(s, id).domain().0);
    let before: Vec<[f64; 2]> = results.iter().map(|r| start(&s, r)).collect();
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": results[0], "by": [3, -1]}),
    );
    let s = sketch(&d);
    assert!(
        within(at(&s, "p2"), [3.0, -1.0], 1e-6),
        "{:?}",
        at(&s, "p2")
    );
    for (result, from) in results.iter().zip(&before) {
        let to = start(&s, result);
        assert!(
            within(to, [from[0] + 3.0, from[1] - 1.0], 1e-6),
            "{result}: {from:?} {to:?}"
        );
    }
    assert!(close(areas(&d).iter().sum::<f64>(), area, 1e-9));
    command(&mut d, json!({"cmd": "undo"}));
    assert!(near(at(&sketch(&d), "p2"), [0.0, 0.0]));
}

#[test]
fn dragging_or_moving_a_derived_offset_moves_its_source() {
    // An ellipse offset out by 2: its curve follows the pointer, carrying
    // the ellipse along; the distance stays.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 0], "major": [10, 0],
               "minor_radius": 5}),
    ); // c1
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1"], "distance": 2}),
    );
    let result = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    // The spline's start and end (shared), on the major axis.
    let end = entity(&s, &result)["control"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(near(at(&s, &end), [12.0, 0.0]), "{:?}", at(&s, &end));
    let check = |d: &Document<MockKernel>, center: [f64; 2], what: &str| {
        let s = sketch(d);
        let got = ellipse_center(&s, "c1");
        assert!(within(got, center, 1e-6), "{what}: centre {got:?}");
        assert_eq!(s["offsets"][0]["value"]["value"], 2.0, "{what}");
        let (lo, hi) = distances(&curve(&s, &result), &curve(&s, "c1"));
        assert!(lo > 2.0 - 1e-4 && hi < 2.0 + 1e-4, "{what}: {lo} {hi}");
    };

    // The curve dragged by an offset, then undone.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": result, "by": [5, 3]}),
    );
    check(&d, [5.0, 3.0], "curve dragged");
    assert!(within(at(&sketch(&d), &end), [17.0, 3.0], 1e-6));
    command(&mut d, json!({"cmd": "undo"}));
    check(&d, [0.0, 0.0], "drag undone");
    assert!(near(at(&sketch(&d), &end), [12.0, 0.0]));

    // Its end point dragged to a position: it gets there.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [13, -2]}),
    );
    check(&d, [1.0, -2.0], "point dragged");
    assert!(within(at(&sketch(&d), &end), [13.0, -2.0], 1e-6));
    command(&mut d, json!({"cmd": "undo"}));
    check(&d, [0.0, 0.0], "point drag undone");

    // Move: translated, and turned a quarter about the origin.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [result], "by": [-4, 0]}),
    );
    check(&d, [-4.0, 0.0], "moved");
    command(&mut d, json!({"cmd": "undo"}));
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [result],
               "rotate": {"center": [2, 0], "angle": PI / 2.0}}),
    );
    check(&d, [2.0, -2.0], "turned");
    let Curve2::Ellipse { rotation, .. } = curve(&sketch(&d), "c1") else {
        panic!("c1 is an ellipse");
    };
    assert!(close(rotation.rem_euclid(PI), PI / 2.0, 1e-9), "{rotation}");
    assert!(within(at(&sketch(&d), &end), [2.0, 10.0], 1e-6));
    command(&mut d, json!({"cmd": "undo"}));
    // The source and its offset moved together: once each.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": ["c1", result, end],
               "by": [0, 6]}),
    );
    check(&d, [0.0, 6.0], "moved with its source");
    command(&mut d, json!({"cmd": "undo"}));

    // A copy is an ordinary curve of the offset's shape: it stays when the
    // source moves.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [result], "by": [0, 30],
               "copy": true}),
    );
    let copy = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert!(entity(&s, &copy).get("derived").is_none());
    assert_eq!(s["offsets"][0]["results"], json!([result]));
    let copied = |s: &Value| curve(s, &copy).point(0.0);
    assert!(within(copied(&s), [12.0, 30.0], 1e-9), "{:?}", copied(&s));
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": result, "by": [5, 0]}),
    );
    check(&d, [5.0, 0.0], "dragged after the copy");
    assert!(within(copied(&sketch(&d)), [12.0, 30.0], 1e-9));
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));

    // A fixed source holds its offset too.
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["c1"]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": result, "by": [5, 3]}),
    );
    check(&d, [0.0, 0.0], "fixed source");
}

/// The region of the sketch with this key: area and centroid.
fn region(s: &Value, key: &str) -> (f64, [f64; 2]) {
    let r = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == key)
        .unwrap_or_else(|| panic!("no region {key} in {}", s["regions"]));
    let c = &r["centroid"];
    (
        r["area"].as_f64().unwrap(),
        [c[0].as_f64().unwrap(), c[1].as_f64().unwrap()],
    )
}

#[test]
fn text_glyphs_bound_regions_with_islands() {
    let mut d = with_sketch();
    // A 40 x 20 rectangle around "OB C" (the mock font's box letters: 5 x 7
    // at height 10, O with a 2 x 3 counter, B with two 2 x 2, C with a
    // parabolic top).
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 40, "height": 20}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "OB C", "at": [5, 5],
               "height": 10, "font": "Mock Sans"}),
    );
    assert_eq!(made["texts"], json!(["t9"]));
    let s = sketch(&d);
    let text = &s["texts"][0];
    assert_eq!(text["family"], "Mock Sans");
    assert_eq!(text["fallback"], false);
    let ids: Vec<&str> = text["outline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "t9.g0.c0", "t9.g0.c1", "t9.g1.c0", "t9.g1.c1", "t9.g1.c2", "t9.g3.c0"
        ]
    );
    let bump = 2.0 / 3.0 * 5.0 * 1.25;
    let mut expected = vec![
        800.0 - 3.0 * 35.0 - bump,
        29.0,
        6.0,
        27.0,
        4.0,
        4.0,
        35.0 + bump,
    ];
    expected.sort_by(f64::total_cmp);
    let actual = areas(&d);
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(&expected) {
        assert!(close(*a, *e, 1e-9), "{actual:?} != {expected:?}");
    }
    // The letters: O's ring and counter (an island), named by contour.
    let (ring, at) = region(&s, "r{t9.g0.c0}");
    assert!(
        close(ring, 29.0, 1e-9) && near(at, [8.5, 8.5]),
        "{ring} {at:?}"
    );
    let o = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "r{t9.g0.c0}")
        .unwrap()
        .clone();
    assert_eq!(o["text"], "t9");
    assert_eq!(o["letter"], true);
    assert_eq!(o["loops"], json!([["t9.g0.c0"], ["t9.g0.c1"]]));
    let counter = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "r{t9.g0.c1}")
        .unwrap()
        .clone();
    assert_eq!(counter["letter"], false);
    // The rectangle has the letters as holes.
    let outer = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["loops"].as_array().unwrap().len() == 4)
        .unwrap()
        .clone();
    assert_eq!(outer["loops"][1], json!(["t9.g0.c0"]));
    // A letter extrudes; its side faces are named by contour.
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{t9.g0.c0}"},
                            {"sketch": "F1", "region": "r{t9.g3.c0}"}],
               "extent": {"type": "distance", "distance": 2}, "operation": "new_body"}}),
    );
    // One body per letter; a face per piece of a contour (the mock does not
    // number repeated names).
    let faces = |body: &str| -> Vec<String> {
        d.body_shape(body.parse().unwrap())
            .unwrap()
            .faces
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    let o = faces("F2.b0");
    assert_eq!(o.iter().filter(|f| *f == "F2:side(t9.g0.c0)").count(), 4);
    assert!(o.iter().any(|f| f == "F2:side(t9.g0.c1)"), "{o:?}");
    assert!(faces("F2.b1").iter().any(|f| f == "F2:side(t9.g3.c0)"));
    // Another letter in the same place keeps the names; centring moves it.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t9", "text": "QB C",
               "align": "center"}),
    );
    let s = sketch(&d);
    let (_, at) = region(&s, "r{t9.g0.c0}");
    assert!(near(at, [8.5 - 12.25, 8.5]), "{at:?}");
    assert_eq!(
        d.recompute().error,
        None,
        "the extrusion still finds its letters"
    );
    // A file keeps texts, patterns and offsets.
    let json = d.to_json();
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(sketch(&loaded)["regions"], s["regions"]);
}

#[test]
fn text_in_a_frame_and_along_a_path() {
    let mut d = with_sketch();
    // A frame of construction lines; the text centred at its top.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "O", "height": 10,
               "frame": {"corner": [50, 0], "diagonal": [80, 20]},
               "align": "center", "valign": "top"}),
    );
    let t = made["texts"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    let frame = s["texts"][0]["frame"].clone();
    assert_eq!(frame.as_array().unwrap().len(), 3);
    assert_eq!(count_curves(&s), 4, "the frame's lines");
    let key = format!("r{{{t}.g0.c0}}");
    let (_, at) = region(&s, &key);
    assert!(near(at, [65.0, 16.0]), "{at:?}");
    // The frame grows: the text follows.
    let corner = frame[1].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": corner, "to": [90, 0]}),
    );
    let s = sketch(&d);
    let (_, at) = region(&s, &key);
    assert!(near(at, [70.0, 16.0]), "{at:?}");
    assert!(near(
        [
            s["texts"][0]["at"][0].as_f64().unwrap(),
            s["texts"][0]["at"][1].as_f64().unwrap()
        ],
        [50.0, 0.0]
    ));
    // Along a line: on it, or hanging below it.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 40], "end": [40, 40],
               "construction": true}),
    );
    let line = sketch(&d)["entities"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["type"] == "line")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "O", "height": 10,
               "path": {"curve": line}}),
    );
    let t = made["texts"][0].as_str().unwrap().to_owned();
    let key = format!("r{{{t}.g0.c0}}");
    let (_, at) = region(&sketch(&d), &key);
    assert!(near(at, [3.5, 43.5]), "{at:?}");
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": t, "align": "center",
               "path": {"curve": line, "above": false}}),
    );
    let (_, at) = region(&sketch(&d), &key);
    assert!(near(at, [20.0, 36.0]), "{at:?}");
    // Removing the path leaves the text where it is told to be.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [line]}),
    );
    assert_eq!(sketch(&d)["texts"][1]["path"], Value::Null);
}
