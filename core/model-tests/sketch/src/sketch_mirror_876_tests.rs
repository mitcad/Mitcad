// SPDX-License-Identifier: MIT
//! Mirror over a rounded rectangle's construction diagonal and about the
//! sketch's own axes (mitcad#876), with the mock kernel.

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

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
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

/// The id of the sketch's point at a position.
fn point_at(s: &Value, xy: [f64; 2]) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "point"
                && close(e["at"][0].as_f64().unwrap(), xy[0])
                && close(e["at"][1].as_f64().unwrap(), xy[1])
        })
        .unwrap_or_else(|| panic!("no point at {xy:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// A circle's centre point id, centre and radius.
fn circle(s: &Value, id: &str) -> (String, [f64; 2], f64) {
    let e = entity(s, id);
    assert_eq!(e["type"], "circle", "{id}: {s}");
    let centre = e["center"].as_str().unwrap().to_owned();
    let at = at(s, &centre);
    (centre, at, e["radius"].as_f64().unwrap())
}

fn add_circle(d: &mut Document<MockKernel>, center: [f64; 2], radius: f64) -> String {
    let made = command(
        d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": center, "radius": radius}),
    );
    made["made"][0].as_str().unwrap().to_owned()
}

/// The sketch solves, without redundant or conflicting constraints (an
/// axis line included, mitcad#877).
fn assert_clean(s: &Value) {
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert!(
        s["conflicts"].as_array().is_none_or(|c| c.is_empty()),
        "{s}"
    );
}

/// The geometry of the report (mitcad#876): a 50 x 50 rounded rectangle
/// (Center Rectangle with radius 5) centred on a fixed point at the
/// origin, its construction diagonal from (-25, -25) to (25, 25) with its
/// middle there, and three free circles off the diagonal. The diagonal is
/// held in place by the rectangle, not fixed itself. Returns the
/// diagonal, its end points and middle, and the circles.
fn reported_design() -> (Document<MockKernel>, String, Vec<String>, Vec<String>) {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "center",
               "center": [0, 0], "corner": [25, 25], "radius": 5}),
    );
    let s = sketch(&d);
    let diagonals: Vec<Value> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line" && e["construction"] == true)
        .cloned()
        .collect();
    assert_eq!(diagonals.len(), 1, "one construction diagonal: {s}");
    let diagonal = diagonals[0]["id"].as_str().unwrap().to_owned();
    let ends = [
        diagonals[0]["start"].as_str().unwrap().to_owned(),
        diagonals[0]["end"].as_str().unwrap().to_owned(),
    ];
    let corners = [[-25.0, -25.0], [25.0, -25.0], [25.0, 25.0]].map(|xy| point_at(&s, xy));
    for (a, b) in [(0, 1), (1, 2)] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "distance", "a": corners[a], "b": corners[b]},
                   "value": 50}),
        );
    }
    let middle = point_at(&s, [0.0, 0.0]);
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [middle]}),
    );
    let circles = vec![
        add_circle(&mut d, [-17.32, 13.75], 4.24),
        add_circle(&mut d, [15.0, 10.0], 5.0),
        add_circle(&mut d, [0.0, 10.1], 5.0),
    ];
    let s = sketch(&d);
    assert_clean(&s);
    let mut on_axis = ends.to_vec();
    on_axis.push(middle);
    (d, diagonal, on_axis, circles)
}

/// Each copy is the mirror image of its original about the line y = x,
/// with the same radius.
fn assert_mirrored_over_the_diagonal(s: &Value, originals: &[String], copies: &[String]) {
    assert_eq!(originals.len(), copies.len(), "{s}");
    for (original, copy) in originals.iter().zip(copies) {
        let (_, [x, y], r) = circle(s, original);
        let (_, [cx, cy], cr) = circle(s, copy);
        assert!(
            close(cx, y) && close(cy, x) && close(cr, r),
            "{copy} at ({cx}, {cy}) r {cr} is not {original} at ({x}, {y}) r {r} mirrored: {s}"
        );
    }
}

#[test]
fn three_circles_mirrored_over_a_rounded_rectangles_diagonal() {
    let (mut d, diagonal, on_axis, circles) = reported_design();
    let dof = sketch(&d)["dof"].as_u64().unwrap();
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": circles,
               "axis": diagonal}),
    );
    let copies = strings(&mirrored["made"]);
    assert_eq!(copies.len(), 3, "{mirrored}");
    // One symmetry per circle: its centre and its radius.
    assert_eq!(
        mirrored["constraints"].as_array().unwrap().len(),
        3,
        "{mirrored}"
    );
    let s = sketch(&d);
    assert_clean(&s);
    // The copies take no freedom of their own.
    assert_eq!(s["dof"].as_u64().unwrap(), dof, "{s}");
    assert_mirrored_over_the_diagonal(&s, &circles, &copies);
    // The diagonal stayed where the rectangle holds it.
    for (p, xy) in on_axis
        .iter()
        .zip([[-25.0, -25.0], [25.0, 25.0], [0.0, 0.0]])
    {
        let [x, y] = at(&s, p);
        assert!(close(x, xy[0]) && close(y, xy[1]), "{p}: {s}");
    }
    // The copies follow their originals.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": circles[1], "radius": 3}),
    );
    let (centre, _, _) = circle(&sketch(&d), &circles[2]);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": centre, "to": [-5, 12]}),
    );
    let s = sketch(&d);
    assert_clean(&s);
    assert!(close(circle(&s, &copies[1]).2, 3.0), "{s}");
    assert_mirrored_over_the_diagonal(&s, &circles, &copies);
}

#[test]
fn a_window_selection_with_the_line_and_the_centres_mirrors_the_rest() {
    // Everything a window over the circles and the diagonal takes: the
    // diagonal itself, its end points and middle, the circles and their
    // centres. The line is left out, the points on it are shared, and the
    // centres get no symmetry beside their circles' (no redundancy).
    let (mut d, diagonal, on_axis, circles) = reported_design();
    let s = sketch(&d);
    let mut entities = vec![diagonal.clone()];
    entities.extend(on_axis.iter().cloned());
    for c in &circles {
        entities.push(c.clone());
        entities.push(circle(&s, c).0);
    }
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": entities,
               "axis": diagonal}),
    );
    let made = strings(&mirrored["made"]);
    assert!(!made.contains(&diagonal), "{mirrored}");
    assert_eq!(
        mirrored["constraints"].as_array().unwrap().len(),
        3,
        "{mirrored}"
    );
    let copies: Vec<String> = made
        .iter()
        .filter(|id| id.starts_with('c'))
        .cloned()
        .collect();
    let s = sketch(&d);
    assert_clean(&s);
    assert_mirrored_over_the_diagonal(&s, &circles, &copies);
    // The points on the diagonal are the copies' own: nothing new there.
    for p in &on_axis {
        assert!(made.contains(p), "{p} shared: {mirrored}");
    }
    // The line alone is still refused.
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [diagonal],
                   "axis": diagonal}),
        )
        .contains("cannot be mirrored"),
    );
}

#[test]
fn circles_mirrored_about_the_sketch_axes() {
    let mut d = with_sketch();
    let c = add_circle(&mut d, [20.0, 10.0], 4.0);
    let depth = d.undo_depth();
    // The x axis, made by the mirror itself in its one step.
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [c], "axis": "x"}),
    );
    assert_eq!(d.undo_depth(), depth + 1);
    let copy = strings(&mirrored["made"])[0].clone();
    let s = sketch(&d);
    assert_clean(&s);
    let (_, centre, radius) = circle(&s, &copy);
    assert!(
        close(centre[0], 20.0) && close(centre[1], -10.0) && close(radius, 4.0),
        "{s}"
    );
    let axis_lines = |s: &Value, axis: &str| -> Vec<String> {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["type"] == "line" && e["axis"] == axis)
            .map(|e| e["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let x = axis_lines(&s, "x");
    assert_eq!(x.len(), 1, "{s}");
    let symmetric: Vec<&Value> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["type"] == "symmetric")
        .collect();
    assert_eq!(symmetric.len(), 1, "{s}");
    assert_eq!(symmetric[0]["axis"], json!(x[0]), "{s}");
    // Undo takes the copy and the axis back together.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert!(axis_lines(&s, "x").is_empty(), "{s}");
    assert_eq!(s["entities"].as_array().unwrap().len(), 2, "{s}");
    command(&mut d, json!({"cmd": "redo"}));

    // The y axis, then the y axis again: the one line serves both.
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [c], "axis": "y"}),
    );
    let copy = strings(&mirrored["made"])[0].clone();
    let s = sketch(&d);
    assert_clean(&s);
    let (_, centre, _) = circle(&s, &copy);
    assert!(close(centre[0], -20.0) && close(centre[1], 10.0), "{s}");
    let y = axis_lines(&s, "y");
    assert_eq!(y.len(), 1, "{s}");
    command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [copy], "axis": "y"}),
    );
    let s = sketch(&d);
    assert_clean(&s);
    assert_eq!(axis_lines(&s, "y"), y, "{s}");
    // The made axis line works as any line: the first copy about it.
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [c], "axis": y[0]}),
    );
    let copy = strings(&mirrored["made"])[0].clone();
    let s = sketch(&d);
    assert_clean(&s);
    let (_, centre, _) = circle(&s, &copy);
    assert!(close(centre[0], -20.0) && close(centre[1], 10.0), "{s}");
    // The copies follow the original about both axes.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": c, "radius": 6}),
    );
    let s = sketch(&d);
    assert_clean(&s);
    let circles = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "circle")
        .count();
    assert_eq!(circles, 5, "{s}");
    for e in s["entities"].as_array().unwrap() {
        if e["type"] == "circle" {
            assert!(close(e["radius"].as_f64().unwrap(), 6.0), "{s}");
        }
    }
    // Anything else is refused with what it takes.
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": [c], "axis": "z"}),
        )
        .contains("invalid mirror line “z”"),
    );
}
