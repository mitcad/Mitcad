// SPDX-License-Identifier: MIT
//! A rounded outline at a distance round a triangle (mitcad#1774, GitHub
//! mitcad/Mitcad#324), with the mock kernel: the triangle's sides as fixed
//! projected construction lines, three lines parallel to them at the
//! distance and an arc at each corner joining them end to end.
//!
//! - Tangent arcs without radii: a Coincident of each arc's centre with its
//!   corner is implied only in part (the tangents hold the centre on the
//!   corner's bisector) and takes the radius away; it is kept, and the
//!   outline is fully defined with the distance as the radius. One the
//!   others imply in full is still refused.
//! - The arcs without tangents, centred on the corners: each radius is
//!   free (3 degrees of freedom, nothing redundant, although each arc
//!   touches the lines it ends on), and dragging an arc changes its radius.
//! - A line distance between lines that are not parallel warns and is
//!   listed; parallel again, it does not.
//! - A filleted corner scaled by 1.5 and back keeps its arc's sweep and
//!   side.
//! - Offset with Corners: Round makes the arcs itself, fully defined, and
//!   they follow Edit Offset's distance.
//! - The reporter's design (`MITCAD_MITCAD_REPORTED`/`github-324`, when it
//!   is there) after the reporter's steps.
//! - Round corners where a line meets an arc or two arcs meet at an angle
//!   (mitcad#1786): a D of a line and an arc, a closed outline with a
//!   tangent join (kept, no arc there), a lens of two arcs; each fully
//!   defined, following Edit Offset's distance; inward, trimmed.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn doc() -> Document<MockKernel> {
    Document::new(MockKernel::default())
}

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

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
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

fn radius(s: &Value, arc: &str) -> f64 {
    entity(s, arc)["geometry"]["radius"].as_f64().unwrap()
}

fn center(s: &Value, arc: &str) -> [f64; 2] {
    xy(&entity(s, arc)["geometry"]["center"])
}

/// The sine of an arc's sweep counter-clockwise from its start to its end:
/// positive for an arc shorter than a half circle.
fn side(s: &Value, arc: &str) -> f64 {
    let g = &entity(s, arc)["geometry"];
    (g["end_angle"].as_f64().unwrap() - g["start_angle"].as_f64().unwrap()).sin()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] * k, a[1] * k]
}

fn unit(a: [f64; 2]) -> [f64; 2] {
    scale(a, 1.0 / a[0].hypot(a[1]))
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// The triangle, counter-clockwise.
const TRIANGLE: [[f64; 2]; 3] = [[0.0, 0.0], [100.0, 0.0], [30.0, 60.0]];

/// The offset distance.
const D: f64 = 5.0;

/// The outward normal of the side from `p` to `q` of the triangle (on the
/// right of a counter-clockwise outline).
fn outward(p: [f64; 2], q: [f64; 2]) -> [f64; 2] {
    let u = unit(sub(q, p));
    [u[1], -u[0]]
}

/// The arc at corner `k` of radius `r` tangent to the offsets of the two
/// sides there: its centre (on the corner's bisector, the corner itself for
/// `r = D`) and where it touches the offset of the side before the corner
/// and of the side after it.
fn corner_arc(k: usize, r: f64) -> ([f64; 2], [f64; 2], [f64; 2]) {
    let t = TRIANGLE[k];
    let (prev, next) = (TRIANGLE[(k + 2) % 3], TRIANGLE[(k + 1) % 3]);
    let inward = unit(add(unit(sub(prev, t)), unit(sub(next, t))));
    let half = cross(unit(sub(next, t)), inward).abs();
    let c = add(t, scale(inward, (r - D) / half));
    let (n1, n2) = (outward(prev, t), outward(t, next));
    (c, add(c, scale(n1, r)), add(c, scale(n2, r)))
}

/// The sketch (F1): the triangle as projected construction lines p1-p3, c4
/// (p1 to p2), c5, c6; the lines c13 (on c4's side), c14 (c5's), c15 (c6's)
/// parallel to them at `D` (k22-k24 and the distances k34-k36); the arcs
/// c19 (at p2, centre p16), c20 (at p3, p17), c21 (at p1, p18) of radius
/// `r`, from the end of one line to the start of the next. With `tangents`
/// each arc is tangent to both its lines (k25-k30); with `centered` each
/// centre is coincident with its corner (k31-k33).
fn outline(r: f64, tangents: bool, centered: bool) -> Document<MockKernel> {
    let mut d = doc();
    let arcs: Vec<([f64; 2], [f64; 2], [f64; 2])> = (0..3).map(|k| corner_arc(k, r)).collect();
    // At corner 1 (p2) arc c19 from the end of c13 to the start of c14, at
    // corner 2 (p3) c20 from c14 to c15, at corner 0 (p1) c21 from c15 to
    // c13.
    let (b, c, a) = (arcs[1], arcs[2], arcs[0]);
    let mut entities = vec![
        json!({"id": "p1", "type": "point", "at": TRIANGLE[0], "fixed": true, "reference": true}),
        json!({"id": "p2", "type": "point", "at": TRIANGLE[1], "fixed": true, "reference": true}),
        json!({"id": "p3", "type": "point", "at": TRIANGLE[2], "fixed": true, "reference": true}),
        json!({"id": "c4", "type": "line", "start": "p1", "end": "p2", "construction": true,
               "fixed": true, "reference": true}),
        json!({"id": "c5", "type": "line", "start": "p2", "end": "p3", "construction": true,
               "fixed": true, "reference": true}),
        json!({"id": "c6", "type": "line", "start": "p3", "end": "p1", "construction": true,
               "fixed": true, "reference": true}),
        json!({"id": "p7", "type": "point", "at": a.2}),
        json!({"id": "p8", "type": "point", "at": b.1}),
        json!({"id": "p9", "type": "point", "at": b.2}),
        json!({"id": "p10", "type": "point", "at": c.1}),
        json!({"id": "p11", "type": "point", "at": c.2}),
        json!({"id": "p12", "type": "point", "at": a.1}),
        json!({"id": "c13", "type": "line", "start": "p7", "end": "p8"}),
        json!({"id": "c14", "type": "line", "start": "p9", "end": "p10"}),
        json!({"id": "c15", "type": "line", "start": "p11", "end": "p12"}),
        json!({"id": "p16", "type": "point", "at": b.0}),
        json!({"id": "p17", "type": "point", "at": c.0}),
        json!({"id": "p18", "type": "point", "at": a.0}),
    ];
    entities.extend([
        json!({"id": "c19", "type": "arc", "center": "p16", "start": "p8", "end": "p9"}),
        json!({"id": "c20", "type": "arc", "center": "p17", "start": "p10", "end": "p11"}),
        json!({"id": "c21", "type": "arc", "center": "p18", "start": "p12", "end": "p7"}),
    ]);
    let mut constraints = vec![
        json!({"id": "k22", "type": "parallel", "a": "c4", "b": "c13"}),
        json!({"id": "k23", "type": "parallel", "a": "c5", "b": "c14"}),
        json!({"id": "k24", "type": "parallel", "a": "c6", "b": "c15"}),
    ];
    if tangents {
        for (k, (line, arc)) in [
            ("c13", "c19"),
            ("c14", "c19"),
            ("c14", "c20"),
            ("c15", "c20"),
            ("c15", "c21"),
            ("c13", "c21"),
        ]
        .iter()
        .enumerate()
        {
            constraints.push(json!({"id": format!("k{}", 25 + k), "type": "tangent",
                                    "a": line, "b": arc}));
        }
    }
    if centered {
        for (k, (point, corner)) in [("p16", "p2"), ("p17", "p3"), ("p18", "p1")]
            .iter()
            .enumerate()
        {
            constraints.push(json!({"id": format!("k{}", 31 + k), "type": "coincident",
                                    "point": point, "entity": corner}));
        }
    }
    let distance = format!("{D} mm");
    let dimensions = vec![
        json!({"id": "k34", "type": "line_distance", "a": "c4", "b": "c13", "value": distance}),
        json!({"id": "k35", "type": "line_distance", "a": "c5", "b": "c14", "value": distance}),
        json!({"id": "k36", "type": "line_distance", "a": "c6", "b": "c15", "value": distance}),
    ];
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
               "entities": entities, "constraints": constraints, "dimensions": dimensions}}),
    );
    d
}

/// The arcs and the corners they are about.
const ARCS: [(&str, &str, usize); 3] = [("c19", "p16", 1), ("c20", "p17", 2), ("c21", "p18", 0)];

#[test]
fn a_centre_made_coincident_with_its_corner_is_kept_and_fixes_the_radius() {
    // Tangent arcs of twice the distance: their centres slide along the
    // corners' bisectors, one degree of freedom each.
    let mut d = outline(2.0 * D, true, false);
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 3, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    for (i, (arc, centre, corner)) in ARCS.iter().enumerate() {
        let before = sketch(&d, "F1")["dof"].as_u64().unwrap();
        let corner_point = format!("p{}", corner + 1);
        let result = command(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                   "constraint": {"type": "coincident", "point": centre,
                                  "entity": corner_point}}),
        );
        // Kept as it was given: nothing older removed in its favour.
        assert!(result["implied_removed"].is_null(), "{result}");
        let k = result["constraints"][0].as_str().unwrap().to_owned();
        let s = sketch(&d, "F1");
        assert!(s["error"].is_null(), "{s}");
        assert!(
            s["constraints"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == k.as_str() && c["type"] == "coincident"),
            "{s}"
        );
        assert_eq!(s["dof"].as_u64().unwrap(), before - 1, "{s}");
        assert_eq!(s["dof"].as_u64().unwrap(), 2 - i as u64, "{s}");
        // The arc about its corner, its radius the distance.
        let at = center(&s, arc);
        assert!(
            close(at[0], TRIANGLE[*corner][0]) && close(at[1], TRIANGLE[*corner][1]),
            "{arc} about {at:?}: {s}"
        );
        assert!(close(radius(&s, arc), D), "{arc}: {s}");
    }
    let s = sketch(&d, "F1");
    assert_eq!(s["dof"], 0, "fully defined: {s}");
    assert_eq!(s["conflicts"], json!([]), "{s}");
    // The redundant part of each coincidence is said in words, without
    // ids.
    for message in strings(&s["warning_messages"]) {
        assert!(!message.contains("k3"), "{message}");
    }

    // An Equal of two of the arcs takes no freedom away: refused, and the
    // refusal says so.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "equal", "a": "c19", "b": "c20"}}),
    );
    assert!(error.contains("would over-constrain"), "{error}");
    assert!(error.contains("takes no freedom away"), "{error}");
}

#[test]
fn arcs_touching_the_lines_they_end_on_keep_their_radii_free() {
    // No tangents, the centres on the corners: each arc's radius is free,
    // although each touches its lines where the solve leaves it.
    let mut d = outline(D, false, true);
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 3, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert!(
        strings(&s["warning_messages"]).is_empty(),
        "nothing follows from the others: {s}"
    );
    // The arc at p2 dragged outward along its corner's bisector: its ends
    // slide along their lines and its radius grows.
    let (t, prev, next) = (TRIANGLE[1], TRIANGLE[0], TRIANGLE[2]);
    let out = scale(unit(add(unit(sub(t, prev)), unit(sub(t, next)))), 3.0);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "c19", "by": out}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    let at = center(&s, "c19");
    assert!(close(at[0], t[0]) && close(at[1], t[1]), "{at:?}: {s}");
    assert!(radius(&s, "c19") > D + 0.5, "{s}");
    assert_eq!(s["dof"], 3, "{s}");
}

#[test]
fn a_line_distance_between_lines_that_are_not_parallel_is_flagged() {
    let mut d = doc();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
            {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true, "reference": true},
            {"id": "p2", "type": "point", "at": [100.0, 0.0], "fixed": true, "reference": true},
            {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true,
             "reference": true},
            {"id": "p4", "type": "point", "at": [0.0, 10.0]},
            {"id": "p5", "type": "point", "at": [100.0, 10.0]},
            {"id": "c6", "type": "line", "start": "p4", "end": "p5"}],
            "constraints": [{"id": "k7", "type": "parallel", "a": "c3", "b": "c6"}],
            "dimensions": [{"id": "k8", "type": "line_distance", "a": "c3", "b": "c6",
                            "value": "10 mm"}]}}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["not_parallel"], json!([]), "{s}");
    assert!(
        !strings(&s["warning_messages"])
            .iter()
            .any(|w| w.contains("not parallel")),
        "{s}"
    );
    // Without its parallel, the line turned: the distance holds its middle
    // only.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["k7"]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "p5", "to": [100.0, 20.0]}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["not_parallel"], json!(["k8"]), "{s}");
    let warnings = strings(&s["warning_messages"]);
    let i = warnings
        .iter()
        .position(|w| w.contains("between lines that are not parallel"))
        .unwrap_or_else(|| panic!("no warning: {s}"));
    assert!(!warnings[i].contains("k8"), "in words: {}", warnings[i]);
    // Its places: the two lines.
    let about = strings(&s["warning_entities"][i]);
    assert!(
        about.contains(&"c3".to_owned()) && about.contains(&"c6".to_owned()),
        "{s}"
    );
    let notices = s["notices"].as_array().cloned().unwrap_or_default();
    assert!(
        notices
            .iter()
            .any(|n| n["message"].as_str().unwrap_or("").contains("not parallel")),
        "{s}"
    );
    // Parallel again: neither.
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "parallel", "a": "c3", "b": "c6"}}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["not_parallel"], json!([]), "{s}");
    assert!(
        !strings(&s["warning_messages"])
            .iter()
            .any(|w| w.contains("not parallel")),
        "{s}"
    );
}

#[test]
fn a_filleted_corner_keeps_its_arc_when_its_dimensions_are_scaled() {
    // A corner of a horizontal and a vertical line rounded by a tangent
    // arc about (30, 10): its lengths and radius by the parameter U.
    let mut d = doc();
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "U", "expression": "10 mm"}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
            {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true},
            {"id": "p2", "type": "point", "at": [30.0, 0.0]},
            {"id": "p3", "type": "point", "at": [30.0, 10.0]},
            {"id": "p4", "type": "point", "at": [40.0, 10.0]},
            {"id": "p5", "type": "point", "at": [40.0, 50.0]},
            {"id": "c6", "type": "line", "start": "p1", "end": "p2"},
            {"id": "c7", "type": "arc", "center": "p3", "start": "p2", "end": "p4"},
            {"id": "c8", "type": "line", "start": "p4", "end": "p5"}],
            "constraints": [
                {"id": "k9", "type": "horizontal", "line": "c6"},
                {"id": "k10", "type": "vertical", "line": "c8"},
                {"id": "k11", "type": "tangent", "a": "c6", "b": "c7"},
                {"id": "k12", "type": "tangent", "a": "c7", "b": "c8"}],
            "dimensions": [
                {"id": "k13", "type": "length", "line": "c6", "value": "3 * U"},
                {"id": "k14", "type": "radius", "curve": "c7", "value": "U"},
                {"id": "k15", "type": "length", "line": "c8", "value": "4 * U"}]}}),
    );
    let check = |d: &Document<MockKernel>, u: f64| {
        let s = sketch(d, "F1");
        assert!(s["error"].is_null(), "{s}");
        assert_eq!(s["dof"], 0, "{s}");
        // On the same side of its chord, a quarter round inside the corner.
        assert!(side(&s, "c7") > 0.5, "{s}");
        let at = center(&s, "c7");
        assert!(
            close(at[0], 3.0 * u) && close(at[1], u),
            "U = {u}: the arc about {at:?}: {s}"
        );
        assert!(close(radius(&s, "c7"), u), "{s}");
    };
    check(&d, 10.0);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "U", "expression": "15 mm"}),
    );
    check(&d, 15.0);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "U", "expression": "10 mm"}),
    );
    check(&d, 10.0);
}

/// The triangle alone (F1): projected construction lines c4 (p1 to p2),
/// c5, c6, counter-clockwise.
fn triangle() -> Document<MockKernel> {
    let mut d = doc();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
            {"id": "p1", "type": "point", "at": TRIANGLE[0], "fixed": true, "reference": true},
            {"id": "p2", "type": "point", "at": TRIANGLE[1], "fixed": true, "reference": true},
            {"id": "p3", "type": "point", "at": TRIANGLE[2], "fixed": true, "reference": true},
            {"id": "c4", "type": "line", "start": "p1", "end": "p2", "construction": true,
             "fixed": true, "reference": true},
            {"id": "c5", "type": "line", "start": "p2", "end": "p3", "construction": true,
             "fixed": true, "reference": true},
            {"id": "c6", "type": "line", "start": "p3", "end": "p1", "construction": true,
             "fixed": true, "reference": true}]}}),
    );
    d
}

/// The round offset's corner arcs: each about a corner of the triangle,
/// of radius `r`, its ends those of two result lines, each line square to
/// the arc's radius there (tangent).
fn check_round_corners(s: &Value, r: f64) {
    let offset = &s["offsets"][0];
    assert_eq!(offset["round"], true, "{s}");
    let corners = offset["corners"].as_array().unwrap();
    assert_eq!(corners.len(), 3, "{s}");
    let results = strings(&offset["results"]);
    assert_eq!(results.len(), 3, "{s}");
    for corner in corners {
        let arc = corner["arc"].as_str().unwrap();
        let e = entity(s, arc);
        assert_eq!(e["type"], "arc", "{s}");
        // Its centre is the corner point itself.
        let centre = e["center"].as_str().unwrap();
        assert!(["p1", "p2", "p3"].contains(&centre), "{arc}: {s}");
        let c = center(s, arc);
        assert!(
            TRIANGLE
                .iter()
                .any(|t| close(c[0], t[0]) && close(c[1], t[1])),
            "{arc} about {c:?}: {s}"
        );
        assert!(close(radius(s, arc), r), "{arc}: {s}");
        assert!(side(s, arc) > 0.0, "an outer round: {s}");
        for end in ["start", "end"] {
            let p = e[end].as_str().unwrap();
            let line = results
                .iter()
                .find(|l| {
                    let line = entity(s, l);
                    line["start"] == p || line["end"] == p
                })
                .unwrap_or_else(|| panic!("{p} of {arc} ends no result: {s}"));
            let g = &entity(s, line)["geometry"];
            let along = sub(xy(&g["end"]), xy(&g["start"]));
            let at = xy(&entity(s, p)["at"]);
            let radial = sub(at, c);
            assert!(
                dot(unit(along), unit(radial)).abs() < 1e-6,
                "{line} is tangent to {arc} at {p}: {s}"
            );
        }
    }
}

#[test]
fn a_round_offset_puts_arcs_about_the_outer_corners() {
    let mut d = triangle();
    let result = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c4", "c5", "c6"],
               "distance": D, "corners": "round"}),
    );
    let k = result["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    check_round_corners(&s, D);
    assert_eq!(s["dof"], 0, "fully defined by the offset: {s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    // No radius of their own.
    assert!(
        !s["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["type"] == "radius"),
        "{s}"
    );
    // Edit Offset: the arcs follow the distance; Corners are kept.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "distance": 8}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    check_round_corners(&s, 8.0);
    assert_eq!(s["dof"], 0, "{s}");
    // Sharp again: the results meet where they cross, no arc.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "corners": "sharp"}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    let offset = &s["offsets"][0];
    assert!(offset["round"].is_null(), "{s}");
    assert_eq!(offset["corners"], Value::Null, "{s}");
    assert!(
        !s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "arc"),
        "{s}"
    );
    // And round once more.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "corners": "round"}),
    );
    let s = sketch(&d, "F1");
    check_round_corners(&s, 8.0);
}

#[test]
fn a_round_offset_inward_keeps_its_corners_trimmed() {
    // Inward every corner turns toward the offset: no arc.
    let mut d = triangle();
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c4", "c5", "c6"],
               "distance": -D, "corners": "round"}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert!(s["offsets"][0]["corners"].is_null(), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
}

/// The reporter's design of GitHub mitcad/Mitcad#324 at `version` (a
/// folder under `github-324`, or the folder itself for ""), loaded with the
/// mock kernel; None, and a note, when the private test files are not here.
fn reported(version: &str) -> Option<Document<MockKernel>> {
    let root = std::env::var("MITCAD_MITCAD_REPORTED").ok()?;
    let path = std::path::Path::new(&root)
        .join("github-324")
        .join(version)
        .join("StrandbeestAttempt3.mitcad");
    if !path.is_file() {
        eprintln!("skipped: {} is not here", path.display());
        return None;
    }
    let mut d = Document::load_design(&path, MockKernel::default())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    d.recompute();
    Some(d)
}

#[test]
fn the_reported_design_solves_after_the_reporters_steps() {
    let Some(mut d) = reported("") else {
        return;
    };
    // GoodRoundedOffset: fully defined as it was.
    let s = sketch(&d, "F31");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    // BadRoundedOffest1: the radii are free, nothing redundant.
    let s = sketch(&d, "F30");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 3, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    // Its arc about p3 dragged outward along its middle: the radius grows,
    // and stays when the sketch is solved again.
    let g = &entity(&s, "c13")["geometry"];
    let (start, end) = (
        g["start_angle"].as_f64().unwrap(),
        g["end_angle"].as_f64().unwrap(),
    );
    let middle = (start + end) / 2.0;
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F30", "entity": "c13",
               "by": [3.0 * middle.cos(), 3.0 * middle.sin()]}),
    );
    d.recompute();
    let s = sketch(&d, "F30");
    assert!(s["error"].is_null(), "{s}");
    // Its ends lie on lines 15.95 from its centre, so its radius is
    // sqrt(15.95^2 + t^2) for the slide t of each end along its line; the
    // pull along the corner's middle slides each by its share along the
    // line, 3 mm times the sine of half the arc's sweep (the line and the
    // corner's middle meet at a right angle less half the sweep). The radius
    // grows only with the square of the slide: 3 mm make it 16.19, not 19.
    let slide = (radius(&s, "c13").powi(2) - 15.95f64.powi(2)).sqrt();
    let share = 3.0 * ((end - start) / 2.0).sin();
    assert!(slide > 1.0, "{slide}: {s}");
    assert!((slide - share).abs() < 0.05, "{slide} / {share}: {s}");
    assert_eq!(s["dof"], 3, "{s}");
    // BadRoundedOffset2: each centre made coincident with its corner.
    for (centre, corner) in [("p16", "p3"), ("p20", "p5"), ("p24", "p2")] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F32",
                   "constraint": {"type": "coincident", "point": centre, "entity": corner}}),
        );
    }
    let s = sketch(&d, "F32");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    for arc in ["c13", "c17", "c21"] {
        assert!((radius(&s, arc) - 15.95).abs() < 1e-6, "{arc}: {s}");
    }
}

#[test]
fn the_reported_line_distances_between_turned_lines_are_flagged() {
    let Some(d) = reported("16dd280") else {
        return;
    };
    // Sketch3: two distances to lines whose parallels were deleted.
    let s = sketch(&d, "F34");
    assert!(s["error"].is_null(), "{s}");
    let flagged = strings(&s["not_parallel"]);
    assert!(!flagged.is_empty(), "{s}");
    assert!(
        flagged.iter().all(|k| k == "k5" || k == "k7"),
        "{flagged:?}: {s}"
    );
    assert!(
        strings(&s["warning_messages"])
            .iter()
            .any(|w| w.contains("not parallel")),
        "{s}"
    );
}

#[test]
fn the_reported_corner_arc_keeps_its_side_from_scale_4_to_6_and_back() {
    let Some(mut d) = reported("55fefb5") else {
        return;
    };
    // Sketch3's lower corner arc (c21), at the configuration's Scale 4.
    let s = sketch(&d, "F34");
    assert!(s["error"].is_null(), "{s}");
    let before = side(&s, "c21");
    assert!(before.abs() > 0.1, "{s}");
    for scale in ["6", "4"] {
        command(
            &mut d,
            json!({"cmd": "set_parameter", "name": "Scale", "expression": scale}),
        );
        let s = sketch(&d, "F34");
        assert!(s["error"].is_null(), "Scale {scale}: {s}");
        assert_eq!(
            side(&s, "c21").signum(),
            before.signum(),
            "Scale {scale}: the corner arc flipped: {s}"
        );
    }
}

// Corners: Round at arcs (mitcad#1786): where a line meets an arc or two
// arcs meet at an angle, as between two lines.

/// A sketch (F1) of fixed projected construction curves: `points` (`p1`,
/// `p2`, … in order) and `curves` (each a JSON entity without its id, which
/// follows the points' as `c<n>`).
fn projected(points: &[[f64; 2]], curves: &[Value]) -> Document<MockKernel> {
    let mut d = doc();
    let mut entities: Vec<Value> = points
        .iter()
        .enumerate()
        .map(|(k, at)| {
            json!({"id": format!("p{}", k + 1), "type": "point", "at": at, "fixed": true,
                   "reference": true})
        })
        .collect();
    for (k, curve) in curves.iter().enumerate() {
        let mut e = curve.clone();
        e["id"] = json!(format!("c{}", points.len() + k + 1));
        e["construction"] = json!(true);
        e["fixed"] = json!(true);
        e["reference"] = json!(true);
        entities.push(e);
    }
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
               "entities": entities}}),
    );
    d
}

/// The round offset's corner arcs: one about each of `corners` (the
/// sources' corner points), of radius `r`, an outer round (less than a half
/// circle), its ends those of the two results it joins and tangent to each
/// there: a line square to the arc's radius, an arc whose centre lies on
/// that radius.
fn check_corner_arcs(s: &Value, corners: &[&str], r: f64) {
    let offset = &s["offsets"][0];
    assert_eq!(offset["round"], true, "{s}");
    let arcs = offset["corners"]
        .as_array()
        .unwrap_or_else(|| panic!("no corner arcs: {s}"));
    assert_eq!(arcs.len(), corners.len(), "{s}");
    let results = strings(&offset["results"]);
    let mut about: Vec<String> = Vec::new();
    for corner in arcs {
        let arc = corner["arc"].as_str().unwrap();
        let e = entity(s, arc);
        assert_eq!(e["type"], "arc", "{s}");
        // Its centre is the corner point itself.
        let centre = e["center"].as_str().unwrap();
        assert!(corners.contains(&centre), "{arc} about {centre}: {s}");
        about.push(centre.to_owned());
        let c = center(s, arc);
        let at = xy(&entity(s, centre)["at"]);
        assert!(close(c[0], at[0]) && close(c[1], at[1]), "{arc}: {s}");
        assert!(close(radius(s, arc), r), "{arc}: {s}");
        assert!(side(s, arc) > 0.0, "an outer round: {s}");
        for end in ["start", "end"] {
            let p = e[end].as_str().unwrap();
            let result = results
                .iter()
                .find(|id| {
                    let x = entity(s, id);
                    x["start"] == p || x["end"] == p
                })
                .unwrap_or_else(|| panic!("{p} of {arc} ends no result: {s}"));
            let x = entity(s, result);
            let point = xy(&entity(s, p)["at"]);
            let radial = unit(sub(point, c));
            if x["type"] == "line" {
                let g = &x["geometry"];
                let along = unit(sub(xy(&g["end"]), xy(&g["start"])));
                assert!(
                    dot(along, radial).abs() < 1e-6,
                    "{result} is tangent to {arc} at {p}: {s}"
                );
            } else {
                assert_eq!(x["type"], "arc", "{s}");
                let to = unit(sub(point, center(s, result)));
                assert!(
                    cross(to, radial).abs() < 1e-6,
                    "{result} is tangent to {arc} at {p}: {s}"
                );
            }
        }
    }
    about.sort();
    about.dedup();
    assert_eq!(about.len(), corners.len(), "one arc per corner: {s}");
}

/// The results of the offset that are arcs.
fn result_arcs(s: &Value) -> Vec<String> {
    strings(&s["offsets"][0]["results"])
        .into_iter()
        .filter(|r| entity(s, r)["type"] == "arc")
        .collect()
}

/// Fully defined by the offset: no error, no freedom, nothing redundant,
/// and no radius dimension of the corner arcs' own.
fn check_defined(s: &Value) {
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 0, "fully defined by the offset: {s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert!(
        !s["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["type"] == "radius"),
        "{s}"
    );
}

/// The radius of the arc of the D outline and of the lens.
fn chord_radius() -> f64 {
    1300f64.sqrt()
}

/// A D: the line c4 from p1 (0, 0) to p2 (60, 0) and the arc c5 about p3
/// (30, -20) from p2 back to p1 over the top, counter-clockwise; the arc
/// meets the line at an angle at both ends.
fn d_outline() -> Document<MockKernel> {
    projected(
        &[[0.0, 0.0], [60.0, 0.0], [30.0, -20.0]],
        &[
            json!({"type": "line", "start": "p1", "end": "p2"}),
            json!({"type": "arc", "center": "p3", "start": "p2", "end": "p1"}),
        ],
    )
}

#[test]
fn a_round_offset_puts_arcs_where_a_line_meets_an_arc() {
    let mut d = d_outline();
    let result = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c4", "c5"],
               "distance": D, "corners": "round"}),
    );
    let k = result["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    check_defined(&s);
    check_corner_arcs(&s, &["p1", "p2"], D);
    // The line's copy at the distance below it, the arc's about its centre
    // at the distance outside it.
    let arcs = result_arcs(&s);
    assert_eq!(arcs.len(), 1, "{s}");
    let at = center(&s, &arcs[0]);
    assert!(close(at[0], 30.0) && close(at[1], -20.0), "{s}");
    assert!(close(radius(&s, &arcs[0]), chord_radius() + D), "{s}");
    for r in strings(&s["offsets"][0]["results"]) {
        let x = entity(&s, &r);
        if x["type"] == "line" {
            let g = &x["geometry"];
            assert!(close(xy(&g["start"])[1], -D), "{s}");
            assert!(close(xy(&g["end"])[1], -D), "{s}");
        }
    }

    // Edit Offset: the corner arcs and the copies follow the distance;
    // Corners are kept.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "distance": 8}),
    );
    let s = sketch(&d, "F1");
    check_defined(&s);
    check_corner_arcs(&s, &["p1", "p2"], 8.0);
    let arcs = result_arcs(&s);
    assert!(close(radius(&s, &arcs[0]), chord_radius() + 8.0), "{s}");

    // Sharp: the copies meet where they cross, no corner arc (the source's
    // arc and its copy are the only arcs).
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "corners": "sharp"}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert!(s["offsets"][0]["corners"].is_null(), "{s}");
    let arcs = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "arc")
        .count();
    assert_eq!(arcs, 2, "{s}");
    // And round once more.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "corners": "round"}),
    );
    let s = sketch(&d, "F1");
    check_defined(&s);
    check_corner_arcs(&s, &["p1", "p2"], 8.0);
}

#[test]
fn a_round_offset_inward_of_a_line_and_an_arc_keeps_its_corners_trimmed() {
    let mut d = d_outline();
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c4", "c5"],
               "distance": -D, "corners": "round"}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert!(s["offsets"][0]["corners"].is_null(), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
}

#[test]
fn a_round_offset_keeps_a_tangent_join_and_rounds_the_angles() {
    // The line c5 from p1 (0, 0) to p2 (40, 0), the arc c6 about p3
    // (40, 20) from p2 to p4 (60, 20), tangent to the line at p2, and the
    // line c7 from p4 back to p1, at an angle to the arc and to c5.
    let mut d = projected(
        &[[0.0, 0.0], [40.0, 0.0], [40.0, 20.0], [60.0, 20.0]],
        &[
            json!({"type": "line", "start": "p1", "end": "p2"}),
            json!({"type": "arc", "center": "p3", "start": "p2", "end": "p4"}),
            json!({"type": "line", "start": "p4", "end": "p1"}),
        ],
    );
    let result = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c5", "c6", "c7"],
               "distance": D, "corners": "round"}),
    );
    let k = result["constraints"][0].as_str().unwrap().to_owned();
    let check = |s: &Value, r: f64| {
        check_defined(s);
        // An arc where the arc meets c7 and where c7 meets c5, none at the
        // tangent join.
        check_corner_arcs(s, &["p1", "p4"], r);
        // The copies of c5 and of the arc share their end below p2.
        let results = strings(&s["offsets"][0]["results"]);
        let line = entity(s, &results[0]);
        let arc = entity(s, &results[1]);
        assert_eq!(line["type"], "line", "{s}");
        assert_eq!(arc["type"], "arc", "{s}");
        let shared = [&line["start"], &line["end"]]
            .into_iter()
            .find(|p| **p == arc["start"] || **p == arc["end"])
            .unwrap_or_else(|| panic!("the tangent join is one point: {s}"));
        let at = xy(&entity(s, shared.as_str().unwrap())["at"]);
        assert!(close(at[0], 40.0) && close(at[1], -r), "{at:?}: {s}");
        assert!(close(radius(s, &results[1]), 20.0 + r), "{s}");
    };
    check(&sketch(&d, "F1"), D);
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "distance": 8}),
    );
    check(&sketch(&d, "F1"), 8.0);
}

#[test]
fn a_round_offset_puts_arcs_where_two_arcs_meet_at_an_angle() {
    // A lens: the arc c5 about p3 (30, 20) from p1 (0, 0) to p2 (60, 0)
    // below, and c6 about p4 (30, -20) from p2 back to p1 above.
    let mut d = projected(
        &[[0.0, 0.0], [60.0, 0.0], [30.0, 20.0], [30.0, -20.0]],
        &[
            json!({"type": "arc", "center": "p3", "start": "p1", "end": "p2"}),
            json!({"type": "arc", "center": "p4", "start": "p2", "end": "p1"}),
        ],
    );
    let result = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c5", "c6"],
               "distance": D, "corners": "round"}),
    );
    let k = result["constraints"][0].as_str().unwrap().to_owned();
    let check = |s: &Value, r: f64| {
        check_defined(s);
        check_corner_arcs(s, &["p1", "p2"], r);
        let arcs = result_arcs(s);
        assert_eq!(arcs.len(), 2, "{s}");
        for arc in &arcs {
            assert!(close(radius(s, arc), chord_radius() + r), "{arc}: {s}");
        }
    };
    check(&sketch(&d, "F1"), D);
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": k, "distance": 8}),
    );
    check(&sketch(&d, "F1"), 8.0);
}
