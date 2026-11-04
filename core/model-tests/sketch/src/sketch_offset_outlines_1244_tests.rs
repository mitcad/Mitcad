// SPDX-License-Identifier: MIT
//! Sketch offsets whose trimmed result falls into several closed outlines,
//! whose lines run on along each other across a notch the offset closes,
//! and offsets of offsets made again with them (mitcad#1244), with the mock
//! kernel. A dog-bone offset inward past its waist gives two closed
//! outlines in one offset record (`outlines`); the walls on either side of
//! the waist lie on one line with those across it, and a rectangle's side
//! with a narrow notch the offset closes is one line (`merged`). An offset
//! of an offset whose middle arc vanishes when the first one's distance
//! grows is made again from the first one's new results.

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

/// The closed polyline through `outline` at z = 20, the mock kernel's
/// outline of the block's top face.
fn polyline(outline: &[[f64; 2]]) -> Vec<Curve3> {
    let n = outline.len();
    (0..n)
        .map(|k| {
            let (a, b) = (outline[k], outline[(k + 1) % n]);
            Curve3::Line {
                start: [a[0], a[1], 20.0],
                end: [b[0], b[1], 20.0],
            }
        })
        .collect()
}

/// A 112 x 40 x 20 block and a sketch on its top face (F3) with the closed
/// polyline `outline` (counter-clockwise) projected onto it (linked, fixed,
/// so that its offsets are fully constrained); its curves.
fn projected(outline: &[[f64; 2]]) -> (Document<MockKernel>, Vec<String>) {
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
    *d.kernel().curves.borrow_mut() = polyline(outline);
    let made = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    let curves: Vec<String> = names(&made["entities"])
        .into_iter()
        .filter(|e| e.starts_with('c'))
        .collect();
    assert_eq!(curves.len(), outline.len());
    (d, curves)
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

/// The two pieces of the notched rectangles' bottom side (both ends at
/// y = 10), left then right.
fn bottom_pieces(s: &Value, curves: &[String]) -> [String; 2] {
    let mut pieces: Vec<(f64, String)> = curves
        .iter()
        .filter_map(|c| {
            let [a, b] = ends_of(s, c);
            let (a, b) = (at(s, &a), at(s, &b));
            let bottom = (a[1] - 10.0).abs() < 1e-6 && (b[1] - 10.0).abs() < 1e-6;
            bottom.then(|| (a[0] + b[0], c.clone()))
        })
        .collect();
    pieces.sort_by(|p, q| p.0.total_cmp(&q.0));
    assert_eq!(pieces.len(), 2, "{s}");
    [pieces[0].1.clone(), pieces[1].1.clone()]
}

/// A dog-bone, two 20 mm squares joined by a waist 10 mm long and 4 mm
/// wide, offset 3 mm inward: past the waist's half width, so that the
/// waist's walls vanish and the offset falls into two squares of 14 mm.
/// The walls of each square on either side of the waist lie on one line,
/// so each square is four lines (`merged`). Both outlines are in the one
/// offset record (`outlines`), fully constrained as their projected source
/// is, and stay so when the design is saved and opened again.
#[test]
fn a_dog_bone_offset_inward_past_its_waist_falls_into_two_outlines() {
    let outline = [
        [10.0, 10.0],
        [30.0, 10.0],
        [30.0, 18.0],
        [40.0, 18.0],
        [40.0, 10.0],
        [60.0, 10.0],
        [60.0, 30.0],
        [40.0, 30.0],
        [40.0, 22.0],
        [30.0, 22.0],
        [30.0, 30.0],
        [10.0, 30.0],
    ];
    let (mut d, curves) = projected(&outline);
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": curves, "distance": -3}),
    );
    let id = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    let results = names(&record["results"]);
    assert_eq!(results.len(), 8, "{s}");
    // The two waist walls vanish, and a wall on either side of the waist
    // runs on along the one across it.
    assert_eq!(names(&record["dropped"]).len(), 4, "{s}");
    let merged = record["merged"].as_array().unwrap();
    assert_eq!(merged.len(), 2, "{s}");
    for m in merged {
        let (curve, into) = (m["curve"].as_str().unwrap(), m["into"].as_str().unwrap());
        assert!(names(&record["dropped"]).iter().any(|c| c == curve), "{s}");
        assert!(names(&record["curves"]).iter().any(|c| c == into), "{s}");
    }
    let outlines = record["outlines"].as_array().unwrap();
    assert_eq!(outlines.len(), 2, "{s}");
    for o in outlines {
        let o = names(o);
        assert_eq!(o.len(), 4, "{s}");
        assert!(closed_lines(&s, &o), "{s}");
    }
    assert!(closed_lines(&s, &results), "{s}");
    assert_eq!(regions_of(&s, 14.0 * 14.0), 2, "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    let notes = names(&made["notes"]);
    assert!(
        notes.iter().any(|n| n.contains("falls into 2 outlines")),
        "{made}"
    );
    assert!(notes.iter().any(|n| n.contains("runs on along")), "{made}");

    // Saved and opened again: the same record, still fully constrained.
    let json = d.to_json();
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json, "saving again gives the same file");
    *loaded.kernel().curves.borrow_mut() = polyline(&outline);
    loaded.recompute();
    let again = sketch(&loaded, "F3");
    assert_eq!(offset(&again, &id), offset(&s, &id), "{again}");
    assert_eq!(again["dof"], 0, "{again}");
    assert_eq!(regions_of(&again, 14.0 * 14.0), 2, "{again}");
}

/// A 40 x 20 rectangle with a 4 mm wide, 5 mm deep notch in its bottom
/// side, offset 3 mm past the notch's half width to the side that closes
/// it: a notch cut into the side offset outward, and one standing out of
/// the side offset inward. The notch's walls and bottom vanish, and the
/// bottom side's two pieces lie on one line, so the offset is one
/// rectangle of four lines, the later piece running on along the first's
/// result (`merged`), fully constrained as its projected source is. The
/// region it bounds: outward the ring of 46 x 26 less the source's 780
/// mm2, inward 34 x 14.
#[test]
fn a_rectangle_with_a_narrow_notch_offset_past_it_is_four_lines() {
    for (depth, distance, area) in [(5.0, 3.0, 416.0), (-5.0, -3.0, 476.0)] {
        let outline = [
            [10.0, 10.0],
            [28.0, 10.0],
            [28.0, 10.0 + depth],
            [32.0, 10.0 + depth],
            [32.0, 10.0],
            [50.0, 10.0],
            [50.0, 30.0],
            [10.0, 30.0],
        ];
        let (mut d, curves) = projected(&outline);
        let made = command(
            &mut d,
            json!({"cmd": "sketch.offset", "sketch": "F3", "curves": curves,
                   "distance": distance}),
        );
        let id = made["constraints"][0].as_str().unwrap().to_owned();
        let s = sketch(&d, "F3");
        let record = offset(&s, &id);
        let results = names(&record["results"]);
        assert_eq!(results.len(), 4, "{distance}: {s}");
        assert!(closed_lines(&s, &results), "{distance}: {s}");
        assert_eq!(names(&record["dropped"]).len(), 4, "{distance}: {s}");
        let merged = record["merged"].as_array().unwrap();
        assert_eq!(merged.len(), 1, "{distance}: {s}");
        // The bottom side's later piece (on the right) runs on along the
        // first's result.
        let [left, right] = bottom_pieces(&s, &curves);
        assert_eq!(merged[0]["into"], left.as_str(), "{distance}: {s}");
        assert_eq!(merged[0]["curve"], right.as_str(), "{distance}: {s}");
        assert!(record.get("outlines").is_none(), "{distance}: {s}");
        assert_eq!(regions_of(&s, area), 1, "{distance}: {s}");
        assert_eq!(s["dof"], 0, "{distance}: {s}");
        assert_eq!(s["redundant"], json!([]), "{distance}: {s}");
        assert!(
            names(&made["notes"])
                .iter()
                .any(|n| n.contains("runs on along")),
            "{distance}: {made}"
        );
    }
}

/// The notched rectangle offset 1 mm outward, the notch still open, and a
/// driven length on the result of the bottom side's right piece. At 3 mm
/// the notch closes: that piece runs on along the left one's result, and
/// the dimension goes onto that result, not removed.
#[test]
fn a_dimension_on_a_piece_that_runs_on_along_another_goes_onto_its_result() {
    let outline = [
        [10.0, 10.0],
        [28.0, 10.0],
        [28.0, 15.0],
        [32.0, 15.0],
        [32.0, 10.0],
        [50.0, 10.0],
        [50.0, 30.0],
        [10.0, 30.0],
    ];
    let (mut d, curves) = projected(&outline);
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": curves, "distance": 1}),
    );
    let id = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    assert!(record.get("dropped").is_none(), "{s}");
    let sources = names(&record["curves"]);
    let results = names(&record["results"]);
    let result_of = |c: &str| results[sources.iter().position(|x| x == c).unwrap()].clone();
    let [left, right] = bottom_pieces(&s, &curves);
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "length", "line": result_of(&right)}, "driven": true}),
    );
    let length = added["dimensions"][0].as_str().unwrap().to_owned();

    let edited = command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F3", "offset": id, "distance": 3}),
    );
    let s = sketch(&d, "F3");
    let record = offset(&s, &id);
    assert_eq!(names(&record["results"]).len(), 4, "{s}");
    assert_eq!(record["merged"][0]["curve"], right.as_str(), "{s}");
    assert_eq!(record["merged"][0]["into"], left.as_str(), "{s}");
    let dimension = s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["id"] == length.as_str())
        .unwrap_or_else(|| panic!("{length} is gone: {edited}"));
    assert_eq!(dimension["line"], result_of(&left).as_str(), "{s}");
    assert!(
        names(&edited["warnings"])
            .iter()
            .all(|w| !w.contains(&length)),
        "{edited}"
    );
}

/// An open chain of a line, a 1 mm arc and a line offset 0.5 mm into its
/// turn, and that offset offset 0.2 mm further. When the first offset's
/// distance grows to 2 mm its arc vanishes: the second offset, whose source
/// curve vanished, is made again from the first one's two lines (not
/// refused), keeping its id and the ids of its lines' results; their corner
/// is where their lines cross, (8.8, 2.2). Back at 0.5 mm the arc comes
/// back in both.
#[test]
fn an_offset_of_an_offset_whose_middle_arc_vanishes_is_made_again() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    // c1 from p2 to p3, c4 from p5 to p6.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [11, 1], "end": [11, 11]}),
    );
    let through = [10.0 + 0.5f64.sqrt(), 1.0 - 0.5f64.sqrt()];
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": "p3", "through": through, "end": "p5"}),
    );
    let arc = names(&arc["made"])
        .into_iter()
        .find(|c| c.starts_with('c'))
        .unwrap();
    // Into the turn: to the left of the chain from c1 on.
    let first = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", arc, "c4"],
               "distance": -0.5}),
    );
    let first = first["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    let made = names(&offset(&s, &first)["results"]);
    assert_eq!(made.len(), 3, "{s}");
    let second = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": made, "distance": -0.2}),
    );
    let second = second["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    let outer = names(&offset(&s, &second)["results"]);
    assert_eq!(outer.len(), 3, "{s}");

    // 2 mm: the first offset's arc vanishes, the second is made again.
    let edited = command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": first, "distance": 2}),
    );
    let s = sketch(&d, "F1");
    let a = offset(&s, &first);
    assert_eq!(names(&a["dropped"]), vec![arc.clone()], "{s}");
    let lines = names(&a["results"]);
    assert_eq!(lines, vec![made[0].clone(), made[2].clone()], "{s}");
    let b = offset(&s, &second);
    let mut sources = names(&b["curves"]);
    sources.sort();
    let mut expected = lines.clone();
    expected.sort();
    assert_eq!(sources, expected, "{s}");
    let results = names(&b["results"]);
    assert_eq!(results.len(), 2, "{s}");
    assert_eq!((&results[0], &results[1]), (&outer[0], &outer[2]), "{s}");
    let [a0, a1] = ends_of(&s, &results[0]);
    let [b0, b1] = ends_of(&s, &results[1]);
    let corner = [&a0, &a1]
        .into_iter()
        .find(|p| **p == b0 || **p == b1)
        .unwrap_or_else(|| panic!("no shared corner: {s}"));
    assert!(near(at(&s, corner), [8.8, 2.2]), "{s}");
    assert!(
        names(&edited["notes"]).iter().any(|n| n.contains("vanish")),
        "{edited}"
    );

    // Back to 0.5 mm: the arc comes back in both offsets.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": first, "distance": 0.5}),
    );
    let s = sketch(&d, "F1");
    let a = offset(&s, &first);
    assert!(names(&a["dropped"]).is_empty(), "{s}");
    let back = names(&a["results"]);
    assert_eq!(back.len(), 3, "{s}");
    let b = offset(&s, &second);
    assert_eq!(names(&b["curves"]).len(), 3, "{s}");
    assert!(names(&b["curves"]).contains(&back[1]), "{s}");
    let results = names(&b["results"]);
    assert_eq!(results.len(), 3, "{s}");
    assert_eq!(entity(&s, &results[1])["type"], "arc", "{s}");
}
