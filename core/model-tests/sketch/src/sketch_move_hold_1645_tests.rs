// SPDX-License-Identifier: MIT
//! Sketch Move/Copy holds the moved geometry where it was dropped
//! (mitcad#1645, GitHub mitcad/Mitcad#245): `sketch.move`'s `hold`
//! constraints are added after the move in its undo step, with a moved
//! entity standing for its copy when `copy` is on, and one the sketch
//! refuses is left out with a warning while the move is kept.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
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

/// Where a point is.
fn at(s: &Value, point: &str) -> [f64; 2] {
    let p = &entity(s, point)["at"];
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

/// The point at a place.
fn point_at(s: &Value, place: [f64; 2]) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["at"].as_array().is_some_and(|p| {
                (p[0].as_f64().unwrap() - place[0]).abs() <= 1e-9
                    && (p[1].as_f64().unwrap() - place[1]).abs() <= 1e-9
            })
        })
        .unwrap_or_else(|| panic!("no point at {place:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The baseline tolerance of geometric comparisons.
fn assert_near(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6,
        "{what}: {a:?}, expected {b:?}"
    );
}

/// A circle about `center` with its centre point's id and its own.
fn circle(doc: &mut Document<MockKernel>, center: [f64; 2]) -> (String, String) {
    let made = command(
        doc,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": center, "radius": 3}),
    );
    let c = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with('c'))
        .unwrap()
        .to_owned();
    let s = sketch(doc);
    let center = entity(&s, &c)["center"].as_str().unwrap().to_owned();
    (center, c)
}

fn point(doc: &mut Document<MockKernel>, at: [f64; 2]) -> String {
    command(
        doc,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": at}),
    );
    point_at(&sketch(doc), at)
}

/// The coincident constraints of `point`, by what they hold it on.
fn coincident_with(s: &Value, point: &str) -> Vec<String> {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["type"] == "coincident" && k["point"] == point)
        .map(|k| k["entity"].as_str().unwrap().to_owned())
        .collect()
}

fn coincident(point: &str, on: &str) -> Value {
    json!({"type": "coincident", "point": point, "entity": on})
}

#[test]
fn a_move_point_to_point_holds_the_centre_on_the_target_and_follows_it() {
    let mut d = with_sketch();
    let (center, c) = circle(&mut d, [-20.0, 0.0]);
    let target = point(&mut d, [20.0, 10.0]);

    let moved = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c],
               "from": center, "to": target, "hold": [coincident(&center, &target)]}),
    );
    assert!(moved["warnings"].is_null(), "{moved}");
    let s = sketch(&d);
    assert_near(at(&s, &center), [20.0, 10.0], "the centre on the target");
    assert_eq!(coincident_with(&s, &center), vec![target.clone()], "{s}");

    // One undo step: the move and its hold go together.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_near(at(&s, &center), [-20.0, 0.0], "undone");
    assert!(coincident_with(&s, &center).is_empty(), "{s}");
    command(&mut d, json!({"cmd": "redo"}));

    // The circle follows when the target point moves.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": target, "to": [25, 15]}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &target), [25.0, 15.0], "the dragged target");
    assert_near(at(&s, &center), [25.0, 15.0], "the centre follows");
}

#[test]
fn a_copy_holds_the_copy_not_the_original() {
    let mut d = with_sketch();
    let (center, c) = circle(&mut d, [-20.0, 0.0]);
    let target = point(&mut d, [20.0, 10.0]);

    // The hold names the original's centre; the copy's is held.
    let copied = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c], "copy": true,
               "from": center, "to": target, "hold": [coincident(&center, &target)]}),
    );
    assert!(copied["warnings"].is_null(), "{copied}");
    let made: Vec<String> = copied["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(made.len(), 1, "{copied}");
    let s = sketch(&d);
    let copy_center = entity(&s, &made[0])["center"].as_str().unwrap().to_owned();
    assert_near(at(&s, &copy_center), [20.0, 10.0], "the copy on the target");
    assert_near(at(&s, &center), [-20.0, 0.0], "the original stays");
    assert_eq!(
        coincident_with(&s, &copy_center),
        vec![target.clone()],
        "{s}"
    );
    assert!(coincident_with(&s, &center).is_empty(), "{s}");

    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": target, "to": [20, 20]}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &copy_center), [20.0, 20.0], "the copy follows");
    assert_near(at(&s, &center), [-20.0, 0.0], "the original stays");
}

#[test]
fn a_translation_onto_a_line_holds_the_point_on_it() {
    let mut d = with_sketch();
    let p = point(&mut d, [0.0, 0.0]);
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-10, 10], "end": [10, 10]}),
    );
    let l = line["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with('c'))
        .unwrap()
        .to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [p], "by": [3, 10],
               "hold": [coincident(&p, &l)]}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &p), [3.0, 10.0], "the point on the line");
    assert_eq!(coincident_with(&s, &p), vec![l.clone()], "{s}");
}

#[test]
fn a_hold_the_sketch_refuses_is_left_out_and_the_move_kept() {
    let mut d = with_sketch();
    let (center, c) = circle(&mut d, [-20.0, 0.0]);
    let target = point(&mut d, [20.0, 10.0]);

    // The same hold twice: the second is implied by the first.
    let moved = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c],
               "from": center, "to": target,
               "hold": [coincident(&center, &target), coincident(&center, &target)]}),
    );
    let warnings = moved["warnings"].as_array().cloned().unwrap_or_default();
    assert_eq!(warnings.len(), 1, "{moved}");
    let shown = moved["warning_messages"][0].as_str().unwrap_or_default();
    assert!(
        shown.starts_with("not held where it was dropped: "),
        "{moved}"
    );
    // In words: no ids in what a person reads (mitcad#1612).
    assert!(
        !shown.contains(&center) && !shown.contains(&target),
        "{moved}"
    );
    let s = sketch(&d);
    assert_near(at(&s, &center), [20.0, 10.0], "the move is kept");
    assert_eq!(coincident_with(&s, &center), vec![target.clone()], "{s}");
}
