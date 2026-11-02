// SPDX-License-Identifier: MIT
//! Offsets made by constraints keep their tangent corners where the curves
//! touch (mitcad#890), with the mock kernel. A rounded outline projected
//! from a face (fixed curves whose lines run on into their arcs) is offset,
//! and the offset is offset again, as an imported grid design's slot and
//! chute outlines are. Each offset's line and arc share their corner point;
//! without more than their own constraints (the line parallel to its source
//! at the distance, the arc about its source's centre) that point was fixed
//! only to the second order: it could slide along the line by the square
//! root of the solve's rounding (2e-5 mm), the arc then ended past the point
//! where the line touches it, and the profile turned there by 1e-6 rad. A
//! sweep along the extruded outline then carried those turns round the path
//! and its cut left rings of the chute as bodies of their own.

use std::f64::consts::{FRAC_PI_2, PI};

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

/// The corner radius of the projected outline.
const RADIUS: f64 = 12.2;

/// A face's outline `width` by 40 with corners rounded by [`RADIUS`], at
/// z = 20, counter-clockwise: each line runs on tangentially into the next
/// arc.
fn rounded(width: f64) -> Vec<Curve3> {
    let (r, h) = (RADIUS, 40.0);
    let line = |a: [f64; 2], b: [f64; 2]| Curve3::Line {
        start: [a[0], a[1], 20.0],
        end: [b[0], b[1], 20.0],
    };
    let arc = |c: [f64; 2], start: f64| Curve3::Conic {
        center: [c[0], c[1], 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: r,
        minor: r,
        start,
        end: start + FRAC_PI_2,
        closed: false,
    };
    vec![
        line([r, 0.0], [width - r, 0.0]),
        arc([width - r, r], -FRAC_PI_2),
        line([width, r], [width, h - r]),
        arc([width - r, h - r], 0.0),
        line([width - r, h], [r, h]),
        arc([r, h - r], FRAC_PI_2),
        line([0.0, h - r], [0.0, r]),
        arc([r, r], PI),
    ]
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

/// The corners two results of an offset share: (line, arc, corner point).
fn corners(s: &Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for offset in s["offsets"].as_array().unwrap() {
        let results: Vec<&str> = offset["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        for (i, a) in results.iter().enumerate() {
            let b = results[(i + 1) % results.len()];
            let (ea, eb) = (entity(s, a), entity(s, b));
            let ends = |e: &Value| [e["start"].clone(), e["end"].clone()];
            let Some(corner) = ends(ea).into_iter().find(|p| ends(eb).contains(p)) else {
                continue;
            };
            let corner = corner.as_str().unwrap().to_owned();
            match (ea["type"].as_str(), eb["type"].as_str()) {
                (Some("line"), Some("arc")) => out.push((a.to_string(), b.to_string(), corner)),
                (Some("arc"), Some("line")) => out.push((b.to_string(), a.to_string(), corner)),
                _ => {}
            }
        }
    }
    out
}

/// How far from square to the radius the line runs at the corner (the
/// cosine of the angle between the line and the radius there): 0 where the
/// line touches the arc.
fn turn(s: &Value, line: &str, arc: &str, corner: &str) -> f64 {
    let l = entity(s, line);
    let (a, b) = (
        at(s, l["start"].as_str().unwrap()),
        at(s, l["end"].as_str().unwrap()),
    );
    let c = at(s, entity(s, arc)["center"].as_str().unwrap());
    let p = at(s, corner);
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [p[0] - c[0], p[1] - c[1]];
    (u[0] * v[0] + u[1] * v[1]) / (u[0].hypot(u[1]) * v[0].hypot(v[1]))
}

/// A 112 x 40 x 20 block whose top face's outline is [`rounded`], and a
/// sketch on it (F3) with that outline projected (linked, fixed) and
/// offset in by 2, and that in by 3 again (an offset of an offset).
fn offset_outline() -> Document<MockKernel> {
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
    *d.kernel().curves.borrow_mut() = rounded(112.0);
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    let outline: Vec<String> = projected["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e.as_str())
        .filter(|e| e.starts_with('c'))
        .map(str::to_owned)
        .collect();
    assert_eq!(outline.len(), 8);
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": outline, "distance": -2}),
    );
    let first = sketch(&d, "F3")["offsets"][0]["results"].clone();
    assert_eq!(first.as_array().unwrap().len(), 8);
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": first, "distance": -3}),
    );
    d
}

#[test]
fn offsets_of_a_projected_rounded_outline_hold_their_corners_where_the_curves_touch() {
    let mut d = offset_outline();
    let s = sketch(&d, "F3");
    let tangent = corners(&s);
    assert_eq!(tangent.len(), 16, "{s}");
    // Nothing of the projection or the offsets follows from the rest, and
    // their helpers do not slide (mitcad#1171): no warning.
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");

    // Each corner is fixed to the first order by the tangency: the solve
    // finds it fully constrained, as the outline it follows is.
    let fixed: Vec<&str> = s["fully_constrained"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for (line, arc, corner) in &tangent {
        assert!(
            fixed.contains(&corner.as_str()),
            "the corner {corner} of {line} and {arc} is not fully constrained: {s}"
        );
    }
    // Wider by three cells of 28 mm: the projection follows the face, the
    // offsets their sources, and every corner stays where its line touches
    // its arc.
    *d.kernel().curves.borrow_mut() = rounded(196.0);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 196}),
    );
    let s = sketch(&d, "F3");
    assert!(s["warnings"].as_array().unwrap().is_empty(), "{s}");
    for (line, arc, corner) in corners(&s) {
        let off = turn(&s, &line, &arc, &corner);
        assert!(
            off.abs() < 1e-12,
            "{line} meets {arc} at {corner} turned {off}: {s}"
        );
    }
    // The inner band of the outline: 5 mm in, the corners rounded by 7.2.
    let (w, h, r) = (196.0 - 10.0, 30.0, RADIUS - 5.0);
    let inner = w * h - (4.0 - PI) * r * r;
    let areas: Vec<f64> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    assert!(
        areas.iter().any(|a| (a - inner).abs() < 1e-6),
        "no region of {inner} in {areas:?}: {s}"
    );
}

/// A sketch with held corners that cannot be solved says what contradicts,
/// as it did before the corners were held (mitcad#890): a distance between
/// the ends of a projected line that differs from its length names that
/// dimension and the fixed line, not every constraint of the offsets whose
/// corners the failed solve also held.
#[test]
fn an_over_constrained_sketch_with_held_corners_names_only_what_contradicts() {
    let mut d = offset_outline();
    let s = sketch(&d, "F3");
    let bottom = entity(&s, "c1");
    let (a, b) = (
        bottom["start"].as_str().unwrap().to_owned(),
        bottom["end"].as_str().unwrap().to_owned(),
    );
    let message = d
        .command(
            &json!({"cmd": "sketch.add_dimension", "sketch": "F3", "value": 50,
                    "dimension": {"type": "distance", "a": a, "b": b}})
            .to_string(),
        )
        .map(|r| panic!("the dimension must be refused, got {r}"))
        .unwrap_err()
        .to_string();
    // The new dimension (an id the sketch does not have) and fixed
    // geometry, nothing else.
    let named = message
        .strip_prefix("the sketch is over-constrained: ")
        .and_then(|m| m.strip_suffix(" contradict each other"))
        .unwrap_or_else(|| panic!("{message}"));
    let named: Vec<&str> = named.split(", ").flat_map(|n| n.split(" and ")).collect();
    let constraints: Vec<&str> = named
        .iter()
        .copied()
        .filter(|n| !n.ends_with(" (fixed)"))
        .collect();
    assert_eq!(constraints.len(), 1, "{message}");
    assert!(
        !s.to_string().contains(&format!("\"{}\"", constraints[0])),
        "{message}"
    );
    // The line's fixed end points (the projected curves made only of fixed
    // points are held by their points, mitcad#1171).
    let (a_fixed, b_fixed) = (format!("{a} (fixed)"), format!("{b} (fixed)"));
    assert!(
        named.iter().any(|n| *n == a_fixed || *n == b_fixed),
        "{message}"
    );
}
