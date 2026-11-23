// SPDX-License-Identifier: MIT
//! The reporter's designs of GitHub mitcad/Mitcad#570 (mitcad#2277;
//! `MITCAD_MITCAD_REPORTED`/`github-570`, when they are there).
//!
//! - One constrained arc and its offset arc, whose ends no constraint
//!   holds: the solve counts them free, and a drag of an end moves it and
//!   it stays there, also after the design is recomputed (each solve put it
//!   back on its source's angle before). The design file keeps how far it
//!   was dragged.
//! - An open chain of three tangent arcs ending on the y axis at a point a
//!   horizontal line leaves: the tangent of the last arc with the line and
//!   the line's Horizontal together put that arc's centre on the axis; each
//!   takes one degree of freedom away, so neither is redundant.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model_api::DocumentApi;

/// The reporter's design in the report's zip `name`, unpacked into `dir`
/// and loaded with the mock kernel; None, and a note, when the private test
/// files are not here.
fn reported(dir: &Path, name: &str) -> Option<Document<MockKernel>> {
    let root = std::env::var("MITCAD_MITCAD_REPORTED").ok()?;
    let zip = PathBuf::from(root).join("github-570").join(name);
    if !zip.is_file() {
        eprintln!("skipped: {} is not here", zip.display());
        return None;
    }
    let archive = mitcad_zip::Archive::open(&zip).unwrap();
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name.ends_with(".mitcad"))
        .unwrap_or_else(|| panic!("no design in {}", zip.display()));
    let design = dir.join("design.mitcad");
    std::fs::write(&design, archive.read(entry).unwrap()).unwrap();
    let mut doc = Document::load_design(&design, MockKernel::default())
        .unwrap_or_else(|e| panic!("{}: {e}", design.display()));
    doc.recompute();
    Some(doc)
}

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

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// Whether two angles are the same direction.
fn same_angle(a: f64, b: f64) -> bool {
    let turn = std::f64::consts::TAU;
    let d = (a - b).rem_euclid(turn);
    d.min(turn - d) < 1e-6
}

/// The arc's centre, radius and the angles of its start and end.
fn arc(s: &Value, id: &str) -> ([f64; 2], f64, f64, f64) {
    let g = &entity(s, id)["geometry"];
    (
        xy(&g["center"]),
        g["radius"].as_f64().unwrap(),
        g["start_angle"].as_f64().unwrap(),
        g["end_angle"].as_f64().unwrap(),
    )
}

#[test]
fn the_reported_offset_arcs_ends_stay_where_they_are_dragged() {
    let dir = unique_temp_dir("2277-reported-arc");
    let Some(mut d) = reported(&dir, "Curve Offset 1.zip") else {
        remove_temp(&dir);
        return;
    };
    // Sketch2 (F2): arc c7 and its offset c9 (k6) from p10 to p11, held by
    // its helper c14; the offset's ends at the source's angles.
    let s = sketch(&d, "F2");
    assert!(s["error"].is_null(), "{s}");
    let dof = s["dof"].as_u64().unwrap();
    assert!(dof >= 2, "the offset's ends are free: {s}");
    let (_, _, source_start, source_end) = arc(&s, "c7");
    let (c, r, start, end) = arc(&s, "c9");
    assert!(same_angle(start, source_start), "{s}");
    assert!(same_angle(end, source_end), "{s}");
    // Its start p10 dragged 10 degrees round its circle.
    let t = start + 10f64.to_radians();
    let target = [c[0] + r * t.cos(), c[1] + r * t.sin()];
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F2", "entity": "p10", "to": target}),
    );
    let check = |d: &Document<MockKernel>| {
        let s = sketch(d, "F2");
        assert!(s["error"].is_null(), "{s}");
        let (c2, r2, start2, end2) = arc(&s, "c9");
        assert!((c2[0] - c[0]).hypot(c2[1] - c[1]) < 1e-6, "{s}");
        assert!((r2 - r).abs() < 1e-6, "{s}");
        assert!(same_angle(start2, t), "the dragged end stays: {s}");
        assert!(same_angle(end2, end), "the other end stays: {s}");
        assert_eq!(s["dof"].as_u64().unwrap(), dof, "{s}");
    };
    check(&d);
    command(&mut d, json!({"cmd": "recompute"}));
    check(&d);
    // The design file keeps how far the end was dragged.
    let design: Value = serde_json::from_str(&d.to_json()).unwrap();
    let f2 = design["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == "F2")
        .expect("Sketch2");
    let shifts = &f2["offsets"][0]["shifts"];
    assert_eq!(shifts.as_array().map(Vec::len), Some(1), "{f2}");
    assert_eq!(shifts[0]["point"], "p10", "{f2}");
    let by = shifts[0]["by"].as_f64().unwrap();
    assert!((by - 10f64.to_radians()).abs() < 1e-6, "{f2}");
    remove_temp(&dir);
}

#[test]
fn the_reported_tangent_and_horizontal_on_the_axis_are_not_redundant() {
    let dir = unique_temp_dir("2277-reported-chain");
    let Some(d) = reported(&dir, "Horse Shoe GH Example.zip") else {
        remove_temp(&dir);
        return;
    };
    // Sketch1 (F6): arc c17 ends at p9 on the y axis, where line c19
    // leaves it; k6 is their tangent, k5 the line's Horizontal.
    let s = sketch(&d, "F6");
    assert!(s["error"].is_null(), "{s}");
    let redundant = strings(&s["redundant"]);
    assert!(
        !redundant.iter().any(|k| k == "k5" || k == "k6"),
        "{redundant:?}: {s}"
    );
    let dof = s["dof"].as_u64().unwrap();
    // Without either, the arc's centre leaves the axis: one more degree of
    // freedom.
    for k in ["k5", "k6"] {
        let Some(mut d) = reported(&dir, "Horse Shoe GH Example.zip") else {
            unreachable!("loaded above");
        };
        command(
            &mut d,
            json!({"cmd": "sketch.remove", "sketch": "F6", "items": [k]}),
        );
        let s = sketch(&d, "F6");
        assert!(s["error"].is_null(), "{s}");
        assert_eq!(s["dof"].as_u64().unwrap(), dof + 1, "without {k}: {s}");
    }
    remove_temp(&dir);
}
