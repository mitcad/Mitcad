// SPDX-License-Identifier: MIT
//! Insert Points (mitcad#2232) through the JSON api, with the mock kernel:
//! `sketch.import_points` and the `points_info` query on point files and
//! airfoils made here from the NACA 4-digit formula (no coordinate file of
//! a database is committed).

use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::{MockKernel, unique_temp_dir};
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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))
}

/// A new document with an empty Sketch1 (F1).
fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn entities(s: &Value) -> &Vec<Value> {
    s["entities"].as_array().unwrap()
}

fn entity(s: &Value, id: &str) -> Value {
    entities(s)
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

/// Where a point entity is.
fn at(s: &Value, id: &str) -> [f64; 2] {
    xy(&entity(s, id)["at"])
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

fn near(a: [f64; 2], b: [f64; 2], within: f64) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= within
}

/// A point of a NACA 4-digit airfoil of chord 1 (camber `m` at `p`,
/// thickness `t`); `sharp` closes the trailing edge.
fn naca_point(code: [f64; 3], x: f64, sharp: bool, upper: bool) -> [f64; 2] {
    let [m, p, t] = code;
    let a4 = if sharp { 0.1036 } else { 0.1015 };
    let yt = 5.0
        * t
        * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x.powi(3) - a4 * x.powi(4));
    let (yc, slope) = if x < p {
        (
            m / (p * p) * (2.0 * p * x - x * x),
            2.0 * m / (p * p) * (p - x),
        )
    } else {
        let q = (1.0 - p) * (1.0 - p);
        (
            m / q * ((1.0 - 2.0 * p) + 2.0 * p * x - x * x),
            2.0 * m / q * (p - x),
        )
    };
    let theta = slope.atan();
    if upper {
        [x - yt * theta.sin(), yc + yt * theta.cos()]
    } else {
        [x + yt * theta.sin(), yc - yt * theta.cos()]
    }
}

const NACA2412: [f64; 3] = [0.02, 0.4, 0.12];

/// A surface from the leading edge to the trailing edge, cosine spacing.
fn surface(count: usize, sharp: bool, upper: bool) -> Vec<[f64; 2]> {
    (0..count)
        .map(|k| {
            let x = 0.5 * (1.0 - (PI * k as f64 / (count - 1) as f64).cos());
            naca_point(NACA2412, x, sharp, upper)
        })
        .collect()
}

fn line(out: &mut String, p: [f64; 2], decimals: usize) {
    writeln!(out, " {:.d$}  {:.d$}", p[0], p[1], d = decimals).unwrap();
}

/// A Selig file of NACA 2412.
fn selig(upper: usize, lower: usize, sharp: bool, decimals: usize) -> String {
    let mut out = String::from("NACA 2412\n");
    for p in surface(upper, sharp, true).iter().rev() {
        line(&mut out, *p, decimals);
    }
    for p in surface(lower, sharp, false).iter().skip(1) {
        line(&mut out, *p, decimals);
    }
    out
}

/// A Lednicer file of NACA 2412.
fn lednicer(upper: usize, lower: usize, decimals: usize) -> String {
    let mut out = format!("NACA 2412\n{upper}.  {lower}.\n\n");
    for p in surface(upper, false, true) {
        line(&mut out, p, decimals);
    }
    out.push('\n');
    for p in surface(lower, false, false) {
        line(&mut out, p, decimals);
    }
    out
}

/// The text written to a file of this name in a folder of the test's own.
fn file(test: &str, name: &str, text: &str) -> PathBuf {
    let path = unique_temp_dir(test).join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn import(d: &mut Document<MockKernel>, path: &Path, extra: Value) -> Value {
    let mut request = json!({"cmd": "sketch.import_points", "sketch": "F1",
                             "path": path.to_str().unwrap()});
    for (key, value) in extra.as_object().unwrap() {
        request[key] = value.clone();
    }
    command(d, request)
}

#[test]
fn chord_and_angle_place_a_sharp_airfoil_and_its_ends_share_one_point() {
    let path = file("points-2232-chord", "naca2412.dat", &selig(61, 61, true, 6));
    let mut d = with_sketch();
    let angle = 5f64.to_radians();
    let made = import(
        &mut d,
        &path,
        json!({"chord": 150, "angle": angle, "at": [20, 10]}),
    );
    assert_eq!(made["airfoil"], true, "{made}");
    assert_eq!(made["airfoil_name"], "NACA 2412");
    assert_eq!(made["closed"], false);
    assert_eq!(made["fit"], "least_squares");
    assert!(made["closing_line"].is_null(), "sharp: no line, {made}");
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    let spline = entity(&s, made["spline"].as_str().unwrap());
    assert_eq!(spline["type"], "spline", "{spline}");
    let control = ids(&spline["control"]);
    // The open spline's two ends are one point: the sharp trailing edge,
    // 150 mm from the leading edge at -5° from +X.
    assert_eq!(control.first(), control.last(), "{spline}");
    let te = at(&s, &control[0]);
    let expected = [20.0 + 150.0 * angle.cos(), 10.0 - 150.0 * angle.sin()];
    assert!(near(te, expected, 1e-6), "{te:?} {expected:?}");
    let le = at(&s, made["leading_edge"].as_str().unwrap());
    assert!(near(le, [20.0, 10.0], 1e-9), "{le:?}");
    // One profile: the airfoil's outline.
    assert_eq!(s["regions"].as_array().unwrap().len(), 1, "{s}");
}

#[test]
fn a_blunt_trailing_edge_gets_its_closing_line() {
    let path = file(
        "points-2232-blunt",
        "naca2412.dat",
        &selig(61, 61, false, 6),
    );
    let mut d = with_sketch();
    let made = import(&mut d, &path, json!({}));
    assert_eq!(made["curves"], 2, "{made}");
    let s = sketch(&d);
    let spline = entity(&s, made["spline"].as_str().unwrap());
    let control = ids(&spline["control"]);
    assert_ne!(control.first(), control.last());
    let closing = entity(&s, made["closing_line"].as_str().unwrap());
    assert_eq!(closing["type"], "line");
    assert_eq!(closing["start"], control.last().unwrap().as_str());
    assert_eq!(closing["end"], control.first().unwrap().as_str());
    assert_eq!(s["regions"].as_array().unwrap().len(), 1, "{s}");
    // Without the closing line the profile stays open.
    let mut open = with_sketch();
    let made = import(&mut open, &path, json!({"close_trailing_edge": false}));
    assert!(made["closing_line"].is_null());
    assert_eq!(made["curves"], 1);
}

#[test]
fn a_lednicer_files_counts_line_is_not_a_point() {
    let path = file("points-2232-lednicer", "naca2412.dat", &lednicer(40, 40, 6));
    let d = Document::new(MockKernel::default());
    let info = query(
        &d,
        json!({"query": "points_info", "path": path.to_str().unwrap()}),
    );
    assert_eq!(info["airfoil"], true, "{info}");
    assert_eq!(info["layout"], "lednicer");
    assert_eq!(info["points"], 80);
    assert_eq!(info["profile_points"], 79);
    assert_eq!(info["name"], "NACA 2412");
    // As plain points: both surfaces, the counts line nowhere.
    let mut d = with_sketch();
    let made = import(&mut d, &path, json!({"airfoil": false, "as": "points"}));
    assert_eq!(made["points"], 80, "{made}");
    let s = sketch(&d);
    assert!(
        entities(&s)
            .iter()
            .all(|e| e["type"] == "point" && xy(&e["at"])[0] <= 1.01),
        "{s}"
    );
}

#[test]
fn airfoil_false_reads_the_same_file_as_plain_points() {
    let path = file(
        "points-2232-plain",
        "naca2412.dat",
        &selig(40, 40, false, 6),
    );
    let mut d = with_sketch();
    let made = import(&mut d, &path, json!({"airfoil": false}));
    assert_eq!(made["airfoil"], false, "{made}");
    assert!(made["airfoil_name"].is_null());
    assert!(made["closing_line"].is_null() && made["leading_edge"].is_null());
    assert_eq!(made["curves"], 1);
    // In the file's numbers (scale 1, mm): the spline starts on the file's
    // first point, the upper trailing edge.
    let s = sketch(&d);
    let spline = entity(&s, made["spline"].as_str().unwrap());
    let control = ids(&spline["control"]);
    let te = at(&s, &control[0]);
    let first = surface(40, false, true)[39];
    assert!(near(te, first, 1e-6), "{te:?} {first:?}");
}

#[test]
fn the_cap_of_60_control_points_warns() {
    let mut text = String::new();
    for k in 0..1200 {
        let x = k as f64 * 0.2;
        writeln!(text, "{x} {}", (x * 30.0 * 2.0 * PI / 240.0).sin() * 10.0).unwrap();
    }
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.import_points", "sketch": "F1", "text": text, "tolerance": 0.001}),
    );
    assert_eq!(made["control_points"], 60, "{made}");
    let warnings = made["warnings"].to_string();
    assert!(
        warnings.contains("the fitted spline stops at 60 control points"),
        "{made}"
    );
    assert!(made["deviation"].as_f64().unwrap() > 0.001);
}

#[test]
fn twelve_points_give_a_fit_point_spline_through_each() {
    let points: Vec<[f64; 2]> = (0..12)
        .map(|k| [k as f64 * 5.0, (k as f64 * 0.7).sin() * 8.0])
        .collect();
    let mut text = String::from("x,y\n");
    for p in &points {
        writeln!(text, "{},{}", p[0], p[1]).unwrap();
    }
    let path = file("points-2232-twelve", "cam.csv", &text);
    let mut d = with_sketch();
    let made = import(&mut d, &path, json!({"at": [5, 5]}));
    assert_eq!(made["fit"], "through", "{made}");
    assert_eq!(made["fit_points"], 12);
    assert_eq!(made["deviation"], 0.0);
    assert!(made["airfoil_name"].is_null());
    let s = sketch(&d);
    let spline = entity(&s, made["spline"].as_str().unwrap());
    assert_eq!(spline["type"], "fitted_spline", "{spline}");
    let through = ids(&spline["points"]);
    assert_eq!(through.len(), 12);
    for (id, p) in through.iter().zip(&points) {
        assert!(near(at(&s, id), [p[0] + 5.0, p[1] + 5.0], 1e-9), "{id}");
    }
}

#[test]
fn as_points_gives_only_points() {
    let path = file(
        "points-2232-points",
        "measured.txt",
        "0\t0\n10\t2\n20\t-1\n30\t4\n",
    );
    let mut d = with_sketch();
    let made = import(&mut d, &path, json!({"as": "points", "scale": 2}));
    assert_eq!(made["curves"], 0, "{made}");
    assert_eq!(made["points"], 4);
    assert!(made["spline"].is_null() && made["fit"].is_null());
    let s = sketch(&d);
    assert_eq!(entities(&s).len(), 4);
    assert!(entities(&s).iter().all(|e| e["type"] == "point"), "{s}");
    assert!(
        entities(&s)
            .iter()
            .any(|e| near(xy(&e["at"]), [60.0, 8.0], 1e-12))
    );
}

#[test]
fn undo_takes_the_insert_back_in_one_step_named_after_the_file() {
    let path = file("points-2232-undo", "naca2412.dat", &selig(61, 61, false, 6));
    let mut d = with_sketch();
    let before = d.undo_label().map(str::to_owned);
    import(&mut d, &path, json!({}));
    assert_eq!(d.undo_label(), Some("Insert naca2412.dat"));
    command(&mut d, json!({"cmd": "undo"}));
    assert!(entities(&sketch(&d)).is_empty());
    assert_eq!(d.undo_label().map(str::to_owned), before);
}

#[test]
fn a_rounded_200_point_naca_2412_is_fitted_within_the_tolerance_and_solves_quickly() {
    let path = file(
        "points-2232-naca",
        "naca2412.dat",
        &selig(100, 101, false, 5),
    );
    let mut d = with_sketch();
    let started = Instant::now();
    let made = import(&mut d, &path, json!({"chord": 100}));
    let took = started.elapsed();
    assert_eq!(made["tolerance"], 0.01, "{made}");
    let deviation = made["deviation"].as_f64().unwrap();
    assert!(deviation <= 0.01, "{deviation} mm");
    let control = made["control_points"].as_u64().unwrap();
    assert!(control <= 40, "{control} control points");
    assert_eq!(made["warnings"], json!([]), "{made}");
    // The insert, its solve and the sketch's regions in under a second (a
    // generous bound).
    assert!(took.as_secs_f64() < 1.0, "{took:?}");
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["regions"].as_array().unwrap().len(), 1, "{s}");
}

#[test]
fn the_points_text_reads_nothing_and_a_missing_source_is_refused() {
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.import_points", "sketch": "F1", "text": "0 0\n10 5\n20 0\n"}),
    );
    assert_eq!(made["fit"], "through", "{made}");
    assert_eq!(d.undo_label(), Some("Insert Points"));
    let refused = rejected(
        &mut d,
        json!({"cmd": "sketch.import_points", "sketch": "F1"}),
    );
    assert!(
        refused.contains("give the points' file as path"),
        "{refused}"
    );
    let one = rejected(
        &mut d,
        json!({"cmd": "sketch.import_points", "sketch": "F1", "text": "name\n1 2\n"}),
    );
    assert!(one.contains("the file has only one point"), "{one}");
}
