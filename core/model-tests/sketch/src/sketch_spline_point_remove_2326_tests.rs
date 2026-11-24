// SPDX-License-Identifier: MIT
//! Remove Point on a spline (mitcad#2326, `sketch.remove_spline_point`),
//! the pair of Insert Point (mitcad#836): a fit point or a control point
//! leaves the spline, which stays with one point fewer; an end point makes
//! the next point the end; inserting a point and removing it again gives
//! back the curve; the refusals; one undo step that restores the spline.
//! With the mock kernel.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::sketch::geometry::Nurbs;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const FREED: &str = "the pattern's copies are now plain sketch geometry";

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
    let answer = doc
        .query(&json!({"query": "sketch", "uid": "F1"}).to_string())
        .unwrap();
    let s: Value = serde_json::from_str(&answer).unwrap();
    assert!(s["error"].is_null(), "{s}");
    s
}

/// A new document with an empty Sketch1 (F1).
fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

/// The sketch query's entry of an entity.
fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

/// Whether the sketch has the entity.
fn has(s: &Value, id: &str) -> bool {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == id)
}

/// A spline through the points, or by them as control points with a
/// degree; returns its curve id.
fn spline(
    d: &mut Document<MockKernel>,
    points: Value,
    degree: Option<u32>,
    closed: bool,
) -> String {
    let mut cmd =
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": points, "closed": closed});
    if let Some(degree) = degree {
        cmd["degree"] = json!(degree);
    }
    let made = command(d, cmd);
    made["made"][0].as_str().unwrap().to_owned()
}

/// Inserts a point on `curve` at `at`; returns the new point.
fn insert(d: &mut Document<MockKernel>, curve: &str, at: [f64; 2]) -> String {
    let made = command(
        d,
        json!({"cmd": "sketch.insert_spline_point", "sketch": "F1", "curve": curve, "at": at}),
    );
    made["made"][0].as_str().unwrap().to_owned()
}

/// The command that removes `point` from `curve`.
fn removal(curve: &str, point: &str) -> Value {
    json!({"cmd": "sketch.remove_spline_point", "sketch": "F1", "curve": curve, "point": point})
}

/// Removes `point` from `curve`; the result.
fn remove(d: &mut Document<MockKernel>, curve: &str, point: &str) -> Value {
    command(d, removal(curve, point))
}

/// The removal refused: its reason.
fn refused(d: &mut Document<MockKernel>, curve: &str, point: &str) -> String {
    rejected(d, removal(curve, point))
}

/// What a result lists as removed.
fn removed(result: &Value) -> Vec<String> {
    result["removed"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn shown_notes(result: &Value) -> Vec<String> {
    result["note_messages"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// The point ids a spline lists: its fit points or its control points.
fn ids(s: &Value, curve: &str) -> Vec<String> {
    let e = entity(s, curve);
    let list = if e["type"] == "fitted_spline" {
        &e["points"]
    } else {
        &e["control"]
    };
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

/// The list without the entry `index`.
fn without(list: &[String], index: usize) -> Vec<String> {
    let mut list = list.to_vec();
    list.remove(index);
    list
}

/// A position in the sketch query.
fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

/// A point's solved position.
fn at(s: &Value, point: &str) -> [f64; 2] {
    xy(&entity(s, point)["at"])
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The solved curve of a spline.
fn nurbs(s: &Value, curve: &str) -> Nurbs {
    let g = &entity(s, curve)["geometry"];
    let numbers = |v: &Value| -> Vec<f64> {
        v.as_array()
            .map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect())
            .unwrap_or_default()
    };
    Nurbs {
        degree: g["degree"].as_u64().unwrap() as usize,
        control: g["control"].as_array().unwrap().iter().map(xy).collect(),
        weights: numbers(&g["weights"]),
        knots: numbers(&g["knots"]),
    }
}

/// The parameters of a fitted spline's fit points (chord lengths over the
/// domain; a closed one's first point again last).
fn fit_params(s: &Value, curve: &str, closed: bool) -> Vec<f64> {
    let mut fit: Vec<[f64; 2]> = ids(s, curve).iter().map(|p| at(s, p)).collect();
    if closed {
        fit.push(fit[0]);
    }
    let mut lengths = vec![0.0];
    let mut total = 0.0;
    for w in fit.windows(2) {
        total += dist(w[0], w[1]);
        lengths.push(total);
    }
    let (lo, hi) = nurbs(s, curve).domain();
    lengths.iter().map(|l| lo + (hi - lo) * l / total).collect()
}

/// The curve passes through every fit point, at its parameter.
fn passes_through_fit_points(s: &Value, curve: &str, closed: bool) {
    let n = nurbs(s, curve);
    let mut points = ids(s, curve);
    if closed {
        points.push(points[0].clone());
    }
    for (p, t) in points.iter().zip(fit_params(s, curve, closed)) {
        let d = dist(n.eval(t)[0], at(s, p));
        assert!(d < 1e-6, "{p} at {t}: {d}");
    }
}

/// The curve is the same at sampled parameters, within 1e-6 mm.
fn same_shape(before: &Nurbs, after: &Nurbs) {
    assert_eq!(before.domain(), after.domain());
    let (lo, hi) = before.domain();
    for i in 0..=100 {
        let t = lo + (hi - lo) * i as f64 / 100.0;
        let d = dist(before.eval(t)[0], after.eval(t)[0]);
        assert!(d < 1e-6, "at {t}: {d}");
    }
}

/// One undo step takes the removal back: the sketch's entities, the
/// spline's points and their places, constraints and dimensions as they
/// were before it.
fn undo_restores(d: &mut Document<MockKernel>, before: &Value) {
    command(d, json!({"cmd": "undo"}));
    let s = sketch(d);
    assert_eq!(s["entities"], before["entities"]);
    assert_eq!(s["constraints"], before["constraints"]);
    assert_eq!(s["dimensions"], before["dimensions"]);
}

#[test]
fn a_fit_point_leaves_an_open_spline_which_stays() {
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([[0, 0], [20, 10], [40, 0], [60, 10], [80, 0]]),
        None,
        false,
    );
    let before = sketch(&d);
    let points = ids(&before, &c);
    let result = remove(&mut d, &c, &points[2]);
    assert_eq!(d.undo_label(), Some("Remove Spline Point in Sketch1"));
    assert!(removed(&result).contains(&points[2]), "{result}");
    let s = sketch(&d);
    assert_eq!(entity(&s, &c)["type"], "fitted_spline");
    assert_eq!(ids(&s, &c), without(&points, 2));
    assert!(!has(&s, &points[2]));
    // The other points stay where they were; the curve passes through them.
    for p in without(&points, 2) {
        assert!(dist(at(&s, &p), at(&before, &p)) < 1e-9, "{p}");
    }
    passes_through_fit_points(&s, &c, false);
    undo_restores(&mut d, &before);
}

#[test]
fn an_end_point_of_an_open_fitted_spline_makes_the_next_point_the_end() {
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([[0, 0], [20, 10], [40, 0], [60, 10]]),
        None,
        false,
    );
    let points = ids(&sketch(&d), &c);
    // A line tangent to the spline at its start, which it does not share.
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-30, -12], "end": [-6, -3]}),
    );
    let l = line["made"][0].as_str().unwrap().to_owned();
    let tangent = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "tangent", "a": c, "b": l, "at": points[0]}}),
    );
    let k = tangent["constraints"][0].as_str().unwrap().to_owned();
    let before = sketch(&d);
    let result = remove(&mut d, &c, &points[0]);
    // The tangent at the removed end goes with it; the line stays.
    assert!(removed(&result).contains(&k), "{result}");
    let s = sketch(&d);
    assert_eq!(ids(&s, &c), without(&points, 0));
    assert!(has(&s, &l));
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["id"] != k.as_str()),
        "{s}"
    );
    // The curve starts at the next point now.
    let n = nurbs(&s, &c);
    let (lo, hi) = n.domain();
    assert!(dist(n.eval(lo)[0], at(&s, &points[1])) < 1e-6);
    assert!(dist(n.eval(hi)[0], at(&s, &points[3])) < 1e-6);
    passes_through_fit_points(&s, &c, false);
    undo_restores(&mut d, &before);
    // And the last point: the one before it becomes the end.
    remove(&mut d, &c, &points[3]);
    let s = sketch(&d);
    assert_eq!(ids(&s, &c), without(&points, 3));
    let n = nurbs(&s, &c);
    assert!(dist(n.eval(n.domain().1)[0], at(&s, &points[2])) < 1e-6);
}

#[test]
fn a_fit_point_leaves_a_closed_spline() {
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([[0, 0], [40, -10], [80, 0], [60, 15], [40, 12]]),
        None,
        true,
    );
    let before = sketch(&d);
    let points = ids(&before, &c);
    // The first point too: a closed spline has no end.
    remove(&mut d, &c, &points[0]);
    let s = sketch(&d);
    assert_eq!(entity(&s, &c)["closed"], true);
    assert_eq!(ids(&s, &c), without(&points, 0));
    passes_through_fit_points(&s, &c, true);
    let n = nurbs(&s, &c);
    let (lo, hi) = n.domain();
    assert!(dist(n.eval(lo)[0], n.eval(hi)[0]) < 1e-9, "still closed");
    remove(&mut d, &c, &points[3]);
    let s = sketch(&d);
    assert_eq!(
        ids(&s, &c),
        vec![points[1].clone(), points[2].clone(), points[4].clone()]
    );
    passes_through_fit_points(&s, &c, true);
    // Three are the least a closed spline keeps.
    let error = refused(&mut d, &c, &points[1]);
    assert!(error.contains("needs 3"), "{error}");
    command(&mut d, json!({"cmd": "undo"}));
    undo_restores(&mut d, &before);
}

#[test]
fn a_control_point_leaves_an_open_spline() {
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([
            [0, 0],
            [10, 25],
            [30, 30],
            [45, -5],
            [60, 10],
            [75, 0],
            [90, 20]
        ]),
        Some(3),
        false,
    );
    let before = sketch(&d);
    let shape = nurbs(&before, &c);
    let points = ids(&before, &c);
    remove(&mut d, &c, &points[3]);
    assert_eq!(d.undo_label(), Some("Remove Spline Point in Sketch1"));
    let s = sketch(&d);
    let e = entity(&s, &c);
    assert_eq!(e["type"], "spline");
    assert_eq!(ids(&s, &c), without(&points, 3));
    assert!(!has(&s, &points[3]));
    // One knot fewer, written out with the control points.
    assert_eq!(e["knots"].as_array().unwrap().len(), 6 + 4);
    let after = nurbs(&s, &c);
    assert_eq!(after.control.len(), 6);
    assert_eq!(after.domain(), shape.domain());
    // The ends stay, the curve still starts and ends there.
    let (lo, hi) = after.domain();
    for (p, t) in [(&points[0], lo), (&points[6], hi)] {
        assert!(dist(at(&s, p), at(&before, p)) < 1e-9, "{p}");
        assert!(dist(after.eval(t)[0], at(&s, p)) < 1e-9, "{p}");
    }
    undo_restores(&mut d, &before);
    // The first control point: the next one becomes the start, no other
    // point moves.
    remove(&mut d, &c, &points[0]);
    let s = sketch(&d);
    assert_eq!(ids(&s, &c), without(&points, 0));
    for p in without(&points, 0) {
        assert!(dist(at(&s, &p), at(&before, &p)) < 1e-9, "{p}");
    }
    let after = nurbs(&s, &c);
    assert!(dist(after.eval(after.domain().0)[0], at(&s, &points[1])) < 1e-9);
    undo_restores(&mut d, &before);
}

#[test]
fn a_control_point_leaves_a_closed_spline() {
    let mut d = with_sketch();
    let point = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0]}),
    );
    let p = point["entities"][0].as_str().unwrap().to_owned();
    // A spline by control points whose ends are one point, smooth there.
    let c = spline(
        &mut d,
        json!([p, [30, -5], [40, 20], [5, 18], [-10, 8], [-8, -6], p]),
        Some(3),
        false,
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "tangent", "a": c, "b": c, "at": p}}),
    );
    let before = sketch(&d);
    let points = ids(&before, &c);
    remove(&mut d, &c, &points[3]);
    let s = sketch(&d);
    let now = ids(&s, &c);
    assert_eq!(now, without(&points, 3));
    // Still closed at the one point, with its tangent constraint.
    assert_eq!((now.first(), now.last()), (Some(&p), Some(&p)));
    assert_eq!(s["constraints"], before["constraints"]);
    let n = nurbs(&s, &c);
    let (lo, hi) = n.domain();
    assert!(dist(n.eval(lo)[0], n.eval(hi)[0]) < 1e-9);
    // The point where it closes cannot go.
    let error = refused(&mut d, &c, &p);
    assert!(error.contains("closes"), "{error}");
    undo_restores(&mut d, &before);
}

#[test]
fn a_point_inserted_and_removed_gives_back_the_curve() {
    // A spline by control points, open: the knot inserted goes again.
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([[0, 0], [10, 25], [30, 30], [45, -5], [60, 10]]),
        Some(3),
        false,
    );
    for t in [0.2, 0.37, 0.5, 0.9] {
        let s = sketch(&d);
        let shape = nurbs(&s, &c);
        let listed = ids(&s, &c);
        let p = insert(&mut d, &c, shape.eval(t)[0]);
        assert_eq!(ids(&sketch(&d), &c).len(), listed.len() + 1);
        remove(&mut d, &c, &p);
        let s = sketch(&d);
        assert_eq!(ids(&s, &c), listed, "{t}");
        let after = nurbs(&s, &c);
        same_shape(&shape, &after);
        assert_eq!(after.knots.len(), shape.knots.len());
        for (a, b) in after.knots.iter().zip(&shape.knots) {
            assert!((a - b).abs() < 1e-12, "{t}: {a} {b}");
        }
        for (a, b) in after.control.iter().zip(&shape.control) {
            assert!(dist(*a, *b) < 1e-6, "{t}: {a:?} {b:?}");
        }
    }

    // Closed at one point with a tangent there.
    let mut d = with_sketch();
    let point = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0]}),
    );
    let p = point["entities"][0].as_str().unwrap().to_owned();
    let c = spline(
        &mut d,
        json!([p, [30, -5], [40, 20], [5, 18], [-10, 8], p]),
        Some(3),
        false,
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "tangent", "a": c, "b": c, "at": p}}),
    );
    for t in [0.1, 0.5, 0.85] {
        let s = sketch(&d);
        let shape = nurbs(&s, &c);
        let listed = ids(&s, &c);
        let q = insert(&mut d, &c, shape.eval(t)[0]);
        remove(&mut d, &c, &q);
        let s = sketch(&d);
        assert_eq!(ids(&s, &c), listed, "{t}");
        same_shape(&shape, &nurbs(&s, &c));
    }

    // A fitted spline: fitted through the same points again.
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([[0, 0], [20, 10], [40, 0], [60, 10]]),
        None,
        false,
    );
    let s = sketch(&d);
    let shape = nurbs(&s, &c);
    let listed = ids(&s, &c);
    let params = fit_params(&s, &c, false);
    let p = insert(&mut d, &c, shape.eval((params[1] + params[2]) / 2.0)[0]);
    remove(&mut d, &c, &p);
    let s = sketch(&d);
    assert_eq!(ids(&s, &c), listed);
    same_shape(&shape, &nurbs(&s, &c));
}

#[test]
fn constraints_and_dimensions_on_the_removed_point_go_the_others_stay() {
    let mut d = with_sketch();
    let c = spline(
        &mut d,
        json!([[0, 0], [20, 10], [40, 0], [60, 10]]),
        None,
        false,
    );
    let points = ids(&sketch(&d), &c);
    let horizontal = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal_points", "a": points[0], "b": points[2]}}),
    );
    let k = horizontal["constraints"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "distance", "a": points[0], "b": points[3]}, "value": 65}),
    );
    let before = sketch(&d);
    let dimensions = before["dimensions"].clone();
    let result = remove(&mut d, &c, &points[2]);
    assert!(removed(&result).contains(&k), "{result}");
    let s = sketch(&d);
    assert!(
        s["constraints"]
            .as_array()
            .is_none_or(|a| a.iter().all(|x| x["id"] != k.as_str())),
        "{s}"
    );
    assert_eq!(s["dimensions"], dimensions);
    assert!((dist(at(&s, &points[0]), at(&s, &points[3])) - 65.0).abs() < 1e-6);
    passes_through_fit_points(&s, &c, false);
    undo_restores(&mut d, &before);
}

#[test]
fn a_patterns_spline_is_freed_first() {
    let mut d = with_sketch();
    let c = spline(&mut d, json!([[10, 0], [20, 5], [30, 0]]), None, false);
    command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": [c],
               "center": [0, 0], "count": 3}),
    );
    let before = sketch(&d);
    let count = before["entities"].as_array().unwrap().len();
    let points = ids(&before, &c);
    let result = remove(&mut d, &c, &points[1]);
    assert!(shown_notes(&result).iter().any(|n| n == FREED), "{result}");
    let s = sketch(&d);
    assert!(s["patterns"].as_array().is_none_or(|p| p.is_empty()), "{s}");
    // The copies stay as plain geometry; only the point went.
    assert_eq!(s["entities"].as_array().unwrap().len(), count - 1);
    assert_eq!(ids(&s, &c), without(&points, 1));
    undo_restores(&mut d, &before);
}

#[test]
fn removing_is_refused_in_words_and_changes_nothing() {
    let mut d = with_sketch();
    // Not a spline.
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, -40], "end": [10, -40]}),
    );
    let l = line["made"][0].as_str().unwrap().to_owned();
    let start = line["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .find(|v| v.starts_with('p'))
        .unwrap();
    let error = refused(&mut d, &l, &start);
    assert!(error.contains("splines only"), "{error}");
    // A curve that does not exist.
    let error = refused(&mut d, "c99", &start);
    assert!(error.contains("c99"), "{error}");
    // A point the spline does not list.
    let c = spline(
        &mut d,
        json!([[0, 0], [20, 10], [40, 0], [60, 10]]),
        None,
        false,
    );
    let points = ids(&sketch(&d), &c);
    let error = refused(&mut d, &c, &start);
    assert!(error.contains("is not a point of"), "{error}");
    // A point another curve uses too: the line from the spline's end.
    let joined = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": points[3], "end": [80, 0]}),
    );
    let j = joined["made"][0].as_str().unwrap().to_owned();
    let error = refused(&mut d, &c, &points[3]);
    assert!(
        error.contains("also a point of") && error.contains(&j),
        "{error}"
    );
    // Fewer points than the spline needs: an open fitted spline keeps 2,
    // a closed one 3, one by control points its degree + 1.
    let two = spline(&mut d, json!([[0, 20], [20, 30]]), None, false);
    let two_points = ids(&sketch(&d), &two);
    let error = refused(&mut d, &two, &two_points[0]);
    assert!(error.contains("needs 2"), "{error}");
    let three = spline(&mut d, json!([[0, 50], [20, 60], [40, 50]]), None, true);
    let three_points = ids(&sketch(&d), &three);
    let error = refused(&mut d, &three, &three_points[1]);
    assert!(error.contains("needs 3"), "{error}");
    let cubic = spline(
        &mut d,
        json!([[0, 80], [10, 95], [30, 95], [40, 80]]),
        Some(3),
        false,
    );
    let cubic_points = ids(&sketch(&d), &cubic);
    let error = refused(&mut d, &cubic, &cubic_points[1]);
    assert!(error.contains("needs 4"), "{error}");
    // A curve a derived offset computes.
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [c], "distance": 2}),
    );
    let s = sketch(&d);
    let result = s["offsets"][0]["results"][0]
        .as_str()
        .unwrap_or_else(|| panic!("no offset result in {s}"))
        .to_owned();
    let offset_points = ids(&s, &result);
    let error = refused(&mut d, &result, &offset_points[1]);
    assert!(error.contains("follows offset"), "{error}");
    // Nothing changed with any of them.
    let after = sketch(&d);
    assert_eq!(after["entities"], s["entities"]);
    assert_eq!(after["constraints"], s["constraints"]);
}
