// SPDX-License-Identifier: MIT
//! Decals (mitcad#2244) through the JSON api with the mock kernel: an
//! image put on a block's top, following the top when the block grows (its
//! centre at the same place of the face), turned, flipped, resized with
//! Lock Aspect, hidden, renamed and deleted as undo steps that recompute
//! nothing; one wrapped onto a rod's side keeping its width along the
//! developed face when the rod grows thicker; saved with its bytes and read
//! back the same; a decal whose face is gone saying why; the refusals; a
//! decal cut at its face's edges and one more than once around a cone
//! refused (mitcad#2266); a decal on a body of an occurrence following it
//! to another parent and into a new component (mitcad#2527), and its body
//! into an existing one (mitcad#2555).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::kernel::{ConeFace, Curve3};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

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

fn decals(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "decals"}))
        .as_array()
        .unwrap()
        .clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn near(actual: &Value, expected: f64) {
    let value = actual
        .as_f64()
        .unwrap_or_else(|| panic!("{actual} is no number"));
    assert!(
        (value - expected).abs() < 1e-6,
        "{actual} is not {expected}"
    );
}

fn point(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| {
        value[i]
            .as_f64()
            .unwrap_or_else(|| panic!("{value} is no point"))
    })
}

fn near_point(actual: &Value, expected: [f64; 3]) {
    let p = point(actual);
    assert!(
        (0..3).all(|i| (p[i] - expected[i]).abs() < 1e-6),
        "{actual} is not {expected:?}"
    );
}

/// A 24-bit BMP file of `width` x `height` grey pixels, 4000 pixels per
/// metre (0.25 mm each).
fn bmp(width: u32, height: u32) -> Vec<u8> {
    let row = (width * 3).div_ceil(4) * 4;
    let pixels = row * height;
    let mut bytes = b"BM".to_vec();
    bytes.extend_from_slice(&(54 + pixels).to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 24, 0]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&pixels.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.resize(bytes.len() + pixels as usize, 0x80);
    bytes
}

fn data() -> String {
    mitcad_model::internal::base64::encode(&bmp(200, 100))
}

/// A block (F2.b0) 20 high on a 20 x 20 rectangle on XY, its height the
/// parameter `height`.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
        ]),
    );
    doc
}

fn top() -> Value {
    json!({"body": "F2.b0", "face": format!("F2:end({R})")})
}

/// A rod (F2.b0): a circle of diameter 20 (d1) on XY extruded 30, its side
/// F2:side(c1) a cylinder of radius 10 along +Z from z = 0.
fn rod() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn a_decal_on_a_blocks_top_follows_it_and_changes_as_undo_steps() {
    let mut doc = block();
    assert!(decals(&doc).is_empty());
    let inserted = command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "file_name": "label.bmp",
               "face": top(), "at": [10, 8, 25], "width": 10}),
    );
    assert_eq!(inserted["name"], "Decal1");
    assert_eq!(inserted["recomputed"], 0, "a decal changes no geometry");
    assert_eq!(undo_label(&doc), "Insert Decal1");
    let decal = &decals(&doc)[0];
    assert_eq!(decal["file_name"], "label.bmp");
    assert_eq!(decal["format"], "bmp");
    assert_eq!(decal["pixels"], json!([200, 100]));
    assert_eq!(decal["surface"], "plane");
    assert_eq!(decal["visible"], true);
    assert_eq!(decal["lock_aspect"], true);
    assert!(
        decal.get("image").is_none(),
        "the list leaves the bytes out"
    );
    // Opaque by default; 10 wide, 5 high in the image's proportions; its
    // centre the top's point under the point given.
    near(&decal["opacity"], 1.0);
    near(&decal["width"], 10.0);
    near(&decal["height"], 5.0);
    near(&decal["center"][0], 10.0);
    near(&decal["center"][1], 8.0);
    near_point(&decal["corners"][0], [5.0, 10.5, 20.0]);
    near_point(&decal["corners"][2], [15.0, 5.5, 20.0]);
    // One quad on a plane, its normal out of the block.
    assert_eq!(decal["mesh"]["columns"], 1);
    assert_eq!(decal["mesh"]["rows"], 1);
    near_point(&decal["mesh"]["normals"][0], [0.0, 0.0, 1.0]);
    assert_eq!(decal["mesh"]["uvs"][3], json!([1.0, 1.0]));
    let file = query(&doc, json!({"query": "decal_image", "name": "Decal1"}));
    assert_eq!(file["data"], data());
    assert_eq!(file["sha256"], decal["sha256"]);

    // The block made higher: the decal stays on its top, its centre at the
    // same place of the face.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 30}),
    );
    let decal = &decals(&doc)[0];
    near_point(&decal["corners"][0], [5.0, 10.5, 30.0]);
    near(&decal["center"][0], 10.0);
    near(&decal["center"][1], 8.0);

    // Turned a quarter, flipped, half opaque: its right is -y, its up -x.
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "angle": 90, "flip": true, "opacity": 0.5}),
    );
    assert_eq!(undo_label(&doc), "Edit Decal1");
    let decal = &decals(&doc)[0];
    near(&decal["opacity"], 0.5);
    near_point(&decal["corners"][0], [7.5, 13.0, 30.0]);
    // Lock Aspect: the height sets the width; without it each alone.
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "height": 4}),
    );
    near(&decals(&doc)[0]["width"], 8.0);
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "lock_aspect": false, "height": 6}),
    );
    let decal = &decals(&doc)[0];
    near(&decal["width"], 8.0);
    near(&decal["height"], 6.0);
    assert_eq!(decal["lock_aspect"], false);
    // Nothing changed: no undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "opacity": 0.5}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    near(&decals(&doc)[0]["height"], 4.0);
    command(&mut doc, json!({"cmd": "redo"}));

    command(
        &mut doc,
        json!({"cmd": "set_decal_visible", "name": "Decal1", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Decal1");
    assert_eq!(decals(&doc)[0]["visible"], false);
    command(
        &mut doc,
        json!({"cmd": "rename_decal", "name": "Decal1", "new_name": "Label"}),
    );
    assert_eq!(undo_label(&doc), "Rename Decal1");
    // The next default name is free again.
    let second = command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top()}),
    );
    assert_eq!(second["name"], "Decal1");
    command(&mut doc, json!({"cmd": "delete_decal", "name": "Label"}));
    assert_eq!(undo_label(&doc), "Delete Label");
    assert_eq!(decals(&doc).len(), 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(decals(&doc)[0]["name"], "Label");

    // Where a new one would lie: the panel's live result.
    let outline = query(
        &doc,
        json!({"query": "decal_outline", "face": top(), "pixels": [200, 100],
               "at": [10, 10, 30], "width": 8}),
    );
    assert_eq!(outline["surface"], "plane");
    near(&outline["size"][1], 4.0);
    near_point(&outline["corners"][0], [6.0, 12.0, 30.0]);
}

#[test]
fn a_decal_wraps_onto_a_rods_side_keeping_its_width() {
    let mut doc = rod();
    let side = json!({"body": "F2.b0", "face": "F2:side(c1)"});
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": side, "at": [12, 0, 15], "width": 10}),
    );
    let arc = |decal: &Value, radius: f64| {
        let left = point(&decal["corners"][0]);
        let right = point(&decal["corners"][1]);
        for p in [left, right] {
            assert!(
                (p[0].hypot(p[1]) - radius).abs() < 1e-6,
                "{p:?} is off the face"
            );
        }
        let cos = (left[0] * right[0] + left[1] * right[1]) / (radius * radius);
        cos.clamp(-1.0, 1.0).acos() * radius
    };
    let decal = &decals(&doc)[0];
    assert_eq!(decal["surface"], "cylinder");
    // Its centre the side's point nearest to the point given: around the
    // axis from +X, 15 up from the rod's bottom.
    near(&decal["center"][0], 0.0);
    near(&decal["center"][1], 15.0);
    assert!(
        (arc(decal, 10.0) - 10.0).abs() < 1e-6,
        "10 mm around the face"
    );
    let columns = decal["mesh"]["columns"].as_u64().unwrap();
    assert!(columns > 1, "wrapped in pieces");
    assert_eq!(decal["mesh"]["rows"], 1, "a cylinder curves only around it");
    // Every point of its grid on the face, its normals out of it.
    for (p, n) in decal["mesh"]["points"]
        .as_array()
        .unwrap()
        .iter()
        .zip(decal["mesh"]["normals"].as_array().unwrap())
    {
        let p = point(p);
        let n = point(n);
        assert!((p[0].hypot(p[1]) - 10.0).abs() < 1e-6);
        assert!((p[0] * n[0] + p[1] * n[1]) / 10.0 > 0.999);
    }
    // The rod made thicker: the decal stays 10 mm around its side.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 30}),
    );
    let decal = &decals(&doc)[0];
    assert_eq!(decal["surface"], "cylinder");
    assert!((arc(decal, 15.0) - 10.0).abs() < 1e-6, "still 10 mm around");
    // Wider than once around the side is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "width": 100}),
    );
    assert!(error.contains("more than once around"), "{error}");
}

#[test]
fn decals_are_saved_with_their_bytes_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "file_name": "label.bmp", "face": top(),
               "center": [5, 6], "angle": 30, "flip": true, "opacity": 0.75, "width": 12}),
    );
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top(),
               "lock_aspect": false, "width": 4, "height": 9}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_decal_visible", "name": "Decal2", "visible": false}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let stored = &file["decals"];
    assert_eq!(stored[0]["name"], "Decal1");
    assert_eq!(stored[0]["image"]["format"], "bmp");
    assert_eq!(stored[0]["image"]["width"], 200);
    assert_eq!(stored[0]["image"]["data"], data());
    assert!(stored[0].get("visible").is_none(), "shown is the default");
    assert!(
        stored[0].get("lock_aspect").is_none(),
        "locked is the default"
    );
    assert_eq!(stored[1]["visible"], false);
    assert_eq!(stored[1]["lock_aspect"], false);
    assert!(stored[1].get("opacity").is_none(), "opaque is the default");

    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(decals(&loaded), decals(&doc));
    let file = query(&loaded, json!({"query": "decal_image", "name": "Decal1"}));
    assert_eq!(file["data"], data(), "the image's bytes kept");

    // Older files without decals load as before.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(decals(&old).is_empty());
    // A name twice.
    let twice = saved.replace("\"Decal2\"", "\"Decal1\"");
    let Err(error) = Document::from_json(&twice, MockKernel::default()) else {
        panic!("a name twice was read");
    };
    assert!(error.to_string().contains("more than once"), "{error}");
}

#[test]
fn a_decal_whose_face_is_gone_says_why() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top(), "width": 10}),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    let decal = &decals(&doc)[0];
    let error = decal["error"].as_str().unwrap_or_else(|| panic!("{decal}"));
    assert!(error.contains("F2.b0"), "{error}");
    assert!(decal.get("corners").is_none());
    assert!(decal.get("mesh").is_none());
    // Its settings are kept and can still be changed; it is back on the
    // top with the block.
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "opacity": 0.4}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(decals(&doc)[0].get("error").is_none());
    near_point(&decals(&doc)[0]["mesh"]["normals"][0], [0.0, 0.0, 1.0]);
}

#[test]
fn decals_refuse_what_they_cannot_keep() {
    let mut doc = block();
    let insert = |extra: Value| {
        let mut command = json!({"cmd": "insert_decal", "data": data(), "face": top()});
        for (key, value) in extra.as_object().unwrap() {
            command[key] = value.clone();
        }
        command
    };
    // Not a face, a face that is not there, a face of a kind not taken.
    let error = refused(&mut doc, insert(json!({"face": "xy"})));
    assert!(error.contains("a face of a body"), "{error}");
    let gone = json!({"body": "F2.b0", "face": "F9:end(r{c1})"});
    let error = refused(&mut doc, insert(json!({"face": gone})));
    assert!(error.contains("the decal's face"), "{error}");
    doc.kernel().surfaces.borrow_mut().insert(
        format!("F2:end({R})"),
        SurfaceGeometry::Sphere {
            center: [10.0, 10.0, 0.0],
            radius: 20.0,
        },
    );
    let error = refused(&mut doc, insert(json!({})));
    assert!(
        error.contains("planar, cylindrical or conical face") && error.contains("sphere"),
        "{error}"
    );
    doc.kernel().surfaces.borrow_mut().clear();
    // An image that is none, too large, or given twice.
    let gif = mitcad_model::internal::base64::encode(b"GIF89a");
    let error = refused(&mut doc, insert(json!({"data": gif})));
    assert!(error.contains("no PNG, JPEG, BMP, WebP or HEIF"), "{error}");
    let large = mitcad_model::internal::base64::encode(&vec![0u8; 33 << 20]);
    let error = refused(&mut doc, insert(json!({"data": large})));
    assert!(
        error.contains("more than the 32 MB a design embeds"),
        "{error}"
    );
    let error = refused(&mut doc, insert(json!({"path": "a.png"})));
    assert!(error.contains("not both"), "{error}");
    let error = refused(&mut doc, json!({"cmd": "insert_decal", "face": top()}));
    assert!(error.contains("give the decal's image"), "{error}");
    // Sizes, opacity, Lock Aspect.
    let error = refused(&mut doc, insert(json!({"width": 0})));
    assert!(error.contains("greater than zero"), "{error}");
    let error = refused(&mut doc, insert(json!({"opacity": 1.5})));
    assert!(error.contains("from 0 to 1"), "{error}");
    let error = refused(&mut doc, insert(json!({"width": 10, "height": 10})));
    assert!(error.contains("Lock Aspect"), "{error}");
    let error = refused(&mut doc, insert(json!({"depth": 3})));
    assert!(error.contains("depth"), "{error}");
    command(&mut doc, insert(json!({"name": "Label"})));
    let error = refused(&mut doc, insert(json!({"name": "Label"})));
    assert_eq!(error, "a decal Label exists");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Gone", "opacity": 0.2}),
    );
    assert_eq!(error, "there is no decal Gone");
}

/// The points the decal's triangles use (mitcad#2266).
fn triangle_points(decal: &Value) -> Vec<[f64; 3]> {
    let points = decal["mesh"]["points"].as_array().unwrap();
    decal["mesh"]["triangles"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t.as_array().unwrap().clone())
        .map(|k| point(&points[k.as_u64().unwrap() as usize]))
        .collect()
}

#[test]
fn a_decal_is_cut_at_its_faces_edges() {
    // The block's top (20 x 20 at z = 20) bounded by its four edges, a
    // decal 10 x 5 centred on its right edge.
    let mut doc = block();
    let line = |a: [f64; 3], b: [f64; 3]| Curve3::Line { start: a, end: b };
    *doc.kernel().curves.borrow_mut() = vec![
        line([0.0, 0.0, 20.0], [20.0, 0.0, 20.0]),
        line([20.0, 0.0, 20.0], [20.0, 20.0, 20.0]),
        line([20.0, 20.0, 20.0], [0.0, 20.0, 20.0]),
        line([0.0, 20.0, 20.0], [0.0, 0.0, 20.0]),
    ];
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top(), "center": [20, 10], "width": 10}),
    );
    let decal = &decals(&doc)[0];
    // Its place and size kept; only the half on the top drawn.
    near(&decal["center"][0], 20.0);
    near(&decal["width"], 10.0);
    near_point(&decal["corners"][2], [25.0, 7.5, 20.0]);
    assert_eq!(decal["mesh"]["cut"], true);
    let used = triangle_points(decal);
    assert!(!used.is_empty());
    assert!(used.iter().all(|p| p[0] <= 20.0 + 1e-9), "{used:?}");
    assert!(used.iter().any(|p| (p[0] - 20.0).abs() < 1e-9));
    // Moved onto the top: whole, the grid's two triangles.
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "center": [10, 10]}),
    );
    let decal = &decals(&doc)[0];
    assert_eq!(decal["mesh"]["cut"], false);
    assert_eq!(decal["mesh"]["triangles"].as_array().unwrap().len(), 2);
}

#[test]
fn a_decal_more_than_once_around_a_cone_is_refused() {
    // The block's side F2:side(c1[c4,c2]) taken for a cone with its apex at
    // z = 40 opening downwards, half angle 10 degrees, its edges the circles
    // at z = 0 and 20: developed, a fan of 62.5 degrees, the low end 40.6
    // mm from the apex.
    let mut doc = block();
    let half_angle = 10f64.to_radians();
    doc.kernel().face_cones.borrow_mut().insert(
        "F2:side(c1[c4,c2])".to_owned(),
        ConeFace {
            apex: [0.0, 0.0, 40.0],
            axis: [0.0, 0.0, -1.0],
            half_angle,
            internal: false,
        },
    );
    let circle = |z: f64| Curve3::Conic {
        center: [0.0, 0.0, z],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: (40.0 - z) * half_angle.tan(),
        minor: (40.0 - z) * half_angle.tan(),
        start: 0.0,
        end: std::f64::consts::TAU,
        closed: true,
    };
    *doc.kernel().curves.borrow_mut() = vec![circle(0.0), circle(20.0)];
    let side = json!({"body": "F2.b0", "face": "F2:side(c1[c4,c2])"});
    // 100 mm wide 5 mm up the face: seen at about 112 degrees from the
    // apex at its top row, more than the fan.
    let error = refused(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": side, "center": [0, 5],
               "lock_aspect": false, "width": 100, "height": 4}),
    );
    assert!(
        error.contains("wide around the conical face at its widest row, more than once around it"),
        "{error}"
    );
    // 20 mm wide: about 33 degrees, taken; widened past once around, refused.
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": side, "center": [0, 5],
               "lock_aspect": false, "width": 20, "height": 4}),
    );
    let decal = &decals(&doc)[0];
    assert_eq!(decal["surface"], "cone");
    assert_eq!(decal["mesh"]["cut"], false);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "width": 100}),
    );
    assert!(error.contains("more than once around it"), "{error}");
    // Moved down past the face's low end (z = 0): cut there.
    command(
        &mut doc,
        json!({"cmd": "edit_decal", "name": "Decal1", "center": [0, 1]}),
    );
    let decal = &decals(&doc)[0];
    assert_eq!(decal["mesh"]["cut"], true);
    // (The circle's points 11.25 degrees apart: chords of it within 0.01
    // mm.)
    assert!(triangle_points(decal).iter().all(|p| p[2] >= -0.02));
}

/// A decal's corners as points.
fn corners_of(decal: &Value) -> Vec<[f64; 3]> {
    decal["corners"]
        .as_array()
        .unwrap_or_else(|| panic!("{decal}"))
        .iter()
        .map(point)
        .collect()
}

fn same_corners(decal: &Value, expected: &[[f64; 3]]) {
    assert!(decal.get("error").is_none(), "{decal}");
    let actual = corners_of(decal);
    assert_eq!(actual.len(), expected.len(), "{decal}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((0..3).all(|i| (a[i] - e[i]).abs() < 1e-6), "{decal}: {e:?}");
    }
}

/// mitcad#2527: a decal on a face of a body of an occurrence, its path
/// from the root, follows the occurrence to another parent (Reparent
/// Occurrence) and its body into a new component (Move into Component),
/// and lies where it lay.
#[test]
fn a_decal_follows_its_occurrence_to_another_parent_and_into_a_component() {
    let mut doc = Document::new(MockKernel::default());
    // Arm, 50 up, with a 20 x 20 x 20 block (F2.b0).
    let created = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Arm", "transform": {"translation": [0, 0, 50]}}),
    );
    let arm = created["occurrence"].as_str().unwrap().to_owned();
    let arm_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    // Cell, 100 along X.
    let cell = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Cell", "transform": {"translation": [100, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top(), "occurrence": arm,
               "center": [10, 10], "width": 10}),
    );
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(arm), "{decal}");
    // On the block's top at z = 20 in Arm, 70 in the design.
    let placed = corners_of(&decal);
    assert!(placed.iter().all(|p| (p[2] - 70.0).abs() < 1e-6), "{decal}");

    // Reparent Occurrence: Arm into Cell, where it is in the design.
    let result = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": arm, "parent": cell}),
    );
    let updated = result["updated"].as_array().unwrap();
    assert!(updated.contains(&json!("Decal1")), "{result}");
    let decal = decals(&doc)[0].clone();
    let through_cell = format!("{cell}/{arm}");
    assert_eq!(decal["occurrence"], json!(through_cell), "{decal}");
    same_corners(&decal, &placed);

    // Move into Component: Arm's features go into a new component placed
    // in Arm; the decal's path goes on through its occurrence.
    command(
        &mut doc,
        json!({"cmd": "move_to_component", "component": arm_component}),
    );
    let decal = decals(&doc)[0].clone();
    let path = decal["occurrence"]
        .as_str()
        .unwrap_or_else(|| panic!("{decal}"));
    assert!(path.starts_with(&format!("{through_cell}/")), "{decal}");
    assert_eq!(path.split('/').count(), 3, "{decal}");
    same_corners(&decal, &placed);

    // Each is undone with its move.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(decals(&doc)[0]["occurrence"], json!(through_cell));
    command(&mut doc, json!({"cmd": "undo"}));
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(arm));
    same_corners(&decal, &placed);
}

/// mitcad#2555: a decal on a face of a body whose features go into an
/// existing component (Move into Component with an explicit destination)
/// takes the destination's path from the root, still finds its face and
/// lies where it lay; Undo gives it its path back. The destination is
/// placed elsewhere, so the body is rebuilt in its coordinates, where the
/// top's coordinates start from another origin: the decal's centre is
/// rewritten (100 mm along the face's x axis here).
#[test]
fn a_decal_follows_its_body_into_an_existing_component() {
    let mut doc = Document::new(MockKernel::default());
    // Arm, 50 up, with a 20 x 20 x 20 block (F2.b0).
    let created = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Arm", "transform": {"translation": [0, 0, 50]}}),
    );
    let arm = created["occurrence"].as_str().unwrap().to_owned();
    let arm_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    // Bin, an existing empty component 100 along X.
    let created = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bin", "transform": {"translation": [100, 0, 0]}}),
    );
    let bin = created["occurrence"].as_str().unwrap().to_owned();
    let bin_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top(), "occurrence": arm,
               "center": [10, 10], "width": 10}),
    );
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(arm), "{decal}");
    let placed = corners_of(&decal);
    assert!(placed.iter().all(|p| (p[2] - 70.0).abs() < 1e-6), "{decal}");

    // Arm's features go into Bin, the body staying where it is in the
    // design.
    let result = command(
        &mut doc,
        json!({"cmd": "move_to_component", "component": arm_component,
               "features": ["F1", "F2"], "source_occurrence": arm,
               "destination": {"kind": "existing", "component": bin_component, "occurrence": bin}}),
    );
    assert_eq!(result["components"][0]["created"], false, "{result}");
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(bin), "{decal}");
    same_corners(&decal, &placed);

    command(&mut doc, json!({"cmd": "undo"}));
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(arm), "{decal}");
    same_corners(&decal, &placed);
    assert_eq!(decal["center"], json!([10.0, 10.0]), "{decal}");
}

/// mitcad#2555: into an existing component turned a quarter turn about Z,
/// a decal on the moved body's top still lies where it lay, the same way
/// round: the destination's x axis is the design's Y, so the top's
/// coordinates are turned too, and the decal's angle is rewritten with its
/// centre. Undo gives back both.
#[test]
fn a_decal_keeps_its_place_and_turn_in_a_turned_destination() {
    let mut doc = Document::new(MockKernel::default());
    // Arm, 50 up, with a 20 x 20 x 20 block (F2.b0).
    let created = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Arm", "transform": {"translation": [0, 0, 50]}}),
    );
    let arm = created["occurrence"].as_str().unwrap().to_owned();
    let arm_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    // Bin, 100 along X and turned a quarter turn about Z (its X along the
    // design's Y).
    let created = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bin",
               "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]}),
    );
    let bin = created["occurrence"].as_str().unwrap().to_owned();
    let bin_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "insert_decal", "data": data(), "face": top(), "occurrence": arm,
               "center": [10, 10], "width": 10}),
    );
    let decal = decals(&doc)[0].clone();
    let placed = corners_of(&decal);
    // Its top edge (top left to top right) along the design's X.
    assert!(placed[1][0] - placed[0][0] > 9.0, "{decal}");

    command(
        &mut doc,
        json!({"cmd": "move_to_component", "component": arm_component,
               "features": ["F1", "F2"], "source_occurrence": arm,
               "destination": {"kind": "existing", "component": bin_component, "occurrence": bin}}),
    );
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(bin), "{decal}");
    same_corners(&decal, &placed);
    // The design's X is the top's -y in Bin: the decal turned -90 degrees
    // there.
    near(&decal["angle"], -90.0);

    command(&mut doc, json!({"cmd": "undo"}));
    let decal = decals(&doc)[0].clone();
    assert_eq!(decal["occurrence"], json!(arm), "{decal}");
    same_corners(&decal, &placed);
    near(&decal["angle"], 0.0);
}
