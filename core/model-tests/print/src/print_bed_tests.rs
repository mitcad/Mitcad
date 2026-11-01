// SPDX-License-Identifier: MIT
//! A body's print bed face (mitcad#731): set, replaced and cleared as undo
//! steps, refused when it cannot lie on the bed (a surface body's too),
//! kept by its name through edits (one that renames it) and in the design
//! file; and the 3D print export's `on_bed`, which turns the bodies onto
//! the one bed face among them and centres them on the plate (a box on its
//! top, on a tilted face, in a turned component, in a component placed
//! twice), keeps the bodies as assembled with several bed faces or one
//! body placed twice turned differently, leaves a surface body's face out
//! (the `bodies` query says why; it can be cleared) and reports a face
//! that is gone or not planar. A piece kept for several bed faces lists
//! them, one per placement, and a face chosen puts the piece on it
//! (mitcad#857).
//! The mock kernel's meshes are the bodies' boxes.

use std::f64::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::exchange::{BaseInput, ImportBody};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const SIDE: &str = "F2:side(c1[c4,c2])";
const TOL: f64 = 1e-6;
const GONE: &str = "the print bed face is gone";
const NOT_PLANAR: &str = "the print bed face is not planar";
const NOT_PRINTED: &str = "a surface body is not printed";
const IDENTITY: mitcad_3mf::Placement = mitcad_3mf::Placement::IDENTITY;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn document(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))
}

/// A 20 x 20 x 20 block (F2.b0, Body1) on a rectangle on XY.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

fn top() -> String {
    format!("F2:end({R})")
}

fn bottom() -> String {
    format!("F2:start({R})")
}

fn set_bed(doc: &mut Document<MockKernel>, body: &str, face: Value) -> Value {
    command(
        doc,
        json!({"cmd": "set_print_bed_face", "uid": body, "face": face}),
    )
}

/// The first body's `print_bed_face` in the `bodies` query, or null.
fn bed_face(doc: &Document<MockKernel>) -> Value {
    let bodies = query(doc, json!({"query": "bodies"}));
    bodies[0]
        .get("print_bed_face")
        .cloned()
        .unwrap_or(Value::Null)
}

/// The surface the mock kernel gives a face by its name.
fn give_surface(doc: &Document<MockKernel>, face: &str, geometry: SurfaceGeometry) {
    let mut surfaces = doc.kernel().surfaces.borrow_mut();
    surfaces.insert(face.to_owned(), geometry);
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

/// A folder of the test's own for the files it writes.
fn folder(name: &str) -> PathBuf {
    mitcad_model::testing::unique_temp_dir(&format!("print-bed-{name}"))
}

/// Exports the bodies to a 3MF file on the print bed: the result and the
/// file read back.
fn export_3mf(
    doc: &mut Document<MockKernel>,
    dir: &Path,
    bodies: Value,
) -> (Value, mitcad_3mf::Model) {
    export_3mf_with(doc, dir, bodies, json!({}))
}

/// As [`export_3mf`], with more of the export's fields.
fn export_3mf_with(
    doc: &mut Document<MockKernel>,
    dir: &Path,
    bodies: Value,
    more: Value,
) -> (Value, mitcad_3mf::Model) {
    let path = dir.join("print.3mf").to_str().unwrap().to_owned();
    let mut export = json!({"cmd": "export", "path": path, "bodies": bodies, "on_bed": true});
    for (key, value) in more.as_object().unwrap() {
        export[key] = value.clone();
    }
    let result = command(doc, export);
    let model = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    (result, model)
}

/// The export's fields that write the bodies where the design has them,
/// as before mitcad#1109: neither centred nor dropped, and no automatic
/// orientation.
fn as_designed() -> Value {
    json!({"center": false, "drop": false, "auto_orient": false})
}

/// The box of a 3MF part's mesh at all its placements.
fn placed_box(part: &mitcad_3mf::Part) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for placement in &part.placements {
        for vertex in &part.mesh.vertices {
            let p = placement.apply(*vertex);
            min = std::array::from_fn(|i| min[i].min(p[i]));
            max = std::array::from_fn(|i| max[i].max(p[i]));
        }
    }
    (min, max)
}

/// The box of several 3MF parts' meshes at all their placements.
fn parts_box(parts: &[mitcad_3mf::Part]) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for part in parts {
        let (lo, hi) = placed_box(part);
        min = std::array::from_fn(|i| min[i].min(lo[i]));
        max = std::array::from_fn(|i| max[i].max(hi[i]));
    }
    (min, max)
}

/// The turn of a result's `bed.transform` (its rows' first three numbers)
/// applied to a vector.
fn turned(result: &Value, v: [f64; 3]) -> [f64; 3] {
    let rows = &result["bed"]["transform"];
    std::array::from_fn(|r| (0..3).map(|c| number(&rows[r][c]) * v[c]).sum())
}

/// The whole of a result's `bed.transform` applied to a point.
fn moved(result: &Value, p: [f64; 3]) -> [f64; 3] {
    let rows = &result["bed"]["transform"];
    let turn = turned(result, p);
    std::array::from_fn(|r| turn[r] + number(&rows[r][3]))
}

/// The box's lowest point is on the bed and its middle over `plate`.
fn assert_on_plate(found: ([f64; 3], [f64; 3]), plate: [f64; 2]) {
    let (min, max) = found;
    assert!(near(min[2], 0.0), "lowest z {}", min[2]);
    assert!(near((min[0] + max[0]) / 2.0, plate[0]), "{min:?} {max:?}");
    assert!(near((min[1] + max[1]) / 2.0, plate[1]), "{min:?} {max:?}");
}

#[test]
fn a_planar_face_is_set_replaced_and_cleared() {
    let mut doc = block();
    let result = set_bed(&mut doc, "F2.b0", json!(top()));
    assert_eq!(result["recomputed"], 0, "no geometry changes");
    assert_eq!(document(&doc)["undo"], "Set Print Bed Face of Body1");
    assert_eq!(bed_face(&doc), json!({"face": top(), "faces": [top()]}));
    // The same again is no undo step.
    let depth = document(&doc)["undo_depth"].clone();
    set_bed(&mut doc, "F2.b0", json!(top()));
    assert_eq!(document(&doc)["undo_depth"], depth);
    // One face per body: another replaces it.
    set_bed(&mut doc, "F2.b0", json!(bottom()));
    assert_eq!(bed_face(&doc)["face"], bottom());
    set_bed(&mut doc, "F2.b0", Value::Null);
    assert_eq!(document(&doc)["undo"], "Clear Print Bed Face of Body1");
    assert_eq!(bed_face(&doc), Value::Null);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(bed_face(&doc)["face"], bottom());
}

#[test]
fn a_face_that_cannot_lie_on_the_bed_is_refused() {
    let mut doc = block();
    doc.kernel().surfaces.borrow_mut().insert(
        SIDE.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 10.0, 10.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    for (body, face, says) in [
        ("F2.b0", SIDE, "planar"),
        ("F2.b0", "F2:end(r{c9})", "has no face"),
        ("F2.b0", "no face at all", "no face name"),
        ("F9.b0", "F9:end(r{c1})", "F9.b0"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "set_print_bed_face", "uid": body, "face": face}),
        );
        assert!(error.contains(says), "{face}: {error}");
    }
    assert_eq!(bed_face(&doc), Value::Null);
}

#[test]
fn the_bed_face_follows_edits_and_is_saved() {
    let mut doc = block();
    let plain = doc.to_json();
    set_bed(&mut doc, "F2.b0", json!(top()));
    // A taller block keeps its top as the bed face.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}),
    );
    assert_eq!(bed_face(&doc), json!({"face": top(), "faces": [top()]}));

    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["bodies"][0]["print_bed_face"], top());
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(bed_face(&loaded), bed_face(&doc));

    // A file of an older version has none, and is saved without one.
    assert!(!plain.contains("print_bed_face"));
    let mut old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert_eq!(old.to_json(), plain);
    command(&mut old, json!({"cmd": "recompute"}));
    assert_eq!(bed_face(&old), Value::Null);

    // A name that is no face name is refused.
    let broken = saved.replace(&top(), "no face at all");
    assert_ne!(broken, saved);
    let Err(error) = Document::from_json(&broken, MockKernel::default()) else {
        panic!("a broken face name was read");
    };
    assert!(error.to_string().contains("print_bed_face"), "{error}");
}

#[test]
fn a_box_on_its_top_is_turned_over_and_centred_on_the_plate() {
    let mut doc = block();
    let dir = folder("top");
    set_bed(&mut doc, "F2.b0", json!(top()));
    let bodies = query(&doc, json!({"query": "bodies"}));
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    let bed = &result["bed"];
    assert_eq!(bed["body"], "F2.b0");
    assert_eq!(bed["name"], "Body1");
    assert_eq!(bed["face"], top());
    assert_eq!(bed["plate"], json!([128.0, 128.0]));
    assert_eq!(bed["kept"], json!([]));
    assert_eq!(bed["problems"], json!([]));
    assert_eq!(result["warnings"], json!([]));
    // The top's outward normal straight down, the top on the bed.
    let down = turned(&result, [0.0, 0.0, 1.0]);
    assert!(near(down[2], -1.0), "{down:?}");
    let middle = moved(&result, [10.0, 10.0, 20.0]);
    assert!(near(middle[2], 0.0), "{middle:?}");
    assert_on_plate(placed_box(&model.parts[0]), [128.0, 128.0]);
    // The placement is the transform; the mesh is the body's own.
    let placement = model.parts[0].placements[0];
    let rows = &bed["transform"];
    for (r, shift) in placement.translation.iter().enumerate() {
        assert!(near(*shift, number(&rows[r][3])), "{placement:?}");
    }
    assert!(near(model.parts[0].mesh.volume(), 8000.0));

    // The design does not move.
    assert_eq!(query(&doc, json!({"query": "bodies"})), bodies);

    // The plate of the design's printer: the middle of its bed shape.
    command(
        &mut doc,
        json!({"cmd": "create_print_profile", "kind": "printer", "name": "Bench printer", "choose": true,
               "values": {"bed_shape": [[0, 0], [250, 0], [250, 210], [0, 210]]}}),
    );
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    assert_eq!(result["bed"]["plate"], json!([125.0, 105.0]));
    assert_on_plate(placed_box(&model.parts[0]), [125.0, 105.0]);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_box_on_a_tilted_face_lies_on_it() {
    let mut doc = block();
    let dir = folder("tilted");
    // The side as a plane tilted 36.87° from the vertical, facing out and
    // down.
    let normal = [0.6, 0.0, -0.8];
    doc.kernel().surfaces.borrow_mut().insert(
        SIDE.to_owned(),
        SurfaceGeometry::Plane {
            origin: [20.0, 10.0, 0.0],
            normal,
        },
    );
    set_bed(&mut doc, "F2.b0", json!(SIDE));
    assert_eq!(bed_face(&doc), json!({"face": SIDE, "faces": [SIDE]}));
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    assert_eq!(result["bed"]["face"], SIDE);
    let down = turned(&result, normal);
    assert!(near(down[0], 0.0) && near(down[1], 0.0), "{down:?}");
    assert!(near(down[2], -1.0), "{down:?}");
    assert_on_plate(placed_box(&model.parts[0]), [128.0, 128.0]);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_body_in_a_turned_component_lies_on_its_face_where_it_is_placed() {
    let mut doc = block();
    let dir = folder("component");
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.components_from_bodies(&[body]).unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "O1", "transform":
               {"translation": [100, 0, 0], "rotation": {"axis": [1, 0, 0], "angle": FRAC_PI_2}}}),
    );
    set_bed(&mut doc, "F2.b0", json!(top()));
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    // The top faces -Y in the design: that is turned down.
    let down = turned(&result, [0.0, -1.0, 0.0]);
    assert!(near(down[2], -1.0), "{down:?}");
    assert_on_plate(placed_box(&model.parts[0]), [128.0, 128.0]);
    mitcad_model::testing::remove_temp(&dir);
}

/// The block and a second body under it, 10 high (F3.b0, Body2).
fn two_bodies() -> Document<MockKernel> {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": -10}, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn several_bed_faces_keep_the_bodies_as_assembled() {
    let mut doc = two_bodies();
    let dir = folder("several");
    set_bed(&mut doc, "F2.b0", json!(top()));
    set_bed(&mut doc, "F3.b0", json!(format!("F3:start({R})")));
    let (result, model) = export_3mf_with(&mut doc, &dir, json!(["F2.b0", "F3.b0"]), as_designed());
    let bed = &result["bed"];
    assert_eq!(bed["kept"], json!(["F2.b0", "F3.b0"]));
    assert_eq!(bed["body"], Value::Null);
    assert_eq!(bed["transform"], Value::Null);
    for part in &model.parts {
        assert_eq!(part.placements, [IDENTITY]);
    }
    // Centred and dropped by default all the same (mitcad#1109): moved
    // alike, not turned.
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0", "F3.b0"]));
    assert_eq!(result["bed"]["kept"], json!(["F2.b0", "F3.b0"]));
    assert_eq!(result["bed"]["body"], Value::Null);
    assert_eq!(model.parts[0].placements, model.parts[1].placements);
    assert_eq!(model.parts[0].placements[0].linear, IDENTITY.linear);
    assert_on_plate(parts_box(&model.parts), [128.0, 128.0]);

    // One bed face among them: both turn onto it as one piece.
    set_bed(&mut doc, "F3.b0", Value::Null);
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0", "F3.b0"]));
    assert_eq!(result["bed"]["body"], "F2.b0");
    assert_eq!(result["bed"]["kept"], json!([]));
    assert_eq!(model.parts[0].placements, model.parts[1].placements);
    assert_ne!(model.parts[0].placements[0], IDENTITY);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_bed_face_that_is_gone_or_not_planar_is_reported() {
    let mut doc = block();
    let dir = folder("gone");
    set_bed(&mut doc, "F2.b0", json!(top()));
    // No longer planar: reported, the body as the design has it.
    doc.kernel().surfaces.borrow_mut().insert(
        top(),
        SurfaceGeometry::Cylinder {
            origin: [10.0, 10.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 10.0,
        },
    );
    assert_eq!(bed_face(&doc)["error"], NOT_PLANAR);
    let (result, model) = export_3mf_with(&mut doc, &dir, json!(["F2.b0"]), as_designed());
    let bed = &result["bed"];
    assert_eq!(bed["transform"], Value::Null);
    assert_eq!(
        bed["problems"],
        json!([{"body": "F2.b0", "name": "Body1", "reason": NOT_PLANAR}])
    );
    assert_eq!(
        result["warnings"],
        json!(["Body1: the print bed face is not planar; printed as in the design"])
    );
    assert_eq!(model.parts[0].placements, [IDENTITY]);
    // By default the automatic orientation lays it down instead
    // (mitcad#1109): the problem still reported, the body not printed as
    // in the design.
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    assert_eq!(result["bed"]["auto"], true, "{}", result["bed"]);
    assert_eq!(
        result["warnings"],
        json!(["Body1: the print bed face is not planar"])
    );
    assert_on_plate(placed_box(&model.parts[0]), [128.0, 128.0]);

    // Gone: a name the body has no face of, as an edit leaves it.
    let saved = doc.to_json().replace(&top(), "F2:end(r{c9})");
    let mut gone = Document::from_json(&saved, MockKernel::default()).unwrap();
    command(&mut gone, json!({"cmd": "recompute"}));
    assert_eq!(
        bed_face(&gone),
        json!({"face": "F2:end(r{c9})", "faces": [], "error": GONE})
    );
    let (result, _) = export_3mf_with(&mut gone, &dir, json!(["F2.b0"]), as_designed());
    assert_eq!(
        result["warnings"],
        json!(["Body1: the print bed face is gone; printed as in the design"])
    );
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn an_stl_file_lies_on_the_bed_too() {
    let mut doc = block();
    let dir = folder("stl");
    set_bed(&mut doc, "F2.b0", json!(top()));
    let path = dir.join("print.stl").to_str().unwrap().to_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path, "bodies": ["F2.b0"], "ascii": true, "on_bed": true}),
    );
    assert_eq!(result["bed"]["face"], top());
    assert_eq!(result["warnings"], json!([]));
    let text = std::fs::read_to_string(&path).unwrap();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for line in text.lines() {
        let Some(numbers) = line.trim().strip_prefix("vertex ") else {
            continue;
        };
        let p: Vec<f64> = numbers
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        min = std::array::from_fn(|i| min[i].min(p[i]));
        max = std::array::from_fn(|i| max[i].max(p[i]));
    }
    assert_on_plate((min, max), [128.0, 128.0]);
    assert!(near(max[2], 20.0), "{max:?}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn only_3mf_and_stl_files_go_on_the_bed() {
    let mut doc = block();
    let dir = folder("formats");
    let step = dir.join("print.step").to_str().unwrap().to_owned();
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": step, "on_bed": true}),
    );
    assert!(error.contains("only 3MF, STL and OBJ"), "{error}");
    let three = dir.join("print.3mf").to_str().unwrap().to_owned();
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": three, "on_bed": true, "coordinates": "component"}),
    );
    assert!(error.contains("design's coordinates"), "{error}");
    // Without a bed face, neither oriented nor settled, the bodies stay
    // where the design has them.
    let (result, model) = export_3mf_with(&mut doc, &dir, json!(["F2.b0"]), as_designed());
    assert_eq!(result["bed"]["body"], Value::Null);
    assert_eq!(result["bed"]["kept"], json!([]));
    assert_eq!(model.parts[0].placements, [IDENTITY]);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_bed_face_follows_a_sketch_edit_that_renames_it() {
    let mut doc = block();
    let dir = folder("renamed");
    // The side y = 0 as a plane facing -Y.
    let plane = SurfaceGeometry::Plane {
        origin: [10.0, 0.0, 10.0],
        normal: [0.0, -1.0, 0.0],
    };
    give_surface(&doc, SIDE, plane.clone());
    set_bed(&mut doc, "F2.b0", json!(SIDE));
    // A fillet between c1 and c2 in the sketch: c1's side face now ends on
    // the fillet's arc, so its name changes.
    command(
        &mut doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": "c1", "b": "c2", "radius": 3}),
    );
    let bed = bed_face(&doc);
    assert_eq!(bed["face"], SIDE, "the assigned name is kept");
    assert_eq!(bed["faces"].as_array().unwrap().len(), 1, "{bed}");
    let renamed = bed["faces"][0].as_str().unwrap().to_owned();
    assert_ne!(renamed, SIDE);
    assert!(renamed.starts_with("F2:side(c1["), "{renamed}");
    // The mock kernel knows no plane of the renamed face yet: the reason is
    // the resolution's own, not that the surface is no plane.
    assert!(bed["error"].is_string(), "{bed}");
    assert_ne!(bed["error"], NOT_PLANAR);
    // Its plane known, the renamed face lies on the bed.
    give_surface(&doc, &renamed, plane);
    assert_eq!(bed_face(&doc), json!({"face": SIDE, "faces": [renamed]}));
    let (result, _) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    let down = turned(&result, [0.0, -1.0, 0.0]);
    assert!(near(down[2], -1.0), "{down:?}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_component_placed_twice_lies_on_its_face_in_both_places() {
    let mut doc = block();
    let dir = folder("twice");
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.components_from_bodies(&[body]).unwrap();
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1",
               "transform": {"translation": [100, 0, 0]}}),
    );
    set_bed(&mut doc, "F2.b0", json!(top()));
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    // The face applies to every occurrence: both turn over as one piece.
    assert_eq!(result["bed"]["body"], "F2.b0");
    assert_eq!(result["bed"]["kept"], json!([]));
    let placements = &model.parts[0].placements;
    assert_eq!(placements.len(), 2);
    for placement in placements {
        assert!(near(placement.linear[2][2], -1.0), "{placement:?}");
    }
    assert_on_plate(placed_box(&model.parts[0]), [128.0, 128.0]);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_component_placed_twice_turned_differently_keeps_its_placement() {
    let mut doc = block();
    let dir = folder("twice-turned");
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.components_from_bodies(&[body]).unwrap();
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1",
               "transform": {"translation": [100, 0, 0],
                             "rotation": {"axis": [1, 0, 0], "angle": FRAC_PI_2}}}),
    );
    set_bed(&mut doc, "F2.b0", json!(top()));
    let (result, model) = export_3mf_with(&mut doc, &dir, json!(["F2.b0"]), as_designed());
    // The top faces +Z in one place and -Y in the other: no one turn lays
    // it on the bed in both, so the body stays as it is assembled (mitcad#737).
    let bed = &result["bed"];
    assert_eq!(bed["kept"], json!(["F2.b0"]));
    assert_eq!(bed["body"], Value::Null);
    assert_eq!(bed["face"], Value::Null);
    assert_eq!(bed["transform"], Value::Null);
    assert_eq!(bed["problems"], json!([]));
    assert_eq!(result["warnings"], json!([]));
    // The 3MF part's placements are the occurrences' own.
    let placements = &model.parts[0].placements;
    assert_eq!(placements.len(), 2);
    let (first, copy) = if placements[0].translation[0] < 50.0 {
        (placements[0], placements[1])
    } else {
        (placements[1], placements[0])
    };
    let numbers = |p: &mitcad_3mf::Placement| -> Vec<f64> {
        p.linear
            .iter()
            .flatten()
            .chain(&p.translation)
            .copied()
            .collect()
    };
    for (a, b) in numbers(&first).iter().zip(numbers(&IDENTITY)) {
        assert!(near(*a, b), "{first:?}");
    }
    assert!(near(copy.translation[0], 100.0), "{copy:?}");
    // The copy turned a quarter about X: its Y goes to Z, its Z to -Y.
    assert!(near(copy.linear[2][1], 1.0), "{copy:?}");
    assert!(near(copy.linear[1][2], -1.0), "{copy:?}");
    assert!(near(copy.linear[2][2], 0.0), "{copy:?}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_surface_body_has_no_print_bed_face() {
    let mut doc = block();
    let sheet = ImportBody {
        name: Some("Surface".to_owned()),
        color: None,
        shape: MockShape::imported("sheet", 4),
    };
    let added = doc.add_base_feature(BaseInput::new(vec![sheet])).unwrap();
    let surface = added.bodies[0].uid;
    let shape = doc.body_shape(surface).unwrap();
    let face = doc.kernel().faces(shape).unwrap()[0].names[0].clone();
    let plane = SurfaceGeometry::Plane {
        origin: [0.0; 3],
        normal: [1.0, 0.0, 0.0],
    };
    give_surface(&doc, &face, plane.clone());
    let error = refused(
        &mut doc,
        json!({"cmd": "set_print_bed_face", "uid": surface, "face": face}),
    );
    assert!(error.contains("no solid or mesh body"), "{error}");

    // One kept in a file anyway counts for nothing: the 3MF file leaves the
    // surface body out, and its face turns nothing.
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    for entry in file["bodies"].as_array_mut().unwrap() {
        if entry["uid"] == json!(surface) {
            entry["print_bed_face"] = json!(face);
        }
    }
    let text = file.to_string();
    let mut loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    give_surface(&loaded, &face, plane);
    command(&mut loaded, json!({"cmd": "recompute"}));
    // The `bodies` query says why it is not used (mitcad#737), and the
    // face can still be cleared.
    let surface_bed = |doc: &Document<MockKernel>| {
        let bodies = query(doc, json!({"query": "bodies"}));
        let entry = bodies
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["uid"] == json!(surface))
            .unwrap_or_else(|| panic!("no {surface} in {bodies}"))
            .clone();
        entry.get("print_bed_face").cloned().unwrap_or(Value::Null)
    };
    let stored = surface_bed(&loaded);
    assert_eq!(stored["face"], json!(face), "{stored}");
    assert_eq!(stored["faces"].as_array().unwrap().len(), 1, "{stored}");
    assert_eq!(stored["error"], NOT_PRINTED, "{stored}");
    let dir = folder("surface");
    let path = dir.join("print.3mf").to_str().unwrap().to_owned();
    let result = command(
        &mut loaded,
        json!({"cmd": "export", "path": path, "on_bed": true,
               "center": false, "drop": false, "auto_orient": false}),
    );
    assert_eq!(result["skipped"][0]["body"], json!(surface));
    assert_eq!(result["bed"]["body"], Value::Null);
    assert_eq!(result["bed"]["transform"], Value::Null);
    assert_eq!(result["bed"]["problems"], json!([]));
    mitcad_model::testing::remove_temp(&dir);

    // Cleared as any body's face (the context menu's Clear only).
    command(
        &mut loaded,
        json!({"cmd": "set_print_bed_face", "uid": surface, "face": null}),
    );
    assert_eq!(surface_bed(&loaded), Value::Null);
    let label = format!("Clear Print Bed Face of {}", loaded.body_name(surface));
    assert_eq!(document(&loaded)["undo"], label);
}

/// A 20 mm block with its top as the print bed face (F2.b0, Body1) in a
/// component Nut placed three times in Nuts:1 (mitcad#856): Nut:1 as it
/// is, Nut:2 turned a quarter about X and moved 50 along x, Nut:3 turned a
/// quarter about Y and moved 100 along x; the top faces +Z, -Y and +X.
/// The placements' paths are O1/O2, O1/O3 and O1/O4.
fn nuts() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Nuts"},
            {"cmd": "create_component", "name": "Nut"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "Nuts"},
            {"cmd": "copy_occurrence", "occurrence": "O2", "transform": {"translation": [50, 0, 0],
              "rotation": {"axis": [1, 0, 0], "angle": FRAC_PI_2}}},
            {"cmd": "copy_occurrence", "occurrence": "O2", "transform": {"translation": [100, 0, 0],
              "rotation": {"axis": [0, 1, 0], "angle": FRAC_PI_2}}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    set_bed(&mut doc, "F2.b0", json!(top()));
    let paths: Vec<Value> = query(&doc, json!({"query": "instances"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["path"].clone())
        .collect();
    assert_eq!(
        paths,
        [
            json!(["O1", "O2"]),
            json!(["O1", "O3"]),
            json!(["O1", "O4"])
        ]
    );
    doc
}

/// Exports Body1 to a 3MF file on the print bed with more of the export's
/// fields: the result and the file read back.
fn export_nuts(
    doc: &mut Document<MockKernel>,
    dir: &Path,
    more: Value,
) -> (Value, mitcad_3mf::Model) {
    let path = dir.join("nuts.3mf").to_str().unwrap().to_owned();
    let mut export = json!({"cmd": "export", "path": path, "bodies": ["F2.b0"], "on_bed": true});
    for (key, value) in more.as_object().unwrap() {
        export[key] = value.clone();
    }
    let result = command(doc, export);
    let model = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    (result, model)
}

/// A placement of the block lies on its top: the top's outward normal
/// (+Z in the component) straight down, the top (z = 20) on the bed and
/// nothing under it. Its box.
fn on_its_top(part: &mitcad_3mf::Part, placement: &mitcad_3mf::Placement) -> ([f64; 3], [f64; 3]) {
    let down: [f64; 3] = std::array::from_fn(|r| placement.linear[r][2]);
    assert!(
        near(down[0], 0.0) && near(down[1], 0.0) && near(down[2], -1.0),
        "{placement:?}"
    );
    let top = placement.apply([10.0, 10.0, 20.0]);
    assert!(near(top[2], 0.0), "{top:?}");
    let alone = mitcad_3mf::Part {
        placements: vec![*placement],
        ..part.clone()
    };
    let found = placed_box(&alone);
    assert!(near(found.0[2], 0.0), "lowest z {}", found.0[2]);
    found
}

#[test]
fn a_chosen_occurrence_of_a_body_placed_turned_differently_lies_on_its_face() {
    let mut doc = nuts();
    let dir = folder("nuts-one");
    // Nut:2 alone: one object, its top on the bed and centred on the plate.
    let (result, model) = export_nuts(&mut doc, &dir, json!({"occurrences": {"F2.b0": ["O1/O3"]}}));
    assert!(model.objects.is_empty(), "one object: {:?}", model.objects);
    assert_eq!(model.parts.len(), 1);
    assert_eq!(model.parts[0].placements.len(), 1);
    let bed = &result["bed"];
    assert_eq!(bed["body"], "F2.b0");
    assert_eq!(bed["face"], top());
    assert_eq!(bed["kept"], json!([]));
    assert_eq!(bed["objects"], json!([]));
    assert_eq!(bed["on_faces"], 1);
    let part = &model.parts[0];
    on_its_top(part, &part.placements[0]);
    assert_on_plate(placed_box(part), [128.0, 128.0]);
    // The path by occurrence names picks the same.
    let (by_name, _) = export_nuts(
        &mut doc,
        &dir,
        json!({"occurrences": {"F2.b0": ["Nuts:1/Nut:2"]}}),
    );
    assert_eq!(by_name["bed"], result["bed"]);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn every_occurrence_chosen_is_an_object_of_its_own_on_its_face() {
    let mut doc = nuts();
    let dir = folder("nuts-all");
    // An empty list: every placement, each an object on its top.
    let (result, model) = export_nuts(&mut doc, &dir, json!({"occurrences": {"F2.b0": []}}));
    let bed = &result["bed"];
    assert_eq!(bed["on_faces"], 3);
    assert_eq!(bed["body"], Value::Null, "no object of the rest: {bed}");
    let objects = bed["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 3, "{bed}");
    for object in objects {
        assert_eq!(object["body"], "F2.b0");
        assert_eq!(object["face"], top());
        assert!(
            object["name"].as_str().unwrap().contains(" (Nuts:1/Nut:"),
            "{object}"
        );
    }
    assert_eq!(model.parts.len(), 1);
    assert_eq!(model.parts[0].placements.len(), 3);
    assert_eq!(model.objects.len(), 3, "a build item each");
    // Each on its top, side by side along X with a gap of 5 mm, the row
    // centred on the plate.
    let mut boxes = Vec::new();
    for object in &model.objects {
        assert_eq!(object.components.len(), 1, "{object:?}");
        let (p, k) = object.components[0];
        let part = &model.parts[p];
        boxes.push(on_its_top(part, &part.placements[k]));
    }
    boxes.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    for pair in boxes.windows(2) {
        assert!(near(pair[1].0[0] - pair[0].1[0], 5.0), "{pair:?}");
    }
    assert!(
        near((boxes[0].0[0] + boxes[2].1[0]) / 2.0, 128.0),
        "{boxes:?}"
    );
    // The three listed by their paths are the same.
    let (listed, _) = export_nuts(
        &mut doc,
        &dir,
        json!({"occurrences": {"F2.b0": ["O1/O2", "O1/O3", "O1/O4"]}}),
    );
    assert_eq!(listed["bed"]["on_faces"], 3);
    assert_eq!(listed["bed"]["objects"].as_array().unwrap().len(), 3);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_component_sent_whole_keeps_its_assembly() {
    let mut doc = nuts();
    let dir = folder("nuts-whole");
    // Nuts:1 sent whole: one object, the nuts as assembled (their tops
    // turned differently: none on the bed).
    // Neither centred nor dropped (mitcad#1109), so that the placements
    // are the design's.
    for more in [
        json!({"occurrences": {"F2.b0": []}, "whole": ["Nuts:1"], "center": false, "drop": false}),
        json!({"center": false, "drop": false}),
    ] {
        let (result, model) = export_nuts(&mut doc, &dir, more.clone());
        let bed = &result["bed"];
        assert!(model.objects.is_empty(), "{more}: {:?}", model.objects);
        assert_eq!(bed["kept"], json!(["F2.b0"]), "{more}: {bed}");
        assert_eq!(bed["transform"], Value::Null, "{more}");
        assert_eq!(bed["objects"], json!([]), "{more}");
        assert_eq!(bed["on_faces"], 0, "{more}");
        let placements = &model.parts[0].placements;
        assert_eq!(placements.len(), 3, "{more}");
        assert!(placements.contains(&IDENTITY), "{more}: {placements:?}");
    }
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn the_occurrences_to_write_are_checked() {
    let mut doc = nuts();
    let dir = folder("nuts-checked");
    let three = dir.join("nuts.3mf").to_str().unwrap().to_owned();
    let stl = dir.join("nuts.stl").to_str().unwrap().to_owned();
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": stl, "on_bed": true, "occurrences": {"F2.b0": ["O1/O3"]}}),
    );
    assert!(error.contains("only 3MF files"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": three, "occurrences": {"F2.b0": ["O1"]}}),
    );
    assert!(error.contains("is not placed by Nuts:1"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": three, "occurrences": {"F2.b0": ["O9"]}}),
    );
    assert!(error.contains("there is no occurrence O9"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": three, "on_bed": true, "whole": [""]}),
    );
    assert!(error.contains("not the design"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}

/// Body2's side as its print bed face: facing -Y at y = 0, as the mock
/// kernel gives it (mitcad#857).
const PAIR_SIDE: &str = "F3:side(c1[c4,c2])";

/// A component Pair (O1, Pair:1) of two bodies, each with a print bed face
/// (mitcad#857): Body1 (F2.b0), the 20 mm block on z 0 to 20, its top
/// (+Z); Body2 (F3.b0), 10 high under it on z -10 to 0, its side facing -Y
/// at y = 0.
fn pair() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Pair"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": -10}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    give_surface(
        &doc,
        PAIR_SIDE,
        SurfaceGeometry::Plane {
            origin: [10.0, 0.0, -5.0],
            normal: [0.0, -1.0, 0.0],
        },
    );
    set_bed(&mut doc, "F2.b0", json!(top()));
    set_bed(&mut doc, "F3.b0", json!(PAIR_SIDE));
    doc
}

/// Exports both bodies of Pair to a 3MF file on the print bed with more of
/// the export's fields: the result and the file read back.
fn export_pair(
    doc: &mut Document<MockKernel>,
    dir: &Path,
    more: Value,
) -> (Value, mitcad_3mf::Model) {
    let path = dir.join("pair.3mf").to_str().unwrap().to_owned();
    let mut export =
        json!({"cmd": "export", "path": path, "bodies": ["F2.b0", "F3.b0"], "on_bed": true});
    for (key, value) in more.as_object().unwrap() {
        export[key] = value.clone();
    }
    let result = command(doc, export);
    let model = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    (result, model)
}

/// The two bodies keep their places to each other: one and the same
/// placement each, a turn without stretching.
fn assert_together(model: &mitcad_3mf::Model) {
    assert_eq!(model.parts.len(), 2);
    assert_eq!(model.parts[0].placements.len(), 1);
    assert_eq!(model.parts[0].placements, model.parts[1].placements);
    let placement = model.parts[0].placements[0];
    let (a, b) = ([0.0, 0.0, -10.0], [20.0, 20.0, 20.0]);
    let (pa, pb) = (placement.apply(a), placement.apply(b));
    let length =
        |p: [f64; 3], q: [f64; 3]| (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt();
    assert!(near(length(pa, pb), length(a, b)), "{placement:?}");
}

#[test]
fn a_piece_kept_for_several_bed_faces_lists_them_to_choose_from() {
    let mut doc = pair();
    let dir = folder("pair-plan");
    let path = dir.join("pair.3mf").to_str().unwrap().to_owned();
    // As 3D Print sends Pair:1 whole, the plan only: nothing is written.
    for more in [
        json!({"occurrences": {"F2.b0": [], "F3.b0": []}, "whole": ["Pair:1"]}),
        json!({}),
    ] {
        let mut export = json!({"cmd": "export", "path": path, "bodies": ["F2.b0", "F3.b0"],
                                "on_bed": true, "plan": true});
        for (key, value) in more.as_object().unwrap() {
            export[key] = value.clone();
        }
        let result = command(&mut doc, export);
        assert!(!Path::new(&path).exists(), "{more}: a file was written");
        let bed = &result["bed"];
        assert_eq!(bed["kept"], json!(["F2.b0", "F3.b0"]), "{more}: {bed}");
        assert_eq!(bed["transform"], Value::Null, "{more}");
        assert_eq!(
            bed["choices"],
            json!([
                {"body": "F2.b0", "name": "Body1", "face": top(), "occurrence": "O1",
                 "occurrence_name": "Pair:1"},
                {"body": "F3.b0", "name": "Body2", "face": PAIR_SIDE, "occurrence": "O1",
                 "occurrence_name": "Pair:1"},
            ]),
            "{more}"
        );
    }
    // A body placed three times turned differently: a choice per
    // placement (counted per occurrence).
    let mut placed = nuts();
    let result = command(
        &mut placed,
        json!({"cmd": "export", "path": path, "bodies": ["F2.b0"], "on_bed": true, "plan": true,
               "occurrences": {"F2.b0": []}, "whole": ["Nuts:1"]}),
    );
    let occurrences: Vec<&Value> = result["bed"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["occurrence"])
        .collect();
    assert_eq!(
        occurrences,
        [&json!("O1/O2"), &json!("O1/O3"), &json!("O1/O4")]
    );
    // One bed face: nothing to choose.
    set_bed(&mut doc, "F3.b0", Value::Null);
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path, "bodies": ["F2.b0", "F3.b0"], "on_bed": true, "plan": true}),
    );
    assert_eq!(result["bed"]["body"], "F2.b0");
    assert_eq!(result["bed"]["choices"], json!([]));
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_bed_face_chosen_puts_the_piece_on_it() {
    let mut doc = pair();
    let dir = folder("pair-chosen");
    for whole in [
        json!({"occurrences": {"F2.b0": [], "F3.b0": []}, "whole": ["Pair:1"]}),
        json!({}),
    ] {
        // Body2's side: it faces down, on the bed at z = 0, the bodies
        // together as assembled.
        let mut more = whole.clone();
        more["bed_face"] = json!({"body": "F3.b0", "occurrence": "Pair:1"});
        let (result, model) = export_pair(&mut doc, &dir, more);
        let bed = &result["bed"];
        assert_eq!(bed["body"], "F3.b0", "{whole}: {bed}");
        assert_eq!(bed["face"], PAIR_SIDE, "{whole}");
        assert_eq!(bed["kept"], json!([]), "{whole}");
        assert_eq!(bed["choices"], json!([]), "{whole}");
        assert_eq!(bed["on_faces"], 1, "{whole}");
        let down = turned(&result, [0.0, -1.0, 0.0]);
        assert!(
            near(down[0], 0.0) && near(down[1], 0.0) && near(down[2], -1.0),
            "{whole}: {down:?}"
        );
        assert!(near(moved(&result, [10.0, 0.0, -5.0])[2], 0.0), "{whole}");
        assert!(model.objects.is_empty(), "{whole}: one object");
        assert_together(&model);
        let placement = model.parts[1].placements[0];
        assert!(near(placement.apply([10.0, 0.0, -5.0])[2], 0.0), "{whole}");
        let mut lowest = f64::INFINITY;
        for part in &model.parts {
            lowest = lowest.min(placed_box(part).0[2]);
        }
        assert!(near(lowest, 0.0), "{whole}: lowest z {lowest}");

        // Body1's top, by uids in a list: the piece turned over onto it.
        let mut more = whole.clone();
        more["bed_face"] = json!([{"body": "Body1", "occurrence": "O1"}]);
        let (result, model) = export_pair(&mut doc, &dir, more);
        assert_eq!(result["bed"]["body"], "F2.b0", "{whole}");
        assert!(near(turned(&result, [0.0, 0.0, 1.0])[2], -1.0), "{whole}");
        assert!(near(moved(&result, [10.0, 10.0, 20.0])[2], 0.0), "{whole}");
        assert_together(&model);
    }
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_bed_face_chosen_is_checked() {
    let mut doc = pair();
    let dir = folder("pair-checked");
    let path = dir.join("pair.3mf").to_str().unwrap().to_owned();
    let export = |more: Value| {
        let mut export =
            json!({"cmd": "export", "path": path, "bodies": ["F2.b0", "F3.b0"], "on_bed": true});
        for (key, value) in more.as_object().unwrap() {
            export[key] = value.clone();
        }
        export
    };
    // Two in one piece.
    let error = refused(
        &mut doc,
        export(json!({"bed_face": [{"body": "F2.b0", "occurrence": "O1"},
                                   {"body": "F3.b0", "occurrence": "O1"}]})),
    );
    assert!(error.contains("chosen for one piece"), "{error}");
    // Not where the body is written.
    let error = refused(
        &mut doc,
        export(json!({"bed_face": {"body": "F2.b0", "occurrence": ""}})),
    );
    assert!(
        error.contains("is not written as the design itself places it"),
        "{error}"
    );
    // Without one.
    set_bed(&mut doc, "F3.b0", Value::Null);
    let error = refused(
        &mut doc,
        export(json!({"bed_face": {"body": "F3.b0", "occurrence": "O1"}})),
    );
    assert!(error.contains("Body2 has no print bed face"), "{error}");
    // A body not written.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": path, "bodies": ["F3.b0"], "on_bed": true,
               "bed_face": {"body": "F2.b0", "occurrence": "O1"}}),
    );
    assert!(error.contains("not among the bodies written"), "{error}");
    // Not on the bed.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": path, "bed_face": {"body": "F2.b0", "occurrence": "O1"}}),
    );
    assert!(error.contains("(on_bed)"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": path, "plan": true}),
    );
    assert!(error.contains("(on_bed)"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}

/// A block of `width` x `depth` on XY, `height` high (F2.b0, Body1).
fn sized_block(width: f64, depth: f64, height: f64) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": width, "height": depth},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
        ]),
    );
    doc
}

#[test]
fn a_body_without_a_bed_face_far_from_the_origin_is_dropped_and_centred() {
    // A 30 x 20 x 10 block, already on its largest face, in a component
    // moved to (300, 400, 50) (mitcad#1109).
    let mut doc = sized_block(30.0, 20.0, 10.0);
    let dir = folder("far");
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.components_from_bodies(&[body]).unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "O1", "transform":
               {"translation": [300, 400, 50]}}),
    );
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    let bed = &result["bed"];
    // The automatic orientation keeps it on its bottom (−Z down), which is
    // not stored.
    assert_eq!(bed["auto"], true, "{bed}");
    assert_eq!(bed["body"], "F2.b0");
    assert_eq!(bed["face"], bottom());
    assert_eq!(bed["occurrence"], "O1");
    assert_eq!(bed["center"], true);
    assert_eq!(bed["drop"], true);
    assert_eq!(bed["on_faces"], 0, "no print bed face of the body");
    assert_eq!(bed_face(&doc), Value::Null, "nothing stored");
    let (min, max) = placed_box(&model.parts[0]);
    assert_on_plate((min, max), [128.0, 128.0]);
    assert!(near(max[2], 10.0), "{max:?}");
    assert!(
        near(max[0] - min[0], 30.0) && near(max[1] - min[1], 20.0),
        "{min:?} {max:?}"
    );

    // Center on Plate and Drop to Bed off: as the design has it.
    let (result, model) = export_3mf_with(
        &mut doc,
        &dir,
        json!(["F2.b0"]),
        json!({"center": false, "drop": false}),
    );
    assert_eq!(result["bed"]["center"], false);
    assert_eq!(result["bed"]["drop"], false);
    let (min, max) = placed_box(&model.parts[0]);
    for (found, wanted) in min.iter().zip([300.0, 400.0, 50.0]) {
        assert!(near(*found, wanted), "{min:?}");
    }
    for (found, wanted) in max.iter().zip([330.0, 420.0, 60.0]) {
        assert!(near(*found, wanted), "{max:?}");
    }
    // Drop to Bed alone: on the bed where the design has it in X and Y.
    let (_, model) = export_3mf_with(&mut doc, &dir, json!(["F2.b0"]), json!({"center": false}));
    let (min, _) = placed_box(&model.parts[0]);
    assert!(
        near(min[0], 300.0) && near(min[1], 400.0) && near(min[2], 0.0),
        "{min:?}"
    );

    // An STL file the same.
    let path = dir.join("far.stl").to_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "export", "path": path, "bodies": ["F2.b0"], "ascii": true, "on_bed": true}),
    );
    let triangles = mitcad_model::exchange::read_stl(&std::fs::read(&path).unwrap());
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in triangles.iter().flatten() {
        min = std::array::from_fn(|i| min[i].min(p[i]));
        max = std::array::from_fn(|i| max[i].max(p[i]));
    }
    assert_on_plate((min, max), [128.0, 128.0]);

    // Only on the print bed.
    let error = refused(
        &mut doc,
        json!({"cmd": "export", "path": path, "center": false}),
    );
    assert!(error.contains("(on_bed)"), "{error}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn the_automatic_orientation_lays_a_block_on_its_largest_face() {
    // 20 x 10 x 40, standing: its largest faces are the 20 x 40 sides
    // facing -Y and +Y (mitcad#1109).
    let mut doc = sized_block(20.0, 10.0, 40.0);
    let dir = folder("largest");
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    let bed = &result["bed"];
    assert_eq!(bed["auto"], true, "{bed}");
    assert!(
        bed["face"].as_str().unwrap().starts_with("F2:side("),
        "{bed}"
    );
    let (min, max) = placed_box(&model.parts[0]);
    assert_on_plate((min, max), [128.0, 128.0]);
    // Lying: 10 high, its 20 x 40 face on the bed.
    assert!(near(max[2], 10.0), "{min:?} {max:?}");
    let mut spans = [max[0] - min[0], max[1] - min[1]];
    spans.sort_by(f64::total_cmp);
    assert!(near(spans[0], 20.0) && near(spans[1], 40.0), "{spans:?}");
    // The plan says the same, and nothing is written.
    let path = dir.join("plan.3mf").to_str().unwrap().to_owned();
    let plan = command(
        &mut doc,
        json!({"cmd": "export", "path": path, "bodies": ["F2.b0"], "on_bed": true, "plan": true}),
    );
    assert_eq!(plan["bed"]["face"], bed["face"]);
    assert_eq!(plan["bed"]["auto"], true);
    assert!(!Path::new(&path).exists());
    // Without the automatic orientation: as the design has it, standing,
    // but dropped and centred.
    let (result, model) = export_3mf_with(
        &mut doc,
        &dir,
        json!(["F2.b0"]),
        json!({"auto_orient": false}),
    );
    assert_eq!(result["bed"]["body"], Value::Null);
    assert_eq!(result["bed"]["auto"], false);
    let (min, max) = placed_box(&model.parts[0]);
    assert_on_plate((min, max), [128.0, 128.0]);
    assert!(near(max[2], 40.0), "{max:?}");
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_bodys_own_bed_face_wins_over_the_automatic_one() {
    // The standing 20 x 10 x 40 block would lie on a side; its top is its
    // print bed face (mitcad#1109).
    let mut doc = sized_block(20.0, 10.0, 40.0);
    let dir = folder("own");
    set_bed(&mut doc, "F2.b0", json!(top()));
    let (result, model) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    let bed = &result["bed"];
    assert_eq!(bed["face"], top(), "{bed}");
    assert_eq!(bed["auto"], false);
    assert_eq!(bed["on_faces"], 1);
    assert_eq!(bed["occurrence"], "");
    let down = turned(&result, [0.0, 0.0, 1.0]);
    assert!(near(down[2], -1.0), "{down:?}");
    let (min, max) = placed_box(&model.parts[0]);
    assert_on_plate((min, max), [128.0, 128.0]);
    assert!(near(max[2], 40.0), "standing on its top: {max:?}");
    // Cleared: the automatic orientation again.
    set_bed(&mut doc, "F2.b0", Value::Null);
    let (result, _) = export_3mf(&mut doc, &dir, json!(["F2.b0"]));
    assert_eq!(result["bed"]["auto"], true);
    assert_ne!(result["bed"]["face"], top());
    mitcad_model::testing::remove_temp(&dir);
}
