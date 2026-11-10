// SPDX-License-Identifier: MIT
//! Conflicting constraints told in the user's words (mitcad#1566): a
//! sketch edit refused for conflicting constraints says what cannot hold by
//! the constraints' types and the dimensions' values, never by ids, and
//! names the constraint that blocks the change, not the whole chain the
//! solver found; the ids stay on the refusal's last line, and in the
//! `sketch` query's `conflicts` of a sketch that does not solve.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::sketch::conflict;
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

fn made(result: &Value) -> String {
    result["made"][0].as_str().unwrap().to_owned()
}

fn first(result: &Value, key: &str) -> String {
    result[key][0].as_str().unwrap().to_owned()
}

/// A field of an entity of the sketch (a line's `start`, an arc's `end`).
fn field(s: &Value, id: &str, key: &str) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))[key]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Whether a text names a sketch entity, constraint or dimension by its id
/// (`p15`, `c14`, `k5`).
fn names_an_id(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            let mut chars = word.chars();
            matches!(chars.next(), Some('p' | 'c' | 'k' | 't'))
                && !word[1..].is_empty()
                && word[1..].chars().all(|c| c.is_ascii_digit())
        })
}

/// The case of the ticket: two lines between two arcs of radius 7.5 whose
/// centres are fixed 35.36 mm apart, both parallel to the construction
/// centre line through the centres, Equal, the upper one 2 mm from the
/// centre line; the arcs run round the outer sides from the upper line's
/// ends to the lower line's. Returns the document, the upper line, the left
/// arc and the Equal's id.
fn equal_lines_between_arcs() -> (Document<MockKernel>, String, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let gap = 35.36;
    let x = (7.5f64 * 7.5 - 4.0).sqrt();
    let left = first(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
        ),
        "entities",
    );
    let right = first(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [gap, 0], "fixed": true}),
        ),
        "entities",
    );
    let centre = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": left, "end": right,
               "construction": true}),
    ));
    let upper = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [x, 2], "end": [gap - x, 2]}),
    ));
    let s = sketch(&d);
    let (upper_start, upper_end) = (field(&s, &upper, "start"), field(&s, &upper, "end"));
    let left_arc = made(&command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": left,
               "start": upper_start, "end": [x, -2]}),
    ));
    let lower_start = field(&sketch(&d), &left_arc, "end");
    let lower = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": lower_start,
               "end": [gap - x, -2]}),
    ));
    let lower_end = field(&sketch(&d), &lower, "end");
    let right_arc = made(&command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": right,
               "start": lower_end, "end": [gap - x, 2]}),
    ));
    let right_end = field(&sketch(&d), &right_arc, "end");
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": right_end, "entity": upper_end}}),
    );
    for arc in [&left_arc, &right_arc] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "radius", "curve": arc}, "value": 7.5}),
        );
    }
    for line in [&upper, &lower] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                   "constraint": {"type": "parallel", "a": line, "b": centre}}),
        );
    }
    let equal = first(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                   "constraint": {"type": "equal", "a": upper, "b": lower}}),
        ),
        "constraints",
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "line_distance", "a": upper, "b": centre}, "value": 2}),
    );
    let s = sketch(&d);
    assert!(s["solved"].as_bool().unwrap(), "{s}");
    assert_eq!(s["conflicts"], json!([]), "{s}");
    (d, upper, left_arc, equal)
}

/// The fillet of R 2 shortens the upper line to about 19.5 mm; the lower
/// one, its ends on the arcs' circles, cannot be shorter than 20.36 mm: the
/// Equal blocks it, not the 2 mm distance, the parallels or the fillet's
/// own tangents and radius.
#[test]
fn a_fillet_the_equal_blocks_names_the_equal_without_ids() {
    let (mut d, upper, left_arc, equal) = equal_lines_between_arcs();
    let before = sketch(&d);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": upper, "b": left_arc, "radius": 2}),
    );
    let (message, ids) = conflict::split(&error);
    assert!(!names_an_id(message), "{error}");
    assert!(
        message.starts_with("the Equal between two lines can't hold with the fillet"),
        "{error}"
    );
    assert_eq!(ids, Some(vec![equal]), "{error}");
    // Refused: the sketch is as it was.
    let after = sketch(&d);
    assert_eq!(after["constraints"], before["constraints"]);
    assert_eq!(after["dimensions"], before["dimensions"]);
    // A small fillet fits (the line stays longer than the lower one can
    // be short).
    command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": upper, "b": left_arc, "radius": 0.5}),
    );
}

/// A constraint that contradicts the others is refused by what blocks it,
/// in words, with the blocking ones' ids on the last line.
#[test]
fn a_contradicting_constraint_names_what_blocks_it() {
    let (mut d, upper, _, _) = equal_lines_between_arcs();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "vertical", "line": upper}}),
    );
    let (message, ids) = conflict::split(&error);
    assert!(!names_an_id(message), "{error}");
    assert!(
        message.ends_with("can't hold with the new constraint"),
        "{error}"
    );
    let ids = ids.unwrap_or_else(|| panic!("{error}"));
    assert!(!ids.is_empty(), "{error}");
    let s = sketch(&d);
    let known: Vec<&Value> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .chain(s["dimensions"].as_array().unwrap())
        .map(|k| &k["id"])
        .collect();
    for id in &ids {
        assert!(known.iter().any(|k| *k == id), "{id} in {error}");
    }
}

/// A dimension that only fixed geometry contradicts says so without
/// naming anything else.
#[test]
fn a_dimension_against_fixed_geometry_names_the_fixed_geometry() {
    let (mut d, _, _, _) = equal_lines_between_arcs();
    let s = sketch(&d);
    let points: Vec<String> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point" && e["fixed"] == true)
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(points.len(), 2, "{s}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 30,
               "dimension": {"type": "distance", "a": points[0], "b": points[1]}}),
    );
    let (message, ids) = conflict::split(&error);
    assert_eq!(
        message, "the new dimension can't hold with fixed geometry",
        "{error}"
    );
    assert_eq!(ids, Some(Vec::new()), "{error}");
}
