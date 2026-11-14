// SPDX-License-Identifier: MIT
//! Trim, Convert to Sketch Geometry and deleting the offset free an
//! offset's curves from their source (mitcad#1755, GitHub mitcad/Mitcad#314).
//!
//! An exact offset held its copies to the source with constraints (each
//! line parallel at the distance, each arc about the source's centre with a
//! construction helper line across the gap) and made them again from its
//! record whenever the curves that vanish at its distance changed: Trim cut
//! an arc copy at its helper line, a trimmed copy could come back whole, and
//! deleting the offset left the copies tied to the source. Now Trim (and
//! Extend, Sketch Fillet and Chamfer) on an offset's curve first turns all of
//! that offset's curves into plain sketch geometry, with a note; Convert to
//! Sketch Geometry does the same for a selected offset's curve, and deleting
//! the offset leaves its curves as plain geometry.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const FREED: &str = "the offset is no longer linked to its source";

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

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// A curve's solved ends.
fn ends(s: &Value, id: &str) -> ([f64; 2], [f64; 2]) {
    let g = &entity(s, id)["geometry"];
    (xy(&g["start"]), xy(&g["end"]))
}

fn radius(s: &Value, id: &str) -> f64 {
    entity(s, id)["geometry"]["radius"].as_f64().unwrap()
}

fn count(s: &Value, kind: &str) -> usize {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .count()
}

fn no_offsets(s: &Value) -> bool {
    s["offsets"].as_array().is_none_or(Vec::is_empty)
}

/// Whether a constraint or dimension of `kind` names `id`.
fn names_curve(list: &Value, kind: &str, id: &str) -> bool {
    list.as_array()
        .unwrap()
        .iter()
        .any(|c| c["type"] == kind && c.as_object().unwrap().values().any(|v| v == id))
}

fn shown_notes(result: &Value) -> Vec<String> {
    result["note_messages"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// A sketch with a line from (0, 0) to (20, 0) and an arc about (20, 10)
/// from its end round to (30, 10), tangent with `tangent`, offset 3 mm to
/// the right of the chain (outward): a line copy from (0, -3) to (20, -3)
/// and an arc copy of radius 13 from there to (33, 10). The source line and
/// arc, the copies (line, arc) and the offset's id.
fn offset_chain(tangent: bool) -> (Document<MockKernel>, [String; 2], [String; 2], String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [20, 0]}),
    )["made"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let corner = entity(&sketch(&d), &line)["end"]
        .as_str()
        .unwrap()
        .to_owned();
    let half = 10.0 * std::f64::consts::FRAC_1_SQRT_2;
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": corner, "through": [20.0 + half, 10.0 - half], "end": [30, 10]}),
    )["made"][0]
        .as_str()
        .unwrap()
        .to_owned();
    if tangent {
        command(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                   "constraint": {"type": "tangent", "a": line, "b": arc}}),
        );
    }
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [line, arc], "distance": 3}),
    );
    let copies: Vec<String> = made["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(copies.len(), 2, "{made}");
    let offset = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert!(!no_offsets(&s), "{s}");
    assert!(near(ends(&s, &copies[0]).0, [0.0, -3.0]), "{s}");
    assert!(near(ends(&s, &copies[0]).1, [20.0, -3.0]), "{s}");
    assert!((radius(&s, &copies[1]) - 13.0).abs() < 1e-6, "{s}");
    (
        d,
        [line, arc],
        [copies[0].clone(), copies[1].clone()],
        offset,
    )
}

/// The source line's length set to 25 (a change of the source), then the
/// design recomputed.
fn change_source(d: &mut Document<MockKernel>, line: &str) {
    command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": line}, "value": 25}),
    );
    command(d, json!({"cmd": "recompute"}));
    let s = sketch(d);
    let (a, b) = ends(&s, line);
    assert!(
        ((b[0] - a[0]).hypot(b[1] - a[1]) - 25.0).abs() < 1e-6,
        "{s}"
    );
}

/// What ties the copies to the source is gone: no record, no parallel or
/// distance between them, no helper line, and the arc copy has a centre of
/// its own.
fn assert_free(s: &Value, source: &[String; 2], copies: &[String; 2]) {
    assert!(no_offsets(s), "{s}");
    assert!(
        !names_curve(&s["constraints"], "parallel", &copies[0]),
        "{s}"
    );
    assert!(
        !names_curve(&s["dimensions"], "line_distance", &copies[0]),
        "{s}"
    );
    let helpers = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["construction"] == true)
        .count();
    assert_eq!(helpers, 0, "{s}");
    assert_ne!(
        entity(s, &copies[1])["center"],
        entity(s, &source[1])["center"],
        "{s}"
    );
}

#[test]
fn trim_cuts_an_offsets_line_and_arc_and_they_stay_cut() {
    let (mut d, source, copies, _) = offset_chain(false);
    // Two lines across the line copy (and its source) at x = 5 and 15, one
    // across the arc copy at y = 5.
    for x in [5, 15] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [x, -10], "end": [x, 5]}),
        );
    }
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [25, 5], "end": [40, 5]}),
    );
    // The arc copy's end trimmed off beyond y = 5, where it crosses at
    // (32, 5): the first trim frees the offset and says so.
    let pick = (-10.0f64).to_radians();
    let trimmed = command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": copies[1],
               "at": [20.0 + 13.0 * pick.cos(), 10.0 + 13.0 * pick.sin()]}),
    );
    assert!(
        shown_notes(&trimmed).iter().any(|n| n == FREED),
        "{trimmed}"
    );
    // A work step of its own that names the arc copy as changed, which
    // stays the offset's step's (mitcad#1750).
    let step = trimmed["step"]
        .as_str()
        .expect("the trim's step")
        .to_owned();
    let s = sketch(&d);
    let listed = s["steps"].as_array().unwrap();
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| a.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default()
    };
    let own = listed
        .iter()
        .find(|x| x["id"] == step.as_str())
        .unwrap_or_else(|| panic!("no {step} in {listed:?}"));
    assert_eq!(own["kind"], "trim", "{listed:?}");
    assert_eq!(
        strings(&own["changed"]),
        std::slice::from_ref(&copies[1]),
        "{listed:?}"
    );
    assert!(
        listed
            .iter()
            .any(|x| x["kind"] == "offset" && strings(&x["items"]).contains(&copies[1])),
        "{listed:?}"
    );
    // The line copy trimmed in the middle, between x = 5 and 15: no offset
    // is left to free.
    let middle = command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": copies[0], "at": [10, -3]}),
    );
    assert!(shown_notes(&middle).is_empty(), "{middle}");
    let s = sketch(&d);
    assert_free(&s, &source, &copies);
    let check = |s: &Value| {
        // The line copy keeps its first piece, a new line its second.
        let (a, b) = ends(s, &copies[0]);
        assert!(near(a, [0.0, -3.0]) && near(b, [5.0, -3.0]), "{s}");
        let second = s["entities"].as_array().unwrap().iter().any(|e| {
            e["type"] == "line" && {
                let g = &e["geometry"];
                near(xy(&g["start"]), [15.0, -3.0]) && near(xy(&g["end"]), [20.0, -3.0])
            }
        });
        assert!(second, "{s}");
        // The arc copy ends where it was cut.
        let (start, end) = ends(s, &copies[1]);
        assert!(near(start, [20.0, -3.0]) && near(end, [32.0, 5.0]), "{s}");
        assert!((radius(s, &copies[1]) - 13.0).abs() < 1e-6, "{s}");
        // Source line, two pieces, three cutters; source arc and its copy.
        assert_eq!(count(s, "line"), 6, "{s}");
        assert_eq!(count(s, "arc"), 2, "{s}");
        assert!(no_offsets(s), "{s}");
    };
    check(&s);
    // The source is as it was.
    let (a, b) = ends(&s, &source[0]);
    assert!(near(a, [0.0, 0.0]) && near(b, [20.0, 0.0]), "{s}");
    assert!((radius(&s, &source[1]) - 10.0).abs() < 1e-6, "{s}");
    // A change of the source and a recompute make nothing again.
    change_source(&mut d, &source[0]);
    check(&sketch(&d));
}

#[test]
fn convert_to_sketch_geometry_frees_every_curve_of_the_offset() {
    let (mut d, source, copies, offset) = offset_chain(false);
    let done = command(
        &mut d,
        json!({"cmd": "sketch.convert_projected", "sketch": "F1", "entities": [copies[1]]}),
    );
    assert_eq!(done["offsets"], json!([offset]), "{done}");
    let converted: Vec<&str> = done["converted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        converted.contains(&copies[0].as_str()) && converted.contains(&copies[1].as_str()),
        "{done}"
    );
    assert!(shown_notes(&done).iter().any(|n| n == FREED), "{done}");
    let s = sketch(&d);
    assert_free(&s, &source, &copies);
    // The copies stay where they are when the source changes.
    change_source(&mut d, &source[0]);
    let s = sketch(&d);
    let (a, b) = ends(&s, &copies[0]);
    assert!(near(a, [0.0, -3.0]) && near(b, [20.0, -3.0]), "{s}");
    assert!((radius(&s, &copies[1]) - 13.0).abs() < 1e-6, "{s}");
    assert!(no_offsets(&s), "{s}");
    // A trim of a freed copy cuts it, with nothing more to free.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, -10], "end": [10, 5]}),
    );
    let trimmed = command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": copies[0], "at": [5, -3]}),
    );
    assert!(shown_notes(&trimmed).is_empty(), "{trimmed}");
    let s = sketch(&d);
    let (a, b) = ends(&s, &copies[0]);
    assert!(near(a, [10.0, -3.0]) && near(b, [20.0, -3.0]), "{s}");
    command(&mut d, json!({"cmd": "recompute"}));
    let s = sketch(&d);
    let (a, b) = ends(&s, &copies[0]);
    assert!(near(a, [10.0, -3.0]) && near(b, [20.0, -3.0]), "{s}");
    assert!(no_offsets(&s), "{s}");
}

#[test]
fn deleting_the_offset_leaves_its_curves_as_plain_geometry() {
    let (mut d, source, copies, offset) = offset_chain(false);
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [offset]}),
    );
    let s = sketch(&d);
    assert!(entity(&s, &copies[0])["type"] == "line", "{s}");
    assert!(entity(&s, &copies[1])["type"] == "arc", "{s}");
    assert_free(&s, &source, &copies);
    change_source(&mut d, &source[0]);
    let s = sketch(&d);
    let (a, b) = ends(&s, &copies[0]);
    assert!(near(a, [0.0, -3.0]) && near(b, [20.0, -3.0]), "{s}");
    assert!((radius(&s, &copies[1]) - 13.0).abs() < 1e-6, "{s}");
    // Trimmed after, it stays cut.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, -10], "end": [10, 5]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": copies[0], "at": [15, -3]}),
    );
    command(&mut d, json!({"cmd": "recompute"}));
    let s = sketch(&d);
    let (a, b) = ends(&s, &copies[0]);
    assert!(near(a, [0.0, -3.0]) && near(b, [10.0, -3.0]), "{s}");
    assert!(no_offsets(&s), "{s}");
}

#[test]
fn a_freed_offset_keeps_its_tangent_corner_as_a_constraint() {
    let (mut d, source, copies, _) = offset_chain(true);
    command(
        &mut d,
        json!({"cmd": "sketch.convert_projected", "sketch": "F1", "entities": [copies[0]]}),
    );
    let s = sketch(&d);
    assert_free(&s, &source, &copies);
    let tangent = s["constraints"].as_array().unwrap().iter().any(|c| {
        c["type"] == "tangent"
            && ((c["a"] == copies[0].as_str() && c["b"] == copies[1].as_str())
                || (c["a"] == copies[1].as_str() && c["b"] == copies[0].as_str()))
    });
    assert!(tangent, "{s}");
}
