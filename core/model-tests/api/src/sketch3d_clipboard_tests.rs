// SPDX-License-Identifier: MIT
//! Copy, Cut and Paste of 3D sketch geometry through the JSON api
//! (mitcad#1661), with the mock kernel: a route copied from one 3D sketch
//! into another keeps its z, its relations, its dimensions and its
//! attachment to the origin, as one undo step; a copy pasted elsewhere
//! moves by the place given and keeps its elevation, with what it left out
//! counted; a cut's copy pastes after its geometry went; a reference to a
//! source the target cannot use is left out and said in words, one it can
//! use is kept; a planar sketch's geometry goes onto a frame plane of a 3D
//! sketch at the place given; a 3D copy is refused by a 2D sketch.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const TOL: f64 = 1e-6;

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
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

/// The `sketch3d` query of `uid`.
fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch3d", "uid": uid}))
}

/// The `sketch_fragment` answer of `items` of `uid`.
fn copy(doc: &Document<MockKernel>, uid: &str, items: &[&str]) -> Value {
    query(
        doc,
        json!({"query": "sketch_fragment", "sketch": uid, "items": items}),
    )
}

fn xyz(value: &Value) -> [f64; 3] {
    let a = value
        .as_array()
        .unwrap_or_else(|| panic!("{value} is not a point"));
    [0, 1, 2].map(|k| a[k].as_f64().unwrap())
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

/// Where the sketch's points are (local coordinates).
fn places(sketch: &Value) -> Vec<[f64; 3]> {
    sketch["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| xyz(&p["at"]))
        .collect()
}

/// The types of the sketch's relations or dimensions (`list`).
fn types(sketch: &Value, list: &str) -> Vec<String> {
    let mut types: Vec<String> = sketch[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap().to_owned())
        .collect();
    types.sort();
    types
}

/// The id of the sketch's point at `at`.
fn point_at(sketch: &Value, at: [f64; 3]) -> Value {
    let points = sketch["points"].as_array().unwrap();
    let found = points.iter().find(|p| near(xyz(&p["at"]), at));
    let point = found.unwrap_or_else(|| panic!("{at:?} in {sketch}"));
    point["id"].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// 3DSketch1 (F1): a line from the origin (attached to it) 40 along +X,
/// then 20 along +Z, each held by its axis and length. Returns the two
/// lines.
fn route(doc: &mut Document<MockKernel>) -> (String, String) {
    command(doc, json!({"cmd": "sketch3d.create"}));
    let ab = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [38, 1, 0], "axis": "x", "length": 40, "start_source": "origin"}),
    );
    let bc = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": ab["end"],
               "end": [40, 0, 19], "axis": "z", "sense": "positive", "length": 20}),
    );
    (
        ab["made"][0].as_str().unwrap().to_owned(),
        bc["made"][0].as_str().unwrap().to_owned(),
    )
}

/// A route copied into another 3D sketch where it was: its three points
/// with their elevation, its alignments, lengths and the attachment to the
/// origin (a source the target takes), fully constrained there too; one
/// undo step that undo takes back whole.
#[test]
fn a_copy_between_3d_sketches_keeps_z_relations_and_references() {
    let mut doc = Document::new(MockKernel::default());
    let (ab, bc) = route(&mut doc);
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let copied = copy(&doc, "F1", &[&ab, &bc]);
    assert_eq!(copied["kind"], "3d", "{copied}");
    assert_eq!(
        copied["entities"], 5,
        "three points and two lines: {copied}"
    );
    // The attachment, two alignments and two lengths.
    assert_eq!(copied["kept"], 5, "{copied}");
    assert_eq!(copied["left_out"], 0, "{copied}");
    assert_eq!(copied["references"], 1, "the origin attachment: {copied}");
    assert!(near(xyz(&copied["base"]), [0.0, 0.0, 0.0]), "{copied}");

    let pasted = command(
        &mut doc,
        json!({"cmd": "sketch3d.paste", "sketch": "F2", "fragment": copied["fragment"]}),
    );
    assert_eq!(pasted["made"].as_array().unwrap().len(), 5, "{pasted}");
    assert_eq!(pasted["kept"], 5, "{pasted}");
    assert_eq!(pasted["left_out"], 0, "{pasted}");
    assert_eq!(pasted["dropped"], 0, "{pasted}");
    assert!(pasted.get("warnings").is_none(), "{pasted}");
    assert_eq!(undo_label(&doc), "Paste in 3DSketch2");

    let target = sketch(&doc, "F2");
    let at = places(&target);
    for wanted in [[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [40.0, 0.0, 20.0]] {
        assert!(at.iter().any(|p| near(*p, wanted)), "{wanted:?}: {at:?}");
    }
    assert_eq!(
        types(&target, "constraints"),
        ["align", "align", "attach"],
        "{target}"
    );
    assert_eq!(types(&target, "dimensions"), ["length", "length"]);
    let attach = target["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "attach")
        .unwrap();
    assert_eq!(attach["source"], "origin", "{attach}");
    assert_eq!(target["status"]["dof"], 0, "{target}");

    // One undo step: the target is empty again, the source as it was.
    command(&mut doc, json!({"cmd": "undo"}));
    let undone = sketch(&doc, "F2");
    assert!(
        undone["points"].as_array().is_none_or(Vec::is_empty),
        "{undone}"
    );
    assert_eq!(sketch(&doc, "F1")["curves"].as_array().unwrap().len(), 2);
}

/// One line of the route pasted at another place: it moves by the place
/// given and keeps its 20 mm rise; a driven offset from the origin's point,
/// which the copy lacks, is left out and counted. A cut is the copy and the
/// removal of the same items: its copy still pastes.
#[test]
fn a_copy_moves_to_the_place_given_and_a_cut_removes() {
    let mut doc = Document::new(MockKernel::default());
    let (_, bc) = route(&mut doc);
    let rise = sketch(&doc, "F1");
    let start = point_at(&rise, [0.0, 0.0, 0.0]);
    let top = point_at(&rise, [40.0, 0.0, 20.0]);
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1", "driven": true,
               "dimension": {"type": "offset", "a": start, "b": top, "axis": "z"}}),
    );
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let copied = copy(&doc, "F1", &[&bc]);
    // The line's alignment and length; the offset from the start, which is
    // not copied, is left out.
    assert_eq!(copied["kept"], 2, "{copied}");
    assert_eq!(copied["left_out"], 1, "{copied}");
    assert!(near(xyz(&copied["base"]), [40.0, 0.0, 0.0]), "{copied}");

    // Cut: the copy, then the same items removed.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [bc]}),
    );
    assert_eq!(sketch(&doc, "F1")["curves"].as_array().unwrap().len(), 1);

    let pasted = command(
        &mut doc,
        json!({"cmd": "sketch3d.paste", "sketch": "F2", "fragment": copied["fragment"],
               "at": [100, 10, 5]}),
    );
    // What the copy left out was said at the copy.
    assert_eq!(pasted["left_out"], 1, "{pasted}");
    assert!(pasted.get("warnings").is_none(), "{pasted}");
    let at = places(&sketch(&doc, "F2"));
    assert_eq!(at.len(), 2, "{at:?}");
    for wanted in [[100.0, 10.0, 5.0], [100.0, 10.0, 25.0]] {
        assert!(at.iter().any(|p| near(*p, wanted)), "{wanted:?}: {at:?}");
    }
}

/// A point attached to a planar sketch's point: pasted into a 3D sketch
/// that comes before that sketch in the timeline, the attachment is left
/// out and the warning says how many in words (no ids); pasted into a 3D
/// sketch after it, the attachment is the same reference.
#[test]
fn a_reference_outside_the_copy_is_kept_or_left_out_in_words() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"})); // F1, before the source
    command(&mut doc, json!({"cmd": "sketch.create"})); // F2
    let added = command(
        &mut doc,
        json!({"cmd": "sketch.add_point", "sketch": "F2", "at": [5, 5]}),
    );
    let point = added["entities"][0].clone();
    command(&mut doc, json!({"cmd": "sketch3d.create"})); // F3
    let source = json!({"sketch": "F2", "point": point});
    let post = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F3", "start": [5, 5, 0],
               "end": [5, 6, 9], "axis": "z", "length": 10, "start_source": source}),
    );
    let line = post["made"][0].as_str().unwrap().to_owned();
    let copied = copy(&doc, "F3", &[&line]);
    assert_eq!(copied["references"], 1, "{copied}");

    // Into F1: the planar sketch comes after it.
    let early = command(
        &mut doc,
        json!({"cmd": "sketch3d.paste", "sketch": "F1", "fragment": copied["fragment"]}),
    );
    assert_eq!(early["dropped"], 1, "{early}");
    assert_eq!(early["kept"], 2, "{early}");
    let warnings = early["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{early}");
    let warning = warnings[0].as_str().unwrap();
    assert!(
        warning.contains("1 link(s) to geometry outside the copy left out"),
        "{warning}"
    );
    // No ids or names of features (mitcad#1327).
    assert!(
        !warning.contains("F2") && !warning.contains("k1"),
        "{warning}"
    );
    let f1 = sketch(&doc, "F1");
    assert_eq!(types(&f1, "constraints"), ["align"], "{f1}");

    // Into F3 itself: the same reference.
    let kept = command(
        &mut doc,
        json!({"cmd": "sketch3d.paste", "sketch": "F3", "fragment": copied["fragment"]}),
    );
    assert_eq!(kept["dropped"], 0, "{kept}");
    assert_eq!(kept["kept"], 3, "{kept}");
    let f3 = sketch(&doc, "F3");
    let attachments: Vec<&Value> = f3["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "attach")
        .collect();
    assert_eq!(attachments.len(), 2, "{f3}");
    assert!(attachments.iter().all(|c| c["source"] == source), "{f3}");
}

/// A planar sketch's line with its horizontal and length goes onto the YZ
/// plane of a 3D sketch, its start at the place given: its points on that
/// plane, the constraint and dimension in-plane ones of it; the distance
/// from the planar origin, which a 3D sketch has no point for, is left out
/// and said. A 3D sketch's copy is refused by a 2D sketch, in words.
#[test]
fn planar_geometry_goes_onto_a_plane_of_a_3d_sketch() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let added = command(
        &mut doc,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let origin = added["entities"][0].clone();
    let made = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, 10], "end": [40, 10]}),
    );
    let line = made["made"][0].as_str().unwrap().to_owned();
    let planar = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    let entities = planar["entities"].as_array().unwrap();
    let entity = entities.iter().find(|e| e["id"] == line.as_str());
    let start = entity.unwrap()["start"].clone();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": line}}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 30,
               "dimension": {"type": "length", "line": line}}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 10,
               "dimension": {"type": "vertical_distance", "a": origin, "b": start}}),
    );
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let copied = copy(&doc, "F1", &[&line]);
    assert_eq!(copied["kept"], 3, "{copied}");

    let pasted = command(
        &mut doc,
        json!({"cmd": "sketch3d.paste", "sketch": "F2", "fragment": copied["fragment"],
               "plane": "yz", "at": [0, 5, 7]}),
    );
    // The horizontal, the length and the plane the points are kept in.
    assert_eq!(pasted["kept"], 3, "{pasted}");
    assert_eq!(pasted["left_out"], 1, "{pasted}");
    let warnings = pasted["warnings"].as_array().unwrap();
    assert!(
        warnings[0]
            .as_str()
            .unwrap()
            .contains("1 relation(s) and dimension(s) left out"),
        "{pasted}"
    );
    let target = sketch(&doc, "F2");
    let at = places(&target);
    assert_eq!(at.len(), 2, "{at:?}");
    for wanted in [[0.0, 5.0, 7.0], [0.0, 35.0, 7.0]] {
        assert!(at.iter().any(|p| near(*p, wanted)), "{wanted:?}: {at:?}");
    }
    let in_plane = target["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "in_plane")
        .unwrap_or_else(|| panic!("an in-plane horizontal in {target}"));
    assert_eq!(in_plane["normal"], "x", "{in_plane}");
    assert_eq!(in_plane["relation"]["type"], "horizontal", "{in_plane}");
    let kinds = types(&target, "constraints");
    assert!(kinds.contains(&"plane_bound".to_owned()), "{target}");
    assert_eq!(types(&target, "dimensions"), ["in_plane"], "{target}");

    // The other way round is refused.
    let pasted_line = target["curves"][0]["id"].as_str().unwrap();
    let spatial = copy(&doc, "F2", &[pasted_line]);
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.paste", "sketch": "F1", "fragment": spatial["fragment"]}),
    );
    assert!(
        error.contains("cannot be pasted into a 2D sketch"),
        "{error}"
    );
}
