// SPDX-License-Identifier: MIT
//! Sketch patterns bound to their originals, offsets of ellipses, splines
//! and mixed chains, and text profiles, with the mock kernel.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::sketch::geometry::{Curve2, Nurbs};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

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
            start: 0.0,
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
        .contains("must not be zero")
    );
}

#[test]
fn a_negative_rectangular_pattern_spacing_goes_against_the_direction() {
    // mitcad#447: a negative spacing turns its direction; zero is refused.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 4}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1"],
               "count": [3, 2], "spacing": [-10, -8]}),
    );
    let pattern = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(s["dof"], 0);
    let copy_at = |s: &Value, index: [u32; 2]| -> [f64; 2] {
        let copy = s["patterns"][0]["copies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["index"] == json!(index))
            .unwrap();
        at(s, copy["entities"]["p2"].as_str().unwrap())
    };
    assert!(near(copy_at(&s, [1, 0]), [-10.0, 0.0]));
    assert!(near(copy_at(&s, [2, 1]), [-20.0, -8.0]));
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
                   "spacing": [0, 5]})
        )
        .contains("must not be zero")
    );
    // A symmetric direction gives the copies of the positive spacing.
    let mut symmetric = with_sketch();
    command(
        &mut symmetric,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 4}),
    );
    command(
        &mut symmetric,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1"],
               "count": [3, 1], "spacing": [-10, 10], "symmetric": [true, false]}),
    );
    let s = sketch(&symmetric);
    assert_eq!(s["dof"], 0);
    let mut xs: Vec<f64> = s["patterns"][0]["copies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| at(&s, c["entities"]["p2"].as_str().unwrap())[0])
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs.len(), 2);
    assert!(near([xs[0], xs[1]], [-10.0, 10.0]), "{xs:?}");
}

#[test]
fn a_symmetric_rectangular_pattern_has_the_originals_in_the_middle() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 4}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1"],
               "count": [5, 1], "spacing": [10, 10], "symmetric": [true, false]}),
    );
    let pattern = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(s["dof"], 0);
    let copies = s["patterns"][0]["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 4);
    // The copies lie 10 and 20 mm to each side of the original; the
    // pattern's index counts from the left end.
    let centre = |index: u32| -> [f64; 2] {
        let copy = copies
            .iter()
            .find(|c| c["index"] == json!([index, 0]))
            .unwrap();
        at(&s, copy["entities"]["p2"].as_str().unwrap())
    };
    for (index, x) in [(0, -20.0), (1, -10.0), (3, 10.0), (4, 20.0)] {
        assert!(
            near(centre(index), [x, 0.0]),
            "{index}: {:?}",
            centre(index)
        );
    }
    assert_eq!(s["patterns"][0]["symmetric"], json!([true, false]));
    // A new spacing keeps the symmetry.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "spacing": [6, 10]}),
    );
    let s = sketch(&d);
    let copies = s["patterns"][0]["copies"].as_array().unwrap();
    let far = copies.iter().find(|c| c["index"] == json!([4, 0])).unwrap();
    assert!(near(
        at(&s, far["entities"]["p2"].as_str().unwrap()),
        [12.0, 0.0]
    ));
    // An even count has no middle instance.
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1"],
                   "count": [4, 1], "spacing": [10, 10], "symmetric": [true, false]})
        )
        .contains("odd count")
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
fn offset_rebinding_keeps_shared_user_parameters() {
    check_offset_rebinding(json!("w"));
}

#[test]
fn offset_rebinding_keeps_shared_generated_parameters() {
    check_offset_rebinding(json!(2));
}

fn check_offset_rebinding(distance: Value) {
    for shape in ["rectangle", "circle", "arc"] {
        let mut d = with_sketch();
        for (name, value) in [("w", "2 mm"), ("t", "4 mm")] {
            command(
                &mut d,
                json!({"cmd": "add_parameter", "name": name, "value": value}),
            );
        }
        let source = match shape {
            "rectangle" => json!({"cmd": "sketch.add_rectangle", "sketch": "F1",
                                  "corner": [0, 0], "width": 40, "height": 20}),
            "circle" => json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
                               "center": [0, 0], "radius": 10}),
            _ => json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
                        "start": [10, 0], "through": [0, 10], "end": [-10, 0]}),
        };
        command(&mut d, source);
        if shape != "rectangle" {
            command(
                &mut d,
                json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["c1"]}),
            );
        }
        let curves = if shape == "rectangle" {
            json!(["c1", "c2", "c3", "c4"])
        } else {
            json!(["c1"])
        };
        let offset = command(
            &mut d,
            json!({"cmd": "sketch.offset", "sketch": "F1",
                                           "curves": curves, "distance": distance}),
        );
        let old = sketch(&d)["offsets"][0]["distance"]
            .as_str()
            .unwrap()
            .to_owned();
        let offset_id = sketch(&d)["offsets"][0]["id"].clone();
        let mut lines = Vec::new();
        for y in [100, 102] {
            let made = command(
                &mut d,
                json!({"cmd": "sketch.add_line", "sketch": "F1",
                                             "start": [100, y], "end": [102, y]}),
            );
            lines.push(made["made"][0].as_str().unwrap().to_owned());
        }
        command(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                               "constraint": {"type": "parallel", "a": lines[0], "b": lines[1]}}),
        );
        let mut unrelated = Vec::new();
        for dimension in [
            json!({"type": "length", "line": lines[0]}),
            json!({"type": "line_distance", "a": lines[0], "b": lines[1]}),
        ] {
            let made = command(
                &mut d,
                json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                                             "dimension": dimension, "value": old}),
            );
            unrelated.push(made["dimensions"][0].clone());
        }
        // Rebinding works for saved designs, without adding ownership fields.
        let mut d = Document::from_json(&d.to_json(), MockKernel::default()).unwrap();
        command(
            &mut d,
            json!({"cmd": "sketch.edit_offset", "sketch": "F1",
                               "offset": offset_id, "distance": "t"}),
        );
        command(
            &mut d,
            json!({"cmd": "set_parameter", "name": old, "value": 3}),
        );
        let s = sketch(&d);
        assert_eq!(s["offsets"][0]["distance"], "t");
        for dimension in s["dimensions"].as_array().unwrap() {
            if unrelated.contains(&dimension["id"]) {
                assert_eq!(dimension["value"], old);
                assert!(close(dimension["measured"].as_f64().unwrap(), 3.0, 1e-7));
            } else if offset["dimensions"]
                .as_array()
                .unwrap()
                .contains(&dimension["id"])
            {
                assert_eq!(dimension["value"], "t");
                assert!(close(dimension["measured"].as_f64().unwrap(), 4.0, 1e-7));
            }
        }
        let Curve2::Line { a, b } = curve(&s, &lines[0]) else {
            panic!("a line");
        };
        assert!(close((a[0] - b[0]).hypot(a[1] - b[1]), 3.0, 1e-7));
        if shape == "rectangle" {
            assert!(close(areas(&d).iter().sum(), 48.0 * 28.0, 1e-7));
        } else {
            let result = offset["made"][0].as_str().unwrap();
            let radius = match curve(&s, result) {
                Curve2::Circle { radius, .. } | Curve2::Arc { radius, .. } => radius,
                other => panic!("a circular offset: {other:?}"),
            };
            assert!(close(radius, 14.0, 1e-7), "{shape}: {radius}");
        }
    }
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
        assert!(within(got, center, 1e-6), "{what}: center {got:?}");
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

/// mitcad#1741: each region of the `sketch` query has its perimeter (the
/// selection card shows it): a 40 x 20 rectangle with a hole of diameter
/// 10 is 120 mm round plus the hole's circle, the disc its circle alone.
#[test]
fn sketch_regions_have_their_perimeters() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 40, "height": 20}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [20, 10], "diameter": 10}),
    );
    let s = sketch(&d);
    let mut perimeters: Vec<f64> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r["perimeter"]
                .as_f64()
                .unwrap_or_else(|| panic!("no perimeter: {r}"))
        })
        .collect();
    perimeters.sort_by(f64::total_cmp);
    let circle = 10.0 * std::f64::consts::PI;
    assert_eq!(perimeters.len(), 2, "{perimeters:?}");
    assert!((perimeters[0] - circle).abs() < 1e-6, "{perimeters:?}");
    assert!(
        (perimeters[1] - 120.0 - circle).abs() < 1e-6,
        "{perimeters:?}"
    );
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

/// A text that is measured as the `.f3d` files' texts are (mitcad#186):
/// `height` is the capitals' height, a frame's top is the capitals' top and
/// its bottom the baseline, lines align by their outlines, and a line
/// breaks at its spaces to fit the frame (the mock font's letters are 0.7
/// em high, 0.5 em wide with 0.1 em of side bearing, and advance 0.7 em).
#[test]
fn text_measured_by_its_capitals_and_outlines_in_a_frame() {
    let letters = |frame: Value, text: &str, extra: Value| {
        let mut d = with_sketch();
        let mut cmd = json!({"cmd": "sketch.add_text", "sketch": "F1", "text": text,
                             "height": 7, "frame": frame, "valign": "top",
                             "cap_height": true, "wrap": true});
        for (k, v) in extra.as_object().unwrap() {
            cmd[k] = v.clone();
        }
        let made = command(&mut d, cmd);
        let t = made["texts"][0].as_str().unwrap().to_owned();
        (sketch(&d), t)
    };
    let corner = |frame: [f64; 2]| json!({"corner": [0, 0], "diagonal": frame});

    // 7 mm capitals: the em is 10 mm. The first letter's outline touches
    // the frame's left side (not its pen position, 1 mm to the left) and
    // the capitals' top the frame's top.
    let (s, t) = letters(corner([40.0, 20.0]), "O", json!({}));
    let (_, at) = region(&s, &format!("r{{{t}.g0.c0}}"));
    assert!(near(at, [2.5, 16.5]), "{at:?}");

    // Aligned right by the outlines.
    let (s, t) = letters(corner([40.0, 20.0]), "O", json!({"align": "right"}));
    let (_, at) = region(&s, &format!("r{{{t}.g0.c0}}"));
    assert!(near(at, [37.5, 16.5]), "{at:?}");

    // Bottom: the baseline on the frame's bottom (the outline's bottom
    // for a letter without descender).
    let (s, t) = letters(corner([40.0, 20.0]), "O", json!({"valign": "bottom"}));
    let (_, at) = region(&s, &format!("r{{{t}.g0.c0}}"));
    assert!(near(at, [2.5, 3.5]), "{at:?}");

    // Two words that do not fit side by side: two lines, from the
    // ascender to the descender (10 em of 10 mm) apart, both on the left.
    let (s, t) = letters(corner([12.0, 30.0]), "O O", json!({}));
    let (_, first) = region(&s, &format!("r{{{t}.g0.c0}}"));
    let (_, second) = region(&s, &format!("r{{{t}.g2.c0}}"));
    assert!(near(first, [2.5, 26.5]), "{first:?}");
    assert!(near(second, [2.5, 16.5]), "{second:?}");

    // The same frame without wrapping: one line, the words side by side.
    let (s, t) = letters(corner([12.0, 30.0]), "O O", json!({"wrap": false}));
    let (_, second) = region(&s, &format!("r{{{t}.g2.c0}}"));
    assert!(near(second, [13.0, 26.5]), "{second:?}");

    // A word wider than the frame stays whole on its line.
    let (s, t) = letters(corner([4.0, 30.0]), "OO", json!({}));
    let (_, second) = region(&s, &format!("r{{{t}.g1.c0}}"));
    assert!(near(second, [9.5, 26.5]), "{second:?}");
}

/// A text measured as the `.ipt` files' texts are (mitcad#414): lines
/// align by their pens, the frame's top meets the first line's highest
/// outline point, and the lines are as far apart as the text says (the
/// mock font's `C` rises in an arc to 0.825 em, its control point at 0.95
/// em; its other letters are 0.7 em high, 0.5 em wide with 0.1 em of side
/// bearing and advance 0.7 em).
#[test]
fn text_aligned_by_its_pens_and_its_highest_outline_with_its_own_line_spacing() {
    let o_at = |extra: Value| {
        let mut d = with_sketch();
        let mut cmd = json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "C\nO",
                             "height": 7, "frame": {"corner": [0, 0], "diagonal": [40, 30]},
                             "valign": "top", "cap_height": true});
        for (k, v) in extra.as_object().unwrap() {
            cmd[k] = v.clone();
        }
        let made = command(&mut d, cmd);
        let t = made["texts"][0].as_str().unwrap().to_owned();
        region(&sketch(&d), &format!("r{{{t}.g2.c0}}")).1
    };
    // The em is 10 mm. The C's arc tops out at 8.25 mm: the first
    // baseline lies that far below the frame's top (30 - 8.25), the
    // second 12 mm below it, and the O's pen (not its outline, 1 mm to
    // the right) on the frame's left side.
    let at = o_at(json!({"pen_align": true, "ink_top": true, "line_spacing": 12}));
    assert!(near(at, [3.5, 13.25]), "{at:?}");
    // By the capitals' top and the outlines: the first baseline 7 mm below
    // the top and the O's outline on the frame's side.
    let at = o_at(json!({"line_spacing": 12}));
    assert!(near(at, [2.5, 14.5]), "{at:?}");
    // The font's spacing without a line spacing of its own: the ascender
    // to the descender, 10 mm.
    let at = o_at(json!({"pen_align": true, "ink_top": true}));
    assert!(near(at, [3.5, 15.25]), "{at:?}");
    // Centred by the pens: the line's advance (7 mm) about the middle.
    let at = o_at(
        json!({"pen_align": true, "ink_top": true, "line_spacing": 12,
                         "align": "center"}),
    );
    assert!(near(at, [20.0, 13.25]), "{at:?}");
}

/// The outlines a file stores with a text (mitcad#416), in units of the
/// height: an `A` that is a triangle 0.5 wide and 0.7 high with advance
/// 0.6, and a `B` that is a 0.4 x 0.7 box 0.1 from its pen, in a font
/// the computer does not have.
fn stored_ab(font: &str) -> Value {
    json!({"text": "AB", "font": font, "glyphs": [
        {"advance": 0.6, "contours": [[[[0, 0], [0.5, 0]], [[0.5, 0], [0.25, 0.7]],
                                       [[0.25, 0.7], [0, 0]]]]},
        {"advance": 0.5, "contours": [[[[0.1, 0], [0.5, 0]], [[0.5, 0], [0.5, 0.7]],
                                       [[0.5, 0.7], [0.1, 0.7]], [[0.1, 0.7], [0.1, 0]]]]}
    ]})
}

#[test]
fn a_text_whose_font_is_missing_keeps_its_stored_outlines_until_its_string_or_font_changes() {
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "AB", "at": [5, 5],
               "height": 10, "font": "No Such Font", "glyphs": stored_ab("No Such Font")}),
    );
    let t = made["texts"][0].as_str().unwrap().to_owned();
    let (a, b) = (format!("r{{{t}.g0.c0}}"), format!("r{{{t}.g1.c0}}"));
    let s = sketch(&d);
    let text = &s["texts"][0];
    assert_eq!(
        (
            text["family"].clone(),
            text["fallback"].clone(),
            text["stored_outlines"].clone(),
            text["has_glyphs"].clone(),
            text.get("glyphs").cloned()
        ),
        (
            json!("No Such Font"),
            json!(false),
            json!(true),
            json!(true),
            None
        )
    );
    // The stored shapes at 10 mm a unit: the triangle and the box, the box
    // 6 mm on.
    let (area, at) = region(&s, &a);
    assert!(
        close(area, 17.5, 1e-9) && near(at, [7.5, 5.0 + 7.0 / 3.0]),
        "{area} {at:?}"
    );
    let (area, at) = region(&s, &b);
    assert!(
        close(area, 28.0, 1e-9) && near(at, [14.0, 8.5]),
        "{area} {at:?}"
    );
    // The sketch has no warning for the font: the text has its own shapes.
    let timeline: Value =
        serde_json::from_str(&d.query(r#"{"query": "timeline"}"#).unwrap()).unwrap();
    assert!(
        !timeline.to_string().contains("not installed"),
        "{timeline}"
    );
    // Its layout changes with them: twice the height, by the capitals (the
    // unit is the height either way), and centred in a frame by the pens
    // with the frame's top at the highest outline point.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": t, "height": 20,
               "cap_height": true}),
    );
    let (area, at) = region(&sketch(&d), &b);
    assert!(
        close(area, 112.0, 1e-9) && near(at, [23.0, 12.0]),
        "{area} {at:?}"
    );
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "AB", "height": 10,
               "font": "No Such Font", "glyphs": stored_ab("No Such Font"),
               "frame": {"corner": [0, 0], "diagonal": [40, 30]}, "align": "center",
               "valign": "top", "cap_height": true, "pen_align": true, "ink_top": true}),
    );
    let t = made["texts"][0].as_str().unwrap().to_owned();
    // The line's 11 mm of advance about the frame's middle, its top 7 mm
    // above the baseline at the frame's top.
    let (_, at) = region(&sketch(&d), &format!("r{{{t}.g1.c0}}"));
    assert!(near(at, [14.5 + 6.0 + 3.0, 30.0 - 3.5]), "{at:?}");
    // Another string drops them: the substitute font, with its warning.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": t, "text": "BA"}),
    );
    let s = sketch(&d);
    let text = &s["texts"][0];
    assert_eq!(
        (
            text["family"].clone(),
            text["fallback"].clone(),
            text.get("stored_outlines").cloned(),
            text.get("has_glyphs").cloned()
        ),
        (json!("Mock Sans"), json!(true), None, None)
    );
    // An installed font is used even with stored outlines of it.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "AB", "at": [5, 5],
               "height": 10, "font": "Mock Sans", "glyphs": stored_ab("Mock Sans")}),
    );
    let text = &sketch(&d)["texts"][0];
    assert_eq!(
        (
            text["fallback"].clone(),
            text.get("stored_outlines").cloned(),
            text["has_glyphs"].clone()
        ),
        (json!(false), None, json!(true))
    );
    // Outlines that are not Bézier pieces are refused.
    let mut bad = stored_ab("No Such Font");
    bad["glyphs"][0]["contours"][0][0] = json!([[0, 0]]);
    let why = rejected(
        &mut with_sketch(),
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "AB", "at": [5, 5],
               "height": 10, "font": "No Such Font", "glyphs": bad}),
    );
    assert!(why.contains("stored glyphs"), "{why}");
}

// --- A derived offset is closed by shared points only (mitcad#360)

#[test]
fn a_chain_whose_ends_only_lie_together_stays_open_in_a_derived_offset() {
    let mut d = with_sketch();
    let spline = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 0], [10, 5], [20, 0]]}),
    );
    let c1 = spline["made"][0].as_str().unwrap().to_owned();
    let end = entity(&sketch(&d), &c1)["points"][2]
        .as_str()
        .unwrap()
        .to_owned();
    // A second curve from the spline's end back to the spline's start
    // position, but through a point of its own.
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": end, "end": [0.0, 0.0]}),
    );
    let line = line["made"][0].as_str().unwrap().to_owned();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [c1, line], "distance": 2}),
    );
    let offset = made["constraints"][0].as_str().unwrap().to_owned();
    let before = sketch(&d);
    assert_eq!(before["offsets"][0]["derived"], true, "{before}");
    // Another edit solves again: the chain is still open, so the offset
    // is made as before and the edit is not refused.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 1.5}),
    );
    let after = sketch(&d);
    assert_eq!(
        after["offsets"][0]["results"].as_array().unwrap().len(),
        2,
        "{after}"
    );
    assert_eq!(
        after["offsets"][0]["corners"], before["offsets"][0]["corners"],
        "{after}"
    );
    // The unjoined end is free: the spline's offset starts exactly the
    // distance from the spline's start, not trimmed against the line.
    let result = after["offsets"][0]["results"][0].as_str().unwrap();
    let first = entity(&after, result)["control"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let source_start = entity(&after, &c1)["points"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let start = at(&after, &source_start);
    let from = at(&after, &first);
    assert!(
        close((from[0] - start[0]).hypot(from[1] - start[1]), 1.5, 1e-6),
        "{after}"
    );
}

// --- A mirrored ellipse keeps its minor radius equal (mitcad#363)

#[test]
fn a_mirrored_ellipse_follows_the_minor_radius_of_the_original() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, -20], "end": [0, 20],
               "construction": true}),
    );
    let axis = "c1";
    let made = command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [20, 0], "major": [10, 0],
               "minor_radius": 5}),
    );
    let original = made["made"][0].as_str().unwrap().to_owned();
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [original], "axis": axis}),
    );
    let copy = mirrored["made"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "minor_radius", "ellipse": original}, "value": 4}),
    );
    let s = sketch(&d);
    let minor = |id: &str| match curve(&s, id) {
        Curve2::Ellipse { minor, .. } => minor,
        other => panic!("{id}: {other:?}"),
    };
    assert!(close(minor(&original), 4.0, 1e-9), "{s}");
    assert!(close(minor(&copy), 4.0, 1e-9), "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
}

// --- Flipping an exact offset leaves nothing behind (mitcad#358)

#[test]
fn flipping_an_exact_offset_of_an_arc_removes_what_it_made_before() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": [10, 0], "through": [0, 10], "end": [-10, 0]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1"], "distance": 5}),
    );
    let offset = sketch(&d)["offsets"][0]["id"].as_str().unwrap().to_owned();
    let counts = |s: &Value| {
        [
            s["entities"].as_array().unwrap().len(),
            s["constraints"].as_array().unwrap().len(),
            s["dimensions"].as_array().unwrap().len(),
            s["dof"].as_u64().unwrap() as usize,
        ]
    };
    let first = counts(&sketch(&d));
    let mut removed_by_flip = 0;
    for _ in 0..2 {
        let flipped = command(
            &mut d,
            json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "flip": true}),
        );
        removed_by_flip = flipped["removed"].as_array().map_or(0, Vec::len);
        let s = sketch(&d);
        assert_eq!(counts(&s), first, "{s}");
        assert_eq!(s["redundant"], json!([]), "{s}");
    }
    // The helper construction line, its two points, and what hung on them.
    assert!(removed_by_flip >= 3, "{removed_by_flip}");
}

// --- Trim and Extend against, and removing a part of, a derived offset
// (mitcad#361)

/// A spline with its derived offset by 2 and a vertical line across both
/// (c<line>); returns the document, the line and the offset's result.
fn spline_offset_and_a_line() -> (Document<MockKernel>, String, String) {
    let mut d = with_sketch();
    let spline = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 0], [10, 10], [20, 0]]}),
    );
    let spline = spline["made"][0].as_str().unwrap().to_owned();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [spline], "distance": 2}),
    );
    let result = made["made"][0].as_str().unwrap().to_owned();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, -10], "end": [10, 30]}),
    );
    (d, line["made"][0].as_str().unwrap().to_owned(), result)
}

#[test]
fn a_line_trimmed_at_a_derived_offset_gets_a_plain_end() {
    // The offset lies on one side of the spline: trimming from below and
    // from above, one of them ends at it.
    for at in [[10.0, -5.0], [10.0, 25.0]] {
        let (mut d, line, result) = spline_offset_and_a_line();
        command(
            &mut d,
            json!({"cmd": "sketch.trim", "sketch": "F1", "curve": line, "at": at}),
        );
        let s = sketch(&d);
        assert_eq!(s["error"], Value::Null, "{s}");
        assert_eq!(s["solved"], true, "{s}");
        // No constraint holds a point on the derived offset.
        let constraints = s["constraints"].as_array().unwrap();
        assert!(
            constraints.iter().all(|k| k["entity"] != result.as_str()),
            "{s}"
        );
    }
}

#[test]
fn a_line_extended_to_a_derived_offset_gets_a_plain_end() {
    // The offset by 2 to the right of the spline lies below it: a stub
    // under both, extended upward, reaches the offset first.
    let (mut d, _, result) = spline_offset_and_a_line();
    let stub = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [14, -10], "end": [14, -4]}),
    );
    let stub = stub["made"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.extend", "sketch": "F1", "curve": stub, "at": [14, -5]}),
    );
    let s = sketch(&d);
    assert_eq!(s["error"], Value::Null, "{s}");
    assert_eq!(s["solved"], true, "{s}");
    let end = entity(&s, &stub)["geometry"]["end"].clone();
    assert!(
        end[1].as_f64().unwrap() > 0.0,
        "the stub reached a curve: {s}"
    );
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["entity"] != result.as_str()),
        "no constraint holds a point on the derived offset: {s}"
    );
}

#[test]
fn a_part_of_a_derived_offset_cannot_be_removed_alone() {
    let (mut d, _, result) = spline_offset_and_a_line();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [result]}),
    );
    assert!(error.contains("follows offset k"), "{error}");
    // The source can go, and the offset with it.
    let s = sketch(&d);
    let source = s["offsets"][0]["curves"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [source]}),
    );
    assert_eq!(sketch(&d)["offsets"], json!([]));
}

/// Copies are bound through their neighbours (mitcad#677): every copy of a
/// 3 x 4 pattern, symmetric in the first direction, still follows the
/// originals at its own offset, and the copies add no freedom.
#[test]
fn copies_bound_through_their_neighbours_follow_the_originals() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [4, 1]}),
    ); // c1 on p2, p3
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [2, 5],
               "radius": 1}),
    ); // c4 on p5
    command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1", "c4"],
               "count": [3, 4], "spacing": [20, 15], "symmetric": [true, false]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "p5", "to": [3, 7]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "c4", "radius": 2}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "p3", "to": [6, -2]}),
    );
    let s = sketch(&d);
    assert_eq!(s["dof"], 7, "the line's 4 and the circle's 3");
    assert_eq!(s["redundant"], json!([]));
    let copies = s["patterns"][0]["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 11);
    let radius = |id: &str| match curve(&s, id) {
        Curve2::Circle { radius, .. } => radius,
        other => panic!("{id}: {other:?}"),
    };
    for copy in copies {
        let index = &copy["index"];
        let by = [
            20.0 * (index[0].as_f64().unwrap() - 1.0),
            15.0 * index[1].as_f64().unwrap(),
        ];
        for p in ["p2", "p3", "p5"] {
            let o = at(&s, p);
            let c = at(&s, copy["entities"][p].as_str().unwrap());
            assert!(near(c, [o[0] + by[0], o[1] + by[1]]), "{index} {p}: {c:?}");
        }
        let r = radius(copy["entities"]["c4"].as_str().unwrap());
        assert!(close(r, radius("c4"), 1e-9), "{index}: radius {r}");
    }
    assert!(close(radius("c4"), 2.0, 1e-9));
}
