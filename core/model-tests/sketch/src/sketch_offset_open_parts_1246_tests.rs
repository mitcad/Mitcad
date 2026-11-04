// SPDX-License-Identifier: MIT
//! Sketch offsets of open chains that fall into parts, of arcs on one
//! circle across a notch the offset closes, of a tapered waist whose walls'
//! offsets cross, and of a dog-bone with a lobe too small for the distance
//! (mitcad#1246), with the mock kernel. An open dog-bone (without one wall
//! of its waist) offset inward past its waist gives two open parts in one
//! record (`outlines` and `ends`); a circle drawn as two arcs with a narrow
//! notch, offset outward past it, gives one circle; a dog-bone whose waist
//! is a trapezoid gives two outlines whose curves do not cross; a dog-bone
//! whose one lobe is narrower than twice the distance gives the other
//! lobe's outline and a note.

use std::f64::consts::PI;

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

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// The offset record `id` of a sketch.
fn offset<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["offsets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap_or_else(|| panic!("no offset {id} in {s}"))
}

/// How many profile regions of the sketch have `area`.
fn regions_of(s: &Value, area: f64) -> usize {
    s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| (r["area"].as_f64().unwrap() - area).abs() < 1e-6 * area)
        .count()
}

/// A curve's start and end points.
fn ends_of(s: &Value, c: &str) -> [String; 2] {
    let e = entity(s, c);
    [
        e["start"].as_str().unwrap().to_owned(),
        e["end"].as_str().unwrap().to_owned(),
    ]
}

/// A line at z = 20 from `a` to `b`.
fn line(a: [f64; 2], b: [f64; 2]) -> Curve3 {
    Curve3::Line {
        start: [a[0], a[1], 20.0],
        end: [b[0], b[1], 20.0],
    }
}

/// The closed polyline through `outline`.
fn polyline(outline: &[[f64; 2]]) -> Vec<Curve3> {
    let n = outline.len();
    (0..n)
        .map(|k| line(outline[k], outline[(k + 1) % n]))
        .collect()
}

/// The open polyline through `points`.
fn open_polyline(points: &[[f64; 2]]) -> Vec<Curve3> {
    points.windows(2).map(|w| line(w[0], w[1])).collect()
}

/// A 112 x 40 x 20 block and a sketch on its top face (F3) with `curves`
/// (the mock kernel's outline of that face, at z = 20) projected onto it
/// (linked, fixed, so that their offsets are fully constrained); the
/// projected curves.
fn projected(curves: &[Curve3]) -> (Document<MockKernel>, Vec<String>) {
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
    *d.kernel().curves.borrow_mut() = curves.to_vec();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    let outline: Vec<String> = names(&made["entities"])
        .into_iter()
        .filter(|e| e.starts_with('c'))
        .collect();
    assert_eq!(outline.len(), curves.len());
    (d, outline)
}

/// Whether curve `c` runs between `a` and `b` (either way).
fn between(s: &Value, c: &str, a: [f64; 2], b: [f64; 2]) -> bool {
    let [p, q] = ends_of(s, c).map(|e| at(s, &e));
    (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))
}

/// The projected `curves` of F3 in the order of the polyline through
/// `points` (closed with `closed`), each found by its ends: the order the
/// offset's chain then runs in, from the first point on.
fn in_order(
    d: &Document<MockKernel>,
    curves: &[String],
    points: &[[f64; 2]],
    closed: bool,
) -> Vec<String> {
    let s = sketch(d, "F3");
    let n = points.len();
    let segments = if closed { n } else { n - 1 };
    (0..segments)
        .map(|k| {
            let (a, b) = (points[k], points[(k + 1) % n]);
            curves
                .iter()
                .find(|c| between(&s, c, a, b))
                .unwrap_or_else(|| panic!("no curve from {a:?} to {b:?}: {s}"))
                .clone()
        })
        .collect()
}

/// Offsets `curves` of F3 by `distance`: the command's result and the
/// offset's id.
fn offset_by(d: &mut Document<MockKernel>, curves: &[String], distance: f64) -> (Value, String) {
    let made = command(
        d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": curves, "distance": distance}),
    );
    let id = made["constraints"][0].as_str().unwrap().to_owned();
    (made, id)
}

/// Whether every curve of `results` is a line whose ends each another
/// result shares: the results make closed outlines of lines.
fn closed_lines(s: &Value, results: &[String]) -> bool {
    let mut points: Vec<String> = Vec::new();
    for r in results {
        if entity(s, r)["type"] != "line" {
            return false;
        }
        points.extend(ends_of(s, r));
    }
    points.sort();
    let all = points.len();
    points.dedup();
    points.len() * 2 == all
}

/// Whether each result of `outline` but the last shares its end point with
/// the start of the next: an open outline joined in its order.
fn joined_in_order(s: &Value, outline: &[String]) -> bool {
    outline
        .windows(2)
        .all(|w| ends_of(s, &w[0])[1] == ends_of(s, &w[1])[0])
}

/// Whether two of the lines `results` cross each other away from their
/// ends.
fn any_crossing(s: &Value, results: &[String]) -> bool {
    let segment = |c: &String| {
        let [a, b] = ends_of(s, c);
        (at(s, &a), at(s, &b))
    };
    let cross = |u: [f64; 2], v: [f64; 2]| u[0] * v[1] - u[1] * v[0];
    for (k, a) in results.iter().enumerate() {
        for b in &results[k + 1..] {
            let ((p0, p1), (q0, q1)) = (segment(a), segment(b));
            let r = [p1[0] - p0[0], p1[1] - p0[1]];
            let t = [q1[0] - q0[0], q1[1] - q0[1]];
            let den = cross(r, t);
            if den.abs() < 1e-12 {
                continue;
            }
            let w = [q0[0] - p0[0], q0[1] - p0[1]];
            let (x, y) = (cross(w, t) / den, cross(w, r) / den);
            if x > 1e-9 && x < 1.0 - 1e-9 && y > 1e-9 && y < 1.0 - 1e-9 {
                return true;
            }
        }
    }
    false
}

/// The dog-bone of mitcad#1244 (two 20 mm squares joined by a waist 10 mm
/// long and 4 mm wide) without the top wall of its waist: an open chain
/// from (30, 22) round the left square, along the waist's bottom wall and
/// round the right square to (40, 22), counter-clockwise.
const OPEN_DOG_BONE: [[f64; 2]; 12] = [
    [30.0, 22.0],
    [30.0, 30.0],
    [10.0, 30.0],
    [10.0, 10.0],
    [30.0, 10.0],
    [30.0, 18.0],
    [40.0, 18.0],
    [40.0, 10.0],
    [60.0, 10.0],
    [60.0, 30.0],
    [40.0, 30.0],
    [40.0, 22.0],
];

/// The open dog-bone offset 6 mm inward: past the waist, whose bottom
/// wall's offset lies in the overlap (nearer the waist's sides than the
/// distance) and vanishes. The offsets of the sides on either side of it
/// are parallel and do not meet, and nothing pairs them across, so the
/// offset ends on both sides there, each end at the distance from the
/// wall's ends, (24, 18) and (46, 18): two open parts of five lines, from
/// the chain's start (24, 22) and to its end (46, 22), in one record whose
/// `ends` are their last results. Each part is held as an open chain's
/// offset is: but for its two free ends, which slide along their lines,
/// fully constrained, so the sketch has two degrees of freedom more than
/// with the offset at 1 mm, one part. The record is saved and read back
/// the same.
#[test]
fn an_open_dog_bone_offset_inward_past_its_waist_falls_into_two_open_parts() {
    let (mut d, curves) = projected(&open_polyline(&OPEN_DOG_BONE));
    let curves = in_order(&d, &curves, &OPEN_DOG_BONE, false);
    let (_, id) = offset_by(&mut d, &curves, -1.0);
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    assert_eq!(names(&record["results"]).len(), 11, "{s}");
    assert!(record.get("outlines").is_none(), "{s}");
    assert!(record.get("ends").is_none(), "{s}");
    let one_part = s["dof"].as_i64().unwrap();

    let (mut d, curves) = projected(&open_polyline(&OPEN_DOG_BONE));
    let curves = in_order(&d, &curves, &OPEN_DOG_BONE, false);
    let (made, id) = offset_by(&mut d, &curves, -6.0);
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    let results = names(&record["results"]);
    assert_eq!(results.len(), 10, "{s}");
    // The waist's bottom wall, from (30, 18) to (40, 18), vanishes.
    let dropped = names(&record["dropped"]);
    assert_eq!(dropped.len(), 1, "{s}");
    let wall = &dropped[0];
    assert!(between(&s, wall, [30.0, 18.0], [40.0, 18.0]), "{s}");
    assert!(record.get("merged").is_none(), "{s}");
    let outlines: Vec<Vec<String>> = record["outlines"]
        .as_array()
        .unwrap()
        .iter()
        .map(names)
        .collect();
    assert_eq!(outlines.len(), 2, "{s}");
    let last: Vec<String> = outlines.iter().filter_map(|o| o.last().cloned()).collect();
    assert_eq!(names(&record["ends"]), last, "{s}");
    for o in &outlines {
        assert_eq!(o.len(), 5, "{s}");
        assert!(joined_in_order(&s, o), "{o:?}: {s}");
        assert!(o.iter().all(|r| entity(&s, r)["type"] == "line"), "{s}");
    }
    // Each part runs from a free start to a free end: the chain's own ends
    // and the ends on either side of the waist.
    let mut free: Vec<[f64; 2]> = outlines
        .iter()
        .flat_map(|o| {
            let [start, _] = ends_of(&s, &o[0]);
            let [_, end] = ends_of(&s, &o[o.len() - 1]);
            [at(&s, &start), at(&s, &end)]
        })
        .collect();
    free.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1])));
    let expected = [[24.0, 18.0], [24.0, 22.0], [46.0, 18.0], [46.0, 22.0]];
    assert!(
        free.iter().zip(&expected).all(|(p, q)| near(*p, *q)),
        "{free:?}: {s}"
    );
    assert_eq!(s["dof"].as_i64().unwrap(), one_part + 2, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    let notes = names(&made["notes"]);
    assert!(
        notes.iter().any(|n| n.contains("falls into 2 outlines")),
        "{made}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains(wall.as_str()) && n.contains("vanish")),
        "{made}"
    );

    // Saved and opened again: the same record.
    let json = d.to_json();
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json, "saving again gives the same file");
    *loaded.kernel().curves.borrow_mut() = open_polyline(&OPEN_DOG_BONE);
    loaded.recompute();
    let again = sketch(&loaded, "F3");
    assert_eq!(offset(&again, &id), offset(&s, &id), "{again}");
}

/// The notched circle's centre and radius.
const CENTER: [f64; 2] = [56.0, 20.0];
const RADIUS: f64 = 10.0;

/// A circle of [`RADIUS`] about [`CENTER`] drawn as two arcs (from the
/// notch round the left to the bottom, and from the bottom round the right
/// to the notch, as a projection splits a circle), with a notch 2 mm wide
/// and 3 mm deep cut into its top: counter-clockwise, at z = 20.
fn notched_circle() -> Vec<Curve3> {
    let (c, r) = (CENTER, RADIUS);
    let on = |t: f64| [c[0] + r * t.cos(), c[1] + r * t.sin()];
    let arc = |start: f64, end: f64| Curve3::Conic {
        center: [CENTER[0], CENTER[1], 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: RADIUS,
        minor: RADIUS,
        start,
        end,
        closed: false,
    };
    // The notch's sides 1 mm either side of the top.
    let half = (1.0 / RADIUS).asin();
    let (left, right) = (PI / 2.0 + half, PI / 2.0 - half);
    let (p, q) = (on(left), on(right));
    let (p_low, q_low) = ([p[0], p[1] - 3.0], [q[0], q[1] - 3.0]);
    vec![
        arc(left, 1.5 * PI),
        arc(-0.5 * PI, right),
        line(q, q_low),
        line(q_low, p_low),
        line(p_low, p),
    ]
}

/// The notched circle offset 2 mm outward: past the notch's half width, so
/// that the notch's walls and bottom vanish. The two arcs' offsets lie on
/// one circle across it: the first runs on along the second's (`merged`),
/// and the second, alone in its outline, runs round its whole circle. The
/// result is one circle of 12 mm about the source's centre, held by the
/// offset's helper across the arc's middle, fully constrained as its
/// projected source is.
#[test]
fn a_circle_with_a_narrow_notch_offset_outward_past_it_is_one_circle() {
    let (mut d, curves) = projected(&notched_circle());
    let (made, id) = offset_by(&mut d, &curves, 2.0);
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    let results = names(&record["results"]);
    assert_eq!(results.len(), 1, "{s}");
    let circle = entity(&s, &results[0]);
    assert_eq!(circle["type"], "circle", "{s}");
    let radius = circle["radius"].as_f64().unwrap();
    assert!((radius - 12.0).abs() < 1e-6, "{radius}: {s}");
    assert!(
        near(at(&s, circle["center"].as_str().unwrap()), CENTER),
        "{s}"
    );
    let dropped = names(&record["dropped"]);
    assert_eq!(dropped.len(), 4, "{s}");
    let merged = record["merged"].as_array().unwrap();
    assert_eq!(merged.len(), 1, "{s}");
    let (curve, into) = (
        merged[0]["curve"].as_str().unwrap(),
        merged[0]["into"].as_str().unwrap(),
    );
    assert_ne!(curve, into, "{s}");
    assert_eq!(entity(&s, curve)["type"], "arc", "{s}");
    assert_eq!(entity(&s, into)["type"], "arc", "{s}");
    assert!(dropped.iter().any(|c| c == curve), "{s}");
    assert!(record.get("outlines").is_none(), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    let notes = names(&made["notes"]);
    assert!(notes.iter().any(|n| n.contains("runs on along")), "{made}");
    assert!(notes.iter().any(|n| n.contains("vanish")), "{made}");
}

/// A dog-bone whose waist is a trapezoid: two 20 mm squares joined by a
/// neck whose bottom wall rises from (30, 15) to (40, 18) and whose top
/// wall falls from (40, 22) to (30, 25), 10 mm wide at the left square and
/// 4 mm at the right one.
const TAPERED: [[f64; 2]; 12] = [
    [10.0, 10.0],
    [30.0, 10.0],
    [30.0, 15.0],
    [40.0, 18.0],
    [40.0, 10.0],
    [60.0, 10.0],
    [60.0, 30.0],
    [40.0, 30.0],
    [40.0, 22.0],
    [30.0, 25.0],
    [30.0, 30.0],
    [10.0, 30.0],
];

/// The tapered dog-bone offset 3 mm inward: the walls' offsets cross where
/// the neck is about 6 mm wide, at (30 + (10 - 6 sqrt 1.09) / 0.6, 20), a
/// point at the distance from both walls; beyond it they lie on the
/// source's side. The outline is split there: the left square's outline
/// runs along the bottom wall's offset into the crossing and back along
/// the top wall's (seven lines), and the right square's sides on either
/// side of the neck lie on one line (`merged`): a 14 mm square. Two
/// outlines in one record, no two curves crossing, fully constrained as
/// the projected source is.
#[test]
fn a_tapered_waist_offset_inward_past_it_falls_into_two_outlines_that_do_not_cross() {
    let (mut d, curves) = projected(&polyline(&TAPERED));
    let curves = in_order(&d, &curves, &TAPERED, true);
    let (made, id) = offset_by(&mut d, &curves, -3.0);
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    let results = names(&record["results"]);
    assert_eq!(results.len(), 11, "{s}");
    assert!(closed_lines(&s, &results), "{s}");
    assert!(!any_crossing(&s, &results), "{s}");
    assert_eq!(names(&record["dropped"]).len(), 1, "{s}");
    assert_eq!(record["merged"].as_array().unwrap().len(), 1, "{s}");
    let mut sizes: Vec<usize> = record["outlines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let o = names(o);
            assert!(closed_lines(&s, &o), "{o:?}: {s}");
            o.len()
        })
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![4, 7], "{s}");
    assert!(record.get("ends").is_none(), "{s}");
    // The walls' results meet where their offsets cross.
    let crossing = [30.0 + (10.0 - 6.0 * 1.09f64.sqrt()) / 0.6, 20.0];
    let corner = results
        .iter()
        .flat_map(|r| ends_of(&s, r))
        .any(|p| near(at(&s, &p), crossing));
    assert!(corner, "{crossing:?}: {s}");
    assert_eq!(regions_of(&s, 14.0 * 14.0), 1, "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert!(
        names(&made["notes"])
            .iter()
            .any(|n| n.contains("falls into 2 outlines")),
        "{made}"
    );
}

/// A dog-bone whose right lobe is narrower than twice the distance: a
/// 20 mm square, a waist 10 mm long and 6 mm wide, and a lobe 8 mm wide
/// and 20 mm tall.
const SMALL_LOBE: [[f64; 2]; 12] = [
    [10.0, 10.0],
    [30.0, 10.0],
    [30.0, 17.0],
    [40.0, 17.0],
    [40.0, 10.0],
    [48.0, 10.0],
    [48.0, 30.0],
    [40.0, 30.0],
    [40.0, 23.0],
    [30.0, 23.0],
    [30.0, 30.0],
    [10.0, 30.0],
];

/// The dog-bone with the small lobe offset 4.5 mm inward: the waist's walls
/// and the lobe's walls along the waist vanish, and of the lobe only its
/// far side is left, which cannot make an outline on its own while the
/// square's sides on either side of the waist lie on one line. It is left
/// out with a note, and the rest is the square's outline, 11 mm, fully
/// constrained as the projected source is.
#[test]
fn a_dog_bone_with_a_lobe_smaller_than_the_distance_keeps_the_other_lobe() {
    let (mut d, curves) = projected(&polyline(&SMALL_LOBE));
    let curves = in_order(&d, &curves, &SMALL_LOBE, true);
    let (made, id) = offset_by(&mut d, &curves, -4.5);
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    let results = names(&record["results"]);
    assert_eq!(results.len(), 4, "{s}");
    assert!(closed_lines(&s, &results), "{s}");
    assert!(record.get("outlines").is_none(), "{s}");
    assert_eq!(names(&record["dropped"]).len(), 8, "{s}");
    assert_eq!(regions_of(&s, 11.0 * 11.0), 1, "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    // The lobe's far side, from (48, 10) to (48, 30), is left out.
    let far = curves
        .iter()
        .find(|c| between(&s, c, [48.0, 10.0], [48.0, 30.0]))
        .unwrap_or_else(|| panic!("no far side: {s}"));
    assert!(names(&record["dropped"]).contains(far), "{s}");
    assert!(
        names(&made["notes"])
            .iter()
            .any(|n| n.contains("the offset leaves out") && n.contains(far.as_str())),
        "{made}"
    );
}
