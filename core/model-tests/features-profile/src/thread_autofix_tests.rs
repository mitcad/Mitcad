// SPDX-License-Identifier: MIT
//! Auto Fix of a thread whose face does not fit it (mitcad#1632), with the
//! mock kernel: the value or dimension that sets the face's diameter (a
//! Hole's diameter, an extruded circle's diameter dimension, a revolved
//! line's distance from the axis, a Cylinder's diameter) is set to the
//! thread's crest diameter, its expression keeping its form; a named
//! parameter used elsewhere is changed or replaced only as asked; a face
//! no dimension sets is left with the reason, and its warning says it has
//! to be changed by hand. The Thread panel's `add_feature` with
//! `fit_faces` does the same with the thread, in one undo step.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn warnings(doc: &Document<MockKernel>, uid: u64) -> Vec<String> {
    doc.warnings(FeatureUid(uid)).to_vec()
}

/// A parameter's expression and value.
fn param(doc: &Document<MockKernel>, name: &str) -> (String, f64) {
    let params = &doc.state().parameters;
    let id = params
        .find(name)
        .unwrap_or_else(|| panic!("no parameter {name}"));
    let p = params.get(id).unwrap();
    (p.expression().to_owned(), p.value())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6
}

/// Adds a feature: its uid's number.
fn add(doc: &mut Document<MockKernel>, def: Value) -> u64 {
    let added = command(doc, json!({"cmd": "add_feature", "def": def}));
    added["uid"]
        .as_str()
        .and_then(|u| u.strip_prefix('F'))
        .and_then(|u| u.parse().ok())
        .unwrap_or_else(|| panic!("no uid in {added}"))
}

/// A cosmetic thread of `designation` (`class`) on `face` of `body`.
fn thread(
    doc: &mut Document<MockKernel>,
    body: &str,
    face: &str,
    designation: &str,
    class: &str,
) -> u64 {
    add(
        doc,
        json!({"type": "thread", "faces": [{"body": body, "face": face}],
               "thread": {"designation": designation, "class": class}}),
    )
}

fn auto_fix(doc: &mut Document<MockKernel>, uid: u64, shared: Option<&str>) -> Value {
    let mut cmd = json!({"cmd": "auto_fix", "features": [format!("F{uid}")]});
    if let Some(shared) = shared {
        cmd["shared"] = json!(shared);
    }
    command(doc, cmd)
}

/// A rod: a circle (Sketch1, F1) of diameter `diameter` (a value or an
/// expression of the diameter dimension), extruded `length` (Extrude1,
/// F2).
fn rod(doc: &mut Document<MockKernel>, diameter: Value, length: Value) {
    add(
        doc,
        json!({"type": "sketch", "plane": "xy",
               "entities": [
                   {"id": "p2", "type": "point", "at": [0, 0], "fixed": true},
                   {"id": "c1", "type": "circle", "center": "p2", "radius": 41}],
               "dimensions": [{"id": "k3", "type": "diameter", "curve": "c1", "value": diameter}]}),
    );
    add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": length}, "operation": "new_body"}),
    );
}

#[test]
fn a_holes_diameter_is_set_to_the_threads_minor_diameter() {
    // M8x1.25 in a Ø5 hole of a Ø20 rod: the hole is 1.647 mm too small.
    let mut doc = Document::new(MockKernel::default());
    rod(&mut doc, json!(20), json!(9));
    add(
        &mut doc,
        json!({"type": "hole", "placement": {"type": "plane", "plane": "xy", "points": [[0, 0, 0]]},
               "diameter": 5, "flip": true, "extent": {"type": "distance", "depth": 9}}),
    );
    let uid = thread(&mut doc, "F2.b0", "F3:hole0.wall", "M8x1.25", "6H");
    assert_eq!(warnings(&doc, uid).len(), 1, "{:?}", warnings(&doc, uid));
    assert_eq!(doc.auto_fixable(FeatureUid(uid)), 1);
    let fixed = auto_fix(&mut doc, uid, None);
    assert_eq!(fixed["fixed"], 1, "{fixed}");
    assert_eq!(fixed["label"], "Auto Fix Thread1", "{fixed}");
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["source"], "F3", "{fixed}");
    assert_eq!(fit["named"], false, "{fixed}");
    // The hole's own diameter parameter: M8x1.25's minor diameter
    // (Ø6.647 mm as the warning shows it).
    let (_, value) = param(&doc, fit["parameter"].as_str().unwrap());
    assert!((value - 6.647).abs() < 1e-3, "{value}");
    // (Written to a micrometre: within half a micrometre of it.)
    assert!(
        (value - fit["needed"].as_f64().unwrap()).abs() <= 5e-4,
        "{fixed}"
    );
    assert!(
        fixed["message"]
            .as_str()
            .unwrap()
            .contains("the hole fitted to the thread"),
        "{fixed}"
    );
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
    // One undo step takes it back.
    assert_eq!(doc.undo().as_deref(), Some("Auto Fix Thread1"));
    assert_eq!(warnings(&doc, uid).len(), 1);
}

#[test]
fn an_extruded_circles_diameter_dimension_is_set_to_the_threads_major_diameter() {
    // M80x2 on a Ø82 rod: an external thread, the face 2 mm too large.
    let mut doc = Document::new(MockKernel::default());
    rod(&mut doc, json!("82 mm"), json!(9));
    let uid = thread(&mut doc, "F2.b0", "F2:side(c1)", "M80x2", "6g");
    assert_eq!(warnings(&doc, uid).len(), 1, "{:?}", warnings(&doc, uid));
    let fixed = auto_fix(&mut doc, uid, None);
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["source"], "F1", "{fixed}");
    assert_eq!(fit["old"], "82 mm", "{fixed}");
    assert_eq!(fit["new"], "80 mm", "{fixed}");
    assert_eq!(fit["internal"], false, "{fixed}");
    assert!(
        fixed["message"]
            .as_str()
            .unwrap()
            .contains("the shaft fitted to the thread: Sketch1's dimension 82 mm is now 80 mm"),
        "{fixed}"
    );
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
}

/// A ring section (Sketch1, F1) revolved about the y axis (Revolve1, F2):
/// its inner line c4 at `radius` (the expression of the distance from the
/// origin point p9), its outer line c2 a 3 mm wall from it.
fn ring(doc: &mut Document<MockKernel>, radius: &str) {
    add(
        doc,
        json!({"type": "sketch", "plane": "xy",
               "entities": [
                   {"id": "p9", "type": "point", "at": [0, 0], "fixed": true},
                   {"id": "p5", "type": "point", "at": [36, 0]},
                   {"id": "p6", "type": "point", "at": [39, 0]},
                   {"id": "p7", "type": "point", "at": [39, 10]},
                   {"id": "p8", "type": "point", "at": [36, 10]},
                   {"id": "c1", "type": "line", "start": "p5", "end": "p6"},
                   {"id": "c2", "type": "line", "start": "p6", "end": "p7"},
                   {"id": "c3", "type": "line", "start": "p7", "end": "p8"},
                   {"id": "c4", "type": "line", "start": "p8", "end": "p5"}],
               "constraints": [
                   {"id": "k10", "type": "horizontal", "line": "c1"},
                   {"id": "k11", "type": "vertical", "line": "c2"},
                   {"id": "k12", "type": "horizontal", "line": "c3"},
                   {"id": "k13", "type": "vertical", "line": "c4"}],
               "dimensions": [
                   {"id": "k14", "type": "point_line_distance", "point": "p9", "line": "c4",
                    "value": radius},
                   {"id": "k15", "type": "line_distance", "a": "c4", "b": "c2", "value": "3 mm"},
                   {"id": "k16", "type": "vertical_distance", "a": "p9", "b": "p5", "value": "0 mm",
                    "signed": true},
                   {"id": "k17", "type": "length", "line": "c4", "value": "10 mm"}]}),
    );
    add(
        doc,
        json!({"type": "revolve",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "axis": "y", "extent": {"type": "full"}, "operation": "new_body"}),
    );
}

const INNER: &str = "F2:side(c4[c3,c1])";
const OUTER: &str = "F2:side(c2[c1,c3])";

#[test]
fn a_revolved_lines_distance_from_the_axis_keeps_its_form_and_the_wall_moves_with_it() {
    // The ticket's case: M76x2 6H in a Ø72 bore whose radius is `72/2`.
    let mut doc = Document::new(MockKernel::default());
    ring(&mut doc, "72/2");
    let uid = thread(&mut doc, "F2.b0", INNER, "M76x2", "6H");
    let warned = warnings(&doc, uid);
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(
        warned[0]
            .contains("the hole's diameter is 1.835 mm smaller than the thread's minor diameter"),
        "{warned:?}"
    );
    let fixed = auto_fix(&mut doc, uid, None);
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["old"], "72/2", "{fixed}");
    assert_eq!(fit["new"], "73.835/2", "{fixed}");
    assert_eq!(fit["internal"], true, "{fixed}");
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
    // The wall keeps its 3 mm: the outer side moved out with the bore.
    let cylinders = doc.kernel().face_cylinders.borrow();
    assert!(
        close(cylinders[INNER].radius, 73.835 / 2.0),
        "{:?}",
        cylinders[INNER]
    );
    assert!(
        close(cylinders[OUTER].radius, 73.835 / 2.0 + 3.0),
        "{:?}",
        cylinders[OUTER]
    );
}

#[test]
fn a_cylinders_diameter_is_set_to_the_threads_major_diameter() {
    let mut doc = Document::new(MockKernel::default());
    add(
        &mut doc,
        json!({"type": "cylinder", "plane": "xy", "diameter": 72, "height": 10,
               "operation": "new_body"}),
    );
    let uid = thread(&mut doc, "F1.b0", "F1:side0", "M76x2", "6g");
    assert_eq!(warnings(&doc, uid).len(), 1, "{:?}", warnings(&doc, uid));
    let fixed = auto_fix(&mut doc, uid, None);
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["source"], "F1", "{fixed}");
    let (_, value) = param(&doc, fit["parameter"].as_str().unwrap());
    assert!(close(value, 76.0), "{value}");
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
}

#[test]
fn a_named_parameter_used_elsewhere_is_changed_or_kept_only_as_asked() {
    // The rod's diameter is the parameter `bore`, which its length uses too.
    let setup = || {
        let mut doc = Document::new(MockKernel::default());
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": "bore", "expression": "82 mm"}),
        );
        rod(&mut doc, json!("bore"), json!("bore"));
        let uid = thread(&mut doc, "F2.b0", "F2:side(c1)", "M80x2", "6g");
        (doc, uid)
    };

    // Without a choice nothing changes: it says which parameter and where.
    let (mut doc, uid) = setup();
    assert_eq!(doc.auto_fixable(FeatureUid(uid)), 1);
    let left = auto_fix(&mut doc, uid, None);
    assert_eq!(left["fixed"], 0, "{left}");
    assert_eq!(left["label"], Value::Null, "{left}");
    assert_eq!(
        left["choices"],
        json!([{"feature": format!("F{uid}"), "parameter": "bore",
                                        "others": 1}])
    );
    assert!(
        left["left"][0]["advice"]
            .as_str()
            .unwrap()
            .contains("the parameter bore is used in 1 other place"),
        "{left}"
    );
    assert_eq!(param(&doc, "bore").0, "82 mm");

    // Change the parameter: the length follows it.
    let (mut doc, uid) = setup();
    let fixed = auto_fix(&mut doc, uid, Some("parameter"));
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["parameter"], "bore", "{fixed}");
    assert_eq!(fit["named"], true, "{fixed}");
    assert_eq!(fit["others"], 1, "{fixed}");
    assert_eq!(param(&doc, "bore"), ("80 mm".to_owned(), 80.0));
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
    assert!(
        fixed["message"]
            .as_str()
            .unwrap()
            .contains("the parameter bore is now 80 mm (used in 1 other place)"),
        "{fixed}"
    );

    // Its own value: `bore` stays, the dimension has one of its own.
    let (mut doc, uid) = setup();
    let fixed = auto_fix(&mut doc, uid, Some("own"));
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["named"], false, "{fixed}");
    assert_eq!(fit["new"], "80 mm", "{fixed}");
    assert_eq!(param(&doc, "bore"), ("82 mm".to_owned(), 82.0));
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
    // One undo step takes back the new parameter too.
    assert_eq!(doc.undo().as_deref(), Some("Auto Fix Thread1"));
    assert_eq!(warnings(&doc, uid).len(), 1);
}

#[test]
fn a_parameter_only_the_dimension_uses_is_changed_keeping_its_form() {
    // `gland/2` with `gland` used nowhere else: `gland` itself changes.
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "gland", "expression": "70 mm + 2 mm"}),
    );
    ring(&mut doc, "gland/2");
    let uid = thread(&mut doc, "F2.b0", INNER, "M76x2", "6H");
    let fixed = auto_fix(&mut doc, uid, None);
    let fit = &fixed["fitted"][0];
    assert_eq!(fit["parameter"], "gland", "{fixed}");
    assert_eq!(fit["others"], 0, "{fixed}");
    let (expression, value) = param(&doc, "gland");
    assert_eq!(expression, "71.835 mm + 2 mm");
    assert!(close(value, 73.835), "{value}");
    assert!(warnings(&doc, uid).is_empty(), "{:?}", warnings(&doc, uid));
}

#[test]
fn a_face_no_dimension_sets_is_left_to_be_changed_by_hand() {
    // A circle without a dimension (its radius stays where it was drawn).
    let mut doc = Document::new(MockKernel::default());
    add(
        &mut doc,
        json!({"type": "sketch", "plane": "xy",
               "entities": [
                   {"id": "p2", "type": "point", "at": [0, 0], "fixed": true},
                   {"id": "c1", "type": "circle", "center": "p2", "radius": 41}]}),
    );
    add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": 9}, "operation": "new_body"}),
    );
    let uid = thread(&mut doc, "F2.b0", "F2:side(c1)", "M80x2", "6g");
    let warned = warnings(&doc, uid);
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(
        warned[0].ends_with(
            "; the shaft has to be changed by hand (no dimension of Sketch1 sets the face's diameter)"
        ),
        "{warned:?}"
    );
    assert_eq!(doc.auto_fixable(FeatureUid(uid)), 0);
    let left = auto_fix(&mut doc, uid, None);
    assert_eq!(left["fixed"], 0, "{left}");
    assert_eq!(
        left["left"][0]["advice"],
        "change the shaft by hand (no dimension of Sketch1 sets the face's diameter)",
        "{left}"
    );
}

#[test]
fn a_thread_added_with_fit_faces_fits_its_hole_in_one_undo_step() {
    // The Thread panel's Fit the hole to the thread.
    let mut doc = Document::new(MockKernel::default());
    ring(&mut doc, "72/2");
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "fit_faces": true,
               "def": {"type": "thread", "faces": [{"body": "F2.b0", "face": INNER}],
                       "thread": {"designation": "M76x2", "class": "6H"}}}),
    );
    assert_eq!(added["fitted"][0]["new"], "73.835/2", "{added}");
    assert!(warnings(&doc, 3).is_empty(), "{:?}", warnings(&doc, 3));
    let radius = added["fitted"][0]["parameter"].as_str().unwrap().to_owned();
    assert_eq!(param(&doc, &radius).0, "73.835/2");
    // Undo takes back both.
    assert_eq!(doc.undo().as_deref(), Some("Add Thread1"));
    assert!(doc.state().entry(FeatureUid(3)).is_none());
    assert_eq!(param(&doc, &radius).0, "72/2");
}

#[test]
fn the_preview_names_the_faces_that_do_not_fit_and_shows_the_fit() {
    let mut doc = Document::new(MockKernel::default());
    ring(&mut doc, "72/2");
    let def = json!({"type": "thread", "faces": [{"body": "F2.b0", "face": INNER}],
                     "thread": {"designation": "M76x2", "class": "6H"}});
    let preview: Value = serde_json::from_str(
        &doc.preview(&json!({"cmd": "add_feature", "def": def}).to_string())
            .unwrap(),
    )
    .unwrap();
    let fits = &preview["thread_fits"][0];
    assert_eq!(fits["face"], INNER, "{preview}");
    assert!(close(fits["diameter"].as_f64().unwrap(), 72.0), "{preview}");
    // M76x2's basic minor diameter, D - 1.082532 P (73.835 as the warning
    // shows it).
    assert!(
        (fits["needed"].as_f64().unwrap() - 73.834_936_5).abs() < 1e-6,
        "{preview}"
    );
    assert_eq!(fits["fixable"], true, "{preview}");
    // With the fit: no face left, what changed named.
    let fitted: Value = serde_json::from_str(
        &doc.preview(&json!({"cmd": "add_feature", "def": def, "fit_faces": true}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(fitted["thread_fits"], Value::Null, "{fitted}");
    assert_eq!(fitted["fitted"][0]["new"], "73.835/2", "{fitted}");
    // Nothing was changed by the previews.
    let radius = fitted["fitted"][0]["parameter"].as_str().unwrap();
    assert_eq!(param(&doc, radius).0, "72/2");
}
