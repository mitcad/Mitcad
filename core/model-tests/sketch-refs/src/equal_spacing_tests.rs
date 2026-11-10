// SPDX-License-Identifier: MIT
//! Equal spacing (mitcad#221) through the sketch commands, with the mock
//! kernel: free and driven spacing, mixed anchors, the four modes, the
//! order, edits, deletions, refusals, undo and files.

use serde_json::{Value, json};

use mitcad_model::Document;
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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

/// Where an anchor is: a point, a circle's or an arc's centre, a line's
/// midpoint.
fn anchor(s: &Value, id: &str) -> [f64; 2] {
    if id.starts_with('p') {
        return at(s, id);
    }
    let g = entity(s, id)["geometry"].clone();
    let xy = |v: &Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
    if g["center"].is_array() {
        return xy(&g["center"]);
    }
    let (a, b) = (xy(&g["start"]), xy(&g["end"]));
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn constraint(s: &Value, id: &str) -> Value {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

fn dimension(s: &Value, id: &str) -> Value {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap().to_owned())
        .collect()
}

/// New points at the positions: their ids.
fn points(d: &mut Document<MockKernel>, at: &[[f64; 2]]) -> Vec<String> {
    at.iter()
        .map(|p| {
            let r = command(
                d,
                json!({"cmd": "sketch.add_point", "sketch": "F1", "at": p}),
            );
            ids(&r["entities"])[0].clone()
        })
        .collect()
}

/// The successive intervals of anchors along `u`, or their distances.
fn intervals(s: &Value, anchors: &[String], u: Option<[f64; 2]>) -> Vec<f64> {
    anchors
        .windows(2)
        .map(|w| {
            let (a, b) = (anchor(s, &w[0]), anchor(s, &w[1]));
            let d = [b[0] - a[0], b[1] - a[1]];
            match u {
                Some(u) => d[0] * u[0] + d[1] * u[1],
                None => d[0].hypot(d[1]),
            }
        })
        .collect()
}

fn anchors_of(s: &Value, k: &str) -> Vec<String> {
    ids(&constraint(s, k)["anchors"])
}

#[test]
fn a_free_equal_spacing_sorts_its_anchors_and_leaves_the_size_free() {
    let mut d = with_sketch();
    let p = points(&mut d, &[[21.0, -3.0], [0.0, 0.0], [30.0, 7.0], [9.0, 5.0]]);
    let s0 = sketch(&d);
    let dof = s0["dof"].as_u64().unwrap();
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [p[0], p[1], p[2], p[3]], "mode": "horizontal"}),
    );
    let k = ids(&r["constraints"])[0].clone();
    assert_eq!(ids(&r["made"]), vec![k.clone()]);
    assert!(ids(&r["dimensions"]).is_empty(), "no dimension asked for");
    assert_eq!(d.undo_label(), Some("Equal Spacing in Sketch1"));
    let s = sketch(&d);
    // Sorted along x: the stored order.
    let order = anchors_of(&s, &k);
    assert_eq!(
        order,
        vec![p[1].clone(), p[3].clone(), p[0].clone(), p[2].clone()]
    );
    assert_eq!(constraint(&s, &k)["mode"], "horizontal");
    assert!(constraint(&s, &k).get("direction").is_none());
    let v = intervals(&s, &order, Some([1.0, 0.0]));
    assert!(near(v[0], v[1]) && near(v[1], v[2]), "{v:?}");
    // Two equations: the common spacing stays free, and nothing aligns.
    assert_eq!(s["dof"].as_u64().unwrap(), dof - 2);
    for (id, y) in [(&p[0], -3.0), (&p[1], 0.0), (&p[2], 7.0), (&p[3], 5.0)] {
        assert!(near(at(&s, id)[1], y), "{id} moved off its y");
    }
    // A drag of the last point stretches every interval alike.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p[2], "to": [60, 7]}),
    );
    let s = sketch(&d);
    let v = intervals(&s, &order, Some([1.0, 0.0]));
    assert!(near(v[0], v[1]) && near(v[1], v[2]), "{v:?}");
    assert!(v[0] > 10.5, "the drag stretched the row: {v:?}");
    assert_eq!(anchors_of(&s, &k), order, "a drag never sorts again");
}

#[test]
fn one_common_dimension_drives_every_interval_and_its_removal_keeps_the_equality() {
    let mut d = with_sketch();
    let p = points(&mut d, &[[0.0, 0.0], [9.0, 0.0], [21.0, 4.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [p[0]]}),
    );
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal", "spacing": 12, "text": [6, -5]}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let dims = ids(&r["dimensions"]);
    assert_eq!(dims.len(), 1, "one dimension: {r}");
    let dim = dims[0].clone();
    assert_eq!(ids(&r["parameters"]).len(), 1);
    let s = sketch(&d);
    assert_eq!(dimension(&s, &dim)["type"], "spacing");
    assert_eq!(dimension(&s, &dim)["constraint"], json!(k));
    assert_eq!(dimension(&s, &dim)["text"], json!([6.0, -5.0]));
    assert!(near(
        dimension(&s, &dim)["measured"].as_f64().unwrap(),
        12.0
    ));
    assert!(near(at(&s, &p[1])[0], 12.0) && near(at(&s, &p[2])[0], 24.0));
    // No follower dimensions: one dimension for every interval.
    assert_eq!(s["dimensions"].as_array().unwrap().len(), 1);
    // The constraint and its dimension are one undo step.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert!(s["constraints"].as_array().unwrap().is_empty());
    assert!(s["dimensions"].as_array().unwrap().is_empty());
    assert!(near(at(&s, &p[1])[0], 9.0));
    command(&mut d, json!({"cmd": "redo"}));
    // Editing the one value changes every interval.
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dim, "value": "20 mm"}),
    );
    let s = sketch(&d);
    assert!(near(at(&s, &p[1])[0], 20.0) && near(at(&s, &p[2])[0], 40.0));
    let dof = s["dof"].as_u64().unwrap();
    // Without the dimension the intervals stay equal, their size free.
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [dim]}),
    );
    assert_eq!(ids(&removed["removed"]), vec![dim.clone()]);
    let s = sketch(&d);
    assert!(!constraint(&s, &k).is_null(), "the equality stays");
    assert_eq!(s["dof"].as_u64().unwrap(), dof + 1);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p[2], "to": [50, 4]}),
    );
    let s = sketch(&d);
    let v = intervals(&s, &p, Some([1.0, 0.0]));
    assert!(near(v[0], v[1]) && v[0] > 20.5, "{v:?}");
    // A dimension can come back afterwards, measured, as the Dimension
    // command adds one.
    let again = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "spacing", "constraint": k}}),
    );
    let s = sketch(&d);
    let dim = ids(&again["dimensions"])[0].clone();
    assert!(near(
        dimension(&s, &dim)["measured"].as_f64().unwrap(),
        intervals(&s, &p, Some([1.0, 0.0]))[0]
    ));
    // But only one.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "spacing", "constraint": k}, "driven": true}),
    );
    assert!(error.contains("another spacing dimension"), "{error}");
    // Removing the constraint takes its dimension along.
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [k]}),
    );
    let mut gone = ids(&removed["removed"]);
    gone.sort();
    let mut expected = [k.clone(), dim.clone()];
    expected.sort();
    assert_eq!(gone, expected);
}

#[test]
fn mixed_anchors_space_along_a_line_without_aligning_or_resizing() {
    let mut d = with_sketch();
    // A point, a line (its midpoint), a circle and an arc (their centres).
    let p = points(&mut d, &[[0.0, 0.0]]);
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [8, -4], "end": [12, 4]}),
    );
    let line = ids(&line["made"])[0].clone();
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [26, 3],
               "radius": 2}),
    );
    let circle = ids(&circle["made"])[0].clone();
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": [33, -2],
               "start": [38, -2], "end": [33, 3]}),
    );
    let arc = ids(&arc["made"])[0].clone();
    // The arc keeps its radius (moving only its centre would reshape it).
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "radius", "curve": arc}, "value": 5}),
    );
    // The direction: a construction line along (3, 4) / 5.
    let dir = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, -30], "end": [3, -26],
               "construction": true}),
    );
    let dir = ids(&dir["made"])[0].clone();
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [p[0], dir]}),
    );
    let before = sketch(&d);
    let length = |s: &Value| {
        let g = entity(s, &line)["geometry"].clone();
        let (a, b) = (&g["start"], &g["end"]);
        (b[0].as_f64().unwrap() - a[0].as_f64().unwrap())
            .hypot(b[1].as_f64().unwrap() - a[1].as_f64().unwrap())
    };
    let radius = |s: &Value, c: &str| entity(s, c)["geometry"]["radius"].as_f64().unwrap();
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [arc, p[0], circle, line], "mode": "along", "direction": dir,
               "spacing": 10}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let s = sketch(&d);
    let order = anchors_of(&s, &k);
    assert_eq!(
        order,
        vec![p[0].clone(), line.clone(), circle.clone(), arc.clone()]
    );
    assert_eq!(constraint(&s, &k)["direction"], json!(dir));
    let u = [0.6, 0.8];
    for v in intervals(&s, &order, Some(u)) {
        assert!(near(v, 10.0), "{v}");
    }
    // Across the direction the anchors keep their offsets (the arc's centre
    // may move across a little, as its ends follow it at its radius).
    let across = |s: &Value, id: &str| {
        let a = anchor(s, id);
        -0.8 * a[0] + 0.6 * a[1]
    };
    for id in &order[..3] {
        assert!(
            near(across(&s, id.as_str()), across(&before, id.as_str())),
            "{id} moved across"
        );
    }
    // The objects keep their own sizes: nothing makes them equal.
    assert!(near(length(&s), length(&before)));
    assert!(near(radius(&s, circle.as_str()), 2.0));
    assert!(near(radius(&s, arc.as_str()), 5.0));
    // A file keeps the constraint, its order, mode, direction and dimension.
    let text = d.to_json();
    let mut loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    loaded.recompute();
    let reopened = sketch(&loaded);
    assert_eq!(reopened["constraints"], s["constraints"]);
    assert_eq!(reopened["dimensions"], s["dimensions"]);
    // An unsupported anchor is refused with the reason.
    let ellipse = command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 40], "major": [5, 40],
               "minor_radius": 2}),
    );
    let ellipse = ids(&ellipse["made"])[0].clone();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [p[0], circle, ellipse], "mode": "horizontal"}),
    );
    assert!(
        error.contains("anchors are points, circles and arcs"),
        "{error}"
    );
    // A circle and its own centre are the same anchor.
    let centre = entity(&s, &circle)["center"].as_str().unwrap().to_owned();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [p[0], circle, centre], "mode": "point_to_point"}),
    );
    assert!(error.contains("stand for the same point"), "{error}");
}

#[test]
fn vertical_and_point_to_point_spacings_keep_the_order_given() {
    let mut d = with_sketch();
    let p = points(&mut d, &[[0.0, 30.0], [4.0, 22.0], [-2.0, 5.0], [6.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [p[0]]}),
    );
    // Listed top down and kept so: the intervals run downwards.
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [p[0], p[1], p[2]], "mode": "vertical", "keep_order": true,
               "spacing": 12}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let s = sketch(&d);
    assert_eq!(
        anchors_of(&s, &k),
        vec![p[0].clone(), p[1].clone(), p[2].clone()]
    );
    assert!(near(at(&s, &p[1])[1], 18.0) && near(at(&s, &p[2])[1], 6.0));
    assert!(
        near(at(&s, &p[1])[0], 4.0) && near(at(&s, &p[2])[0], -2.0),
        "x free"
    );
    // A chain in the order given: equal links, not a straight row.
    let mut d = with_sketch();
    let p = points(
        &mut d,
        &[[0.0, 0.0], [10.0, 0.0], [14.0, 7.0], [10.0, 20.0]],
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [p[0]]}),
    );
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [p[0], p[1], p[2], p[3]], "mode": "point_to_point", "spacing": 12}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let s = sketch(&d);
    assert_eq!(anchors_of(&s, &k), p);
    for v in intervals(&s, &p, None) {
        assert!(near(v, 12.0), "{v}");
    }
    let (a, b, c) = (at(&s, &p[0]), at(&s, &p[1]), at(&s, &p[2]));
    let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    assert!(cross.abs() > 1.0, "the chain was straightened");
}

#[test]
fn ties_along_the_direction_are_named_and_ordered_explicitly() {
    let mut d = with_sketch();
    let p = points(
        &mut d,
        &[[0.0, 0.0], [10.0, 5.0], [10.0, -5.0], [20.0, 0.0]],
    );
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal"}),
    );
    assert!(
        error.contains(&format!("{} and {} are at the same place", p[1], p[2])),
        "{error}"
    );
    assert!(sketch(&d)["constraints"].as_array().unwrap().is_empty());
    // The listed order, kept: the tied pair is pulled apart evenly.
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal", "keep_order": true}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let s = sketch(&d);
    assert_eq!(anchors_of(&s, &k), p);
    let v = intervals(&s, &p, Some([1.0, 0.0]));
    assert!(near(v[0], v[1]) && near(v[1], v[2]), "{v:?}");
}

#[test]
fn members_order_and_mode_change_in_one_edit_and_bad_edits_change_nothing() {
    let mut d = with_sketch();
    let p = points(
        &mut d,
        &[[0.0, 0.0], [10.0, 30.0], [20.0, 10.0], [30.0, 20.0]],
    );
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1",
               "anchors": [p[0], p[1], p[2]], "mode": "horizontal", "spacing": 10}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let dim = ids(&r["dimensions"])[0].clone();
    // A fourth member: sorted in along x.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_equal_spacing", "sketch": "F1", "constraint": k,
               "anchors": [p[3], p[0], p[1], p[2]]}),
    );
    assert_eq!(d.undo_label(), Some("Edit Equal Spacing in Sketch1"));
    let s = sketch(&d);
    assert_eq!(anchors_of(&s, &k), p);
    let v = intervals(&s, &p, Some([1.0, 0.0]));
    assert!(v.iter().all(|v| near(*v, 10.0)), "{v:?}");
    // Vertical now: sorted along y, the dimension stays and drives it.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_equal_spacing", "sketch": "F1", "constraint": k,
               "mode": "vertical"}),
    );
    let s = sketch(&d);
    let order = anchors_of(&s, &k);
    assert_eq!(
        order,
        vec![p[0].clone(), p[2].clone(), p[3].clone(), p[1].clone()]
    );
    assert_eq!(constraint(&s, &k)["mode"], "vertical");
    assert!(!dimension(&s, &dim).is_null());
    let v = intervals(&s, &order, Some([0.0, 1.0]));
    assert!(v.iter().all(|v| near(*v, 10.0)), "{v:?}");
    // Two anchors make no equal spacing: refused, nothing changes.
    let before = sketch(&d);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.edit_equal_spacing", "sketch": "F1", "constraint": k,
               "anchors": [p[0], p[1]]}),
    );
    assert!(error.contains("at least three"), "{error}");
    assert_eq!(sketch(&d)["constraints"], before["constraints"]);
    // Undo takes back one edit at a time.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_eq!(constraint(&s, &k)["mode"], "horizontal");
    assert_eq!(anchors_of(&s, &k), p);
    command(&mut d, json!({"cmd": "redo"}));
    assert_eq!(constraint(&sketch(&d), &k)["mode"], "vertical");
    // Only an equal spacing edits so.
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "vertical_points", "a": p[0], "b": p[2]}}),
    );
    let h = sketch(&d)["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "vertical_points")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.edit_equal_spacing", "sketch": "F1", "constraint": h,
               "mode": "vertical"}),
    );
    assert!(error.contains("is not an equal spacing"), "{error}");
}

#[test]
fn deleted_members_leave_the_chain_and_too_few_take_it_away() {
    let mut d = with_sketch();
    let p = points(
        &mut d,
        &[
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 0.0],
            [30.0, 0.0],
            [40.0, 0.0],
        ],
    );
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "point_to_point", "spacing": 10}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let dim = ids(&r["dimensions"])[0].clone();
    // A member deleted: its neighbours become successive, the rest keeps
    // its order.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [p[2]]}),
    );
    let s = sketch(&d);
    let left: Vec<String> = [0, 1, 3, 4].iter().map(|i| p[*i].clone()).collect();
    assert_eq!(anchors_of(&s, &k), left);
    assert!(!dimension(&s, &dim).is_null());
    for v in intervals(&s, &left, None) {
        assert!(near(v, 10.0), "{v}");
    }
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [p[0]]}),
    );
    assert_eq!(anchors_of(&sketch(&d), &k).len(), 3);
    // Two left: the constraint and its dimension go, no dangling anchor.
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [p[4]]}),
    );
    let removed = ids(&removed["removed"]);
    assert!(
        removed.contains(&k) && removed.contains(&dim),
        "{removed:?}"
    );
    let s = sketch(&d);
    assert!(s["constraints"].as_array().unwrap().is_empty());
    assert!(s["dimensions"].as_array().unwrap().is_empty());
    // An along spacing whose direction line is deleted goes too.
    let mut d = with_sketch();
    let p = points(&mut d, &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    let dir = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, -10], "end": [10, -10]}),
    );
    let dir = ids(&dir["made"])[0].clone();
    let r = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "along", "direction": dir}),
    );
    let k = ids(&r["constraints"])[0].clone();
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [dir]}),
    );
    assert!(ids(&removed["removed"]).contains(&k));
    assert!(sketch(&d)["constraints"].as_array().unwrap().is_empty());
}

#[test]
fn conflicts_redundancy_and_patterns_are_refused_without_a_partial_change() {
    let mut d = with_sketch();
    let p = points(&mut d, &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [p[0]]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "horizontal_distance", "a": p[0], "b": p[2]},
               "value": 30}),
    );
    let before = sketch(&d);
    // The ends are 30 apart: a spacing of 10 contradicts it.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal", "spacing": 10}),
    );
    assert!(
        error.contains("can't hold") || error.contains("cannot be solved"),
        "{error}"
    );
    let after = sketch(&d);
    assert_eq!(after["constraints"], before["constraints"]);
    assert_eq!(after["dimensions"], before["dimensions"]);
    // The free spacing is fine, a second one on the same anchors redundant.
    command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal"}),
    );
    let s = sketch(&d);
    assert!(near(at(&s, &p[1])[0], 15.0));
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal"}),
    );
    // Named with what it follows from: the first one (k2).
    assert!(
        error.contains("k3 would over-constrain") && error.contains("follows from k2"),
        "{error}"
    );
    assert_eq!(sketch(&d)["constraints"], s["constraints"]);
    // Copies of a pattern: its spacing already governs them.
    let mut d = with_sketch();
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0],
               "radius": 2}),
    );
    let circle = ids(&circle["made"])[0].clone();
    let pattern = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": [circle],
               "count": [3, 1], "spacing": [10, 10]}),
    );
    let pattern_id = ids(&pattern["constraints"])[0].clone();
    let copies = ids(&pattern["made"]);
    let mut anchors = vec![circle.clone()];
    anchors.extend(copies.iter().filter(|c| c.starts_with('c')).cloned());
    assert_eq!(anchors.len(), 3, "{pattern}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": anchors,
               "mode": "horizontal"}),
    );
    assert!(
        error.contains(&format!("instances of pattern {pattern_id}"))
            && error.contains("Edit Pattern"),
        "{error}"
    );
}

#[test]
fn copies_keep_their_spacing_among_the_copies() {
    let mut d = with_sketch();
    let p = points(&mut d, &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "point_to_point"}),
    );
    let copied = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": p, "by": [0, 50],
               "copy": true}),
    );
    let made = ids(&copied["made"]);
    let added = ids(&copied["constraints"]);
    assert_eq!(added.len(), 1, "{copied}");
    let s = sketch(&d);
    assert_eq!(anchors_of(&s, &added[0]), made);
}
