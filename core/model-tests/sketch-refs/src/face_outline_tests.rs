// SPDX-License-Identifier: MIT
//! Project of a curved face as it is seen along the sketch's normal
//! (mitcad#1265), with the mock kernel's round faces
//! (`MockKernel::round_faces`): a cylinder seen from the side is the
//! rectangle of its outline, its seam left out where it is not on the
//! outline; a cone the trapezoid; seen along the axis only the circles; a
//! linked projection follows a radius change, and a projection of an
//! earlier version (no `outline`) keeps the face's edges.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::{MockKernel, RoundFace};
use mitcad_model::topo::TopoName;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    let query = json!({"query": "sketch", "uid": uid}).to_string();
    serde_json::from_str(&doc.query(&query).unwrap()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// The block's side face over c1, which the tests make round.
fn side() -> String {
    "F2:side(c1[c4,c2])"
        .parse::<TopoName>()
        .unwrap()
        .to_string()
}

/// Another side face of the block: a tube's inner face.
fn other_side() -> String {
    "F2:side(c3[c2,c4])"
        .parse::<TopoName>()
        .unwrap()
        .to_string()
}

/// A 60 x 40 block 20 high (F1, F2) whose faces of `round` (by name) are
/// those round faces.
fn block_with(round: &[(String, RoundFace)]) -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    for (name, face) in round {
        d.kernel()
            .round_faces
            .borrow_mut()
            .insert(name.clone(), *face);
    }
    d
}

/// Projects `face` into a new sketch on `plane` (F3).
fn project(d: &mut Document<MockKernel>, plane: &str, face: &str, linked: bool) -> Value {
    command(d, json!({"cmd": "sketch.create", "plane": plane}));
    let result = command(
        d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "body": "F2.b0",
               "linked": linked}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    sketch(d, "F3")
}

fn of_type<'a>(s: &'a Value, kind: &str) -> Vec<&'a Value> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .collect()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn near(p: [f64; 2], q: [f64; 2]) -> bool {
    close(p[0], q[0]) && close(p[1], q[1])
}

/// The sketch's lines are exactly these (each either way round), and it
/// has no other curves.
fn assert_lines(s: &Value, expected: &[([f64; 2], [f64; 2])]) {
    let lines = of_type(s, "line");
    assert_eq!(lines.len(), expected.len(), "{s}");
    for (a, b) in expected {
        let found = lines.iter().any(|e| {
            let (p, q) = (xy(&e["geometry"]["start"]), xy(&e["geometry"]["end"]));
            (near(p, *a) && near(q, *b)) || (near(p, *b) && near(q, *a))
        });
        assert!(found, "no line {a:?}-{b:?} in {s}");
    }
    let curves = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] != "point")
        .count();
    assert_eq!(curves, expected.len(), "{s}");
}

/// A cylinder's rectangle on the XZ sketch (x = X, y = -Z): `radius` wide
/// on each side of the axis, 50 high.
fn rectangle(radius: f64) -> Vec<([f64; 2], [f64; 2])> {
    vec![
        ([-radius, 0.0], [radius, 0.0]),
        ([-radius, -50.0], [radius, -50.0]),
        ([radius, 0.0], [radius, -50.0]),
        ([-radius, 0.0], [-radius, -50.0]),
    ]
}

#[test]
fn a_cylinders_side_face_seen_from_the_side_is_its_rectangle() {
    // The XZ sketch looks along Y. With the seam toward +Y (at right
    // angles to where the outline is) the seam would be a line down the
    // middle: it is no edge to see and is left out; the far long side is
    // the silhouette. With the seam toward +X it is on the outline and
    // comes once.
    for seam in [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [-0.6, 0.8, 0.0]] {
        let round = RoundFace::cylinder([0.0, 0.0, 0.0], 10.0, 50.0, seam);
        let mut d = block_with(&[(side(), round)]);
        let s = project(&mut d, "xz", &side(), true);
        assert_lines(&s, &rectangle(10.0));
        // The four corners, shared.
        assert_eq!(of_type(&s, "point").len(), 4, "{s}");
        assert_eq!(s["projections"][0]["outline"], true, "{s}");
        assert_eq!(s["warnings"], json!([]), "{s}");
    }
}

#[test]
fn a_tubes_inner_face_seen_from_the_side_is_its_rectangle() {
    let outer = RoundFace::cylinder([0.0, 0.0, 0.0], 10.0, 50.0, [0.0, 1.0, 0.0]);
    let inner = RoundFace::cylinder([0.0, 0.0, 0.0], 8.0, 50.0, [0.0, -1.0, 0.0]);
    let mut d = block_with(&[(side(), outer), (other_side(), inner)]);
    let s = project(&mut d, "xz", &other_side(), false);
    assert_lines(&s, &rectangle(8.0));
    // Unlinked: fixed copies without a record.
    assert_eq!(s["projections"], json!([]), "{s}");
}

#[test]
fn a_cones_side_face_seen_from_the_side_is_its_trapezoid() {
    let cone = RoundFace {
        top_radius: 5.0,
        ..RoundFace::cylinder([0.0, 0.0, 0.0], 10.0, 50.0, [0.0, 1.0, 0.0])
    };
    let mut d = block_with(&[(side(), cone)]);
    let s = project(&mut d, "xz", &side(), true);
    assert_lines(
        &s,
        &[
            ([-10.0, 0.0], [10.0, 0.0]),
            ([-5.0, -50.0], [5.0, -50.0]),
            ([10.0, 0.0], [5.0, -50.0]),
            ([-10.0, 0.0], [-5.0, -50.0]),
        ],
    );
    assert_eq!(s["warnings"], json!([]), "{s}");
}

#[test]
fn a_round_face_seen_along_its_axis_is_its_circles_only() {
    // On XY: no silhouette lines, and the seam, seen end on, no point.
    let cylinder = RoundFace::cylinder([5.0, 6.0, 0.0], 10.0, 50.0, [0.0, 1.0, 0.0]);
    let mut d = block_with(&[(side(), cylinder)]);
    let s = project(&mut d, "xy", &side(), true);
    let circles = of_type(&s, "circle");
    assert_eq!(circles.len(), 1, "{s}");
    assert!(close(
        circles[0]["geometry"]["radius"].as_f64().unwrap(),
        10.0
    ));
    assert!(
        near(xy(&circles[0]["geometry"]["center"]), [5.0, 6.0]),
        "{s}"
    );
    assert!(of_type(&s, "line").is_empty(), "{s}");
    // Only the circle's centre.
    assert_eq!(of_type(&s, "point").len(), 1, "{s}");

    let cone = RoundFace {
        top_radius: 5.0,
        ..cylinder
    };
    let mut d = block_with(&[(side(), cone)]);
    let s = project(&mut d, "xy", &side(), true);
    let mut radii: Vec<f64> = of_type(&s, "circle")
        .iter()
        .map(|c| c["geometry"]["radius"].as_f64().unwrap())
        .collect();
    radii.sort_by(f64::total_cmp);
    assert!(
        radii.len() == 2 && close(radii[0], 5.0) && close(radii[1], 10.0),
        "{s}"
    );
    assert!(of_type(&s, "line").is_empty(), "{s}");
    assert_eq!(of_type(&s, "point").len(), 2, "{s}");
}

/// The id of the line from `a` to `b` (either way round).
fn line_id(s: &Value, a: [f64; 2], b: [f64; 2]) -> String {
    of_type(s, "line")
        .into_iter()
        .find(|e| {
            let (p, q) = (xy(&e["geometry"]["start"]), xy(&e["geometry"]["end"]));
            (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))
        })
        .unwrap_or_else(|| panic!("no line {a:?}-{b:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The round face's radius changes (as a dimension of the model would)
/// and the model is computed again.
fn set_radius(d: &mut Document<MockKernel>, radius: f64) {
    let round = d.kernel().round_faces.borrow()[&side()];
    d.kernel().round_faces.borrow_mut().insert(
        side(),
        RoundFace {
            radius,
            top_radius: radius,
            ..round
        },
    );
    let changed = command(
        d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
}

#[test]
fn a_linked_projection_of_a_cylinder_follows_its_radius() {
    let round = RoundFace::cylinder([0.0, 0.0, 0.0], 10.0, 50.0, [0.0, 1.0, 0.0]);
    let mut d = block_with(&[(side(), round)]);
    let s = project(&mut d, "xz", &side(), true);
    let silhouette = line_id(&s, [-10.0, 0.0], [-10.0, -50.0]);
    let bottom = line_id(&s, [-10.0, 0.0], [10.0, 0.0]);
    set_radius(&mut d, 15.0);
    let s = sketch(&d, "F3");
    assert_lines(&s, &rectangle(15.0));
    // The same entities, moved.
    assert_eq!(line_id(&s, [-15.0, 0.0], [-15.0, -50.0]), silhouette, "{s}");
    assert_eq!(line_id(&s, [-15.0, 0.0], [15.0, 0.0]), bottom, "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(s["projections"][0].get("unmatched"), None, "{s}");
}

#[test]
fn a_projection_of_an_earlier_version_keeps_the_faces_edges() {
    // As Project made it before mitcad#1265 (no `outline`): the edge-on
    // circles and the seam down the middle, which go on following the
    // face's edges as they did.
    let round = RoundFace::cylinder([0.0, 0.0, 0.0], 10.0, 50.0, [0.0, 1.0, 0.0]);
    let mut d = block_with(&[(side(), round)]);
    let point = |id: &str, at: [f64; 2]| json!({"id": id, "type": "point", "at": at, "fixed": true, "reference": true});
    let line = |id: &str, start: &str, end: &str| {
        json!({"id": id, "type": "line", "start": start, "end": end, "fixed": true,
               "reference": true})
    };
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xz",
               "entities": [
                   point("p1", [-10.0, 0.0]), point("p2", [10.0, 0.0]), line("c3", "p1", "p2"),
                   point("p4", [-10.0, -50.0]), point("p5", [10.0, -50.0]),
                   line("c6", "p4", "p5"),
                   point("p7", [0.0, 0.0]), point("p8", [0.0, -50.0]), line("c9", "p7", "p8")],
               "projections": [{"source": side(), "body": "F2.b0", "by_geometry": true,
                                "entities": ["p1", "p2", "c3", "p4", "p5", "c6", "p7", "p8",
                                             "c9"]}]}}),
    );
    set_radius(&mut d, 15.0);
    let s = sketch(&d, "F3");
    assert_lines(
        &s,
        &[
            ([-15.0, 0.0], [15.0, 0.0]),
            ([-15.0, -50.0], [15.0, -50.0]),
            ([0.0, 0.0], [0.0, -50.0]),
        ],
    );
    assert_eq!(s["projections"][0].get("outline"), None, "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
}
