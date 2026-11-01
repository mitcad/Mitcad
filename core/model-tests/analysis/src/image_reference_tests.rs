// SPDX-License-Identifier: MIT
//! Image references (mitcad#782): an image inserted on a plane or a face
//! with its bytes embedded, calibrated by two pixels or two points and a
//! distance (about the first point), turned, flipped, made see-through,
//! hidden, renamed and deleted as undo steps, following its face, and
//! saved in the design file and read back the same.

use serde_json::{Value, json};

use mitcad_model::Document;
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

fn images(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "image_references"}))
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

fn near_points(actual: &Value, expected: &[[f64; 3]]) {
    let points = actual.as_array().unwrap_or_else(|| panic!("{actual}"));
    assert!(points.len() >= expected.len(), "{actual}");
    for (point, expected) in points.iter().zip(expected) {
        for (i, expected) in expected.iter().enumerate() {
            let value = point[i].as_f64().unwrap();
            assert!((value - expected).abs() < 1e-6, "{actual}");
        }
    }
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

fn boxed(kind: &[u8], body: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(8 + body.len())
        .unwrap()
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(body);
    bytes
}

/// A full box: version 0, no flags.
fn full(kind: &[u8], body: &[u8]) -> Vec<u8> {
    boxed(kind, &[&[0u8; 4][..], body].concat())
}

/// A HEIF file (mitcad#850) of one HEVC image, `width` x `height` pixels
/// coded, a quarter turn anticlockwise (`irot`) when `turned`, with `bits`
/// per channel. Its 16 bytes of image data are no HEVC: the model reads
/// only the header (`image_header`'s tests read more kinds; the
/// application's `app.unit` decodes real ones).
fn heif(width: u32, height: u32, turned: bool, bits: u8) -> Vec<u8> {
    let mut bytes = boxed(b"ftyp", b"heic\0\0\0\0mif1heic");
    let mut hdlr = vec![0; 4];
    hdlr.extend_from_slice(b"pict");
    hdlr.extend_from_slice(&[0; 13]);
    let mut infe = vec![2, 0, 0, 0, 0, 1, 0, 0];
    infe.extend_from_slice(b"hvc1\0");
    let iinf = full(b"iinf", &[&[0, 1][..], &boxed(b"infe", &infe)[..]].concat());
    let mut hvcc = vec![
        1, 1, 0x60, 0, 0, 0, 0x90, 0, 0, 0, 0, 0, 90, 0xf0, 0, 0xfc, 0xfd,
    ];
    hvcc.extend_from_slice(&[0xf8 | (bits - 8), 0xf8 | (bits - 8)]);
    let mut ispe = width.to_be_bytes().to_vec();
    ispe.extend_from_slice(&height.to_be_bytes());
    let mut ipco = boxed(b"hvcC", &hvcc);
    ipco.extend(full(b"ispe", &ispe));
    let mut ipma = vec![0, 0, 0, 1, 0, 1];
    if turned {
        ipco.extend(boxed(b"irot", &[1]));
        ipma.extend_from_slice(&[3, 0x81, 2, 0x83]);
    } else {
        ipma.extend_from_slice(&[2, 0x81, 2]);
    }
    let mut iprp = boxed(b"ipco", &ipco);
    iprp.extend(full(b"ipma", &ipma));
    let meta = [
        full(b"hdlr", &hdlr),
        full(b"pitm", &[0, 1]),
        iinf,
        boxed(b"iprp", &iprp),
    ]
    .concat();
    bytes.extend(full(b"meta", &meta));
    bytes.extend(boxed(b"mdat", &[0; 16]));
    bytes
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

#[test]
fn an_image_is_inserted_calibrated_and_changed_as_undo_steps() {
    let mut doc = block();
    assert!(images(&doc).is_empty());
    let inserted = command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": data(), "file_name": "plan.bmp",
               "plane": "xy", "at": [10, 10, 7]}),
    );
    assert_eq!(inserted["name"], "Image1");
    assert_eq!(inserted["recomputed"], 0, "an image changes no geometry");
    assert_eq!(undo_label(&doc), "Insert Image1");
    let image = &images(&doc)[0];
    assert_eq!(image["file_name"], "plan.bmp");
    assert_eq!(image["format"], "bmp");
    assert_eq!(image["pixels"], json!([200, 100]));
    assert_eq!(image["visible"], true);
    assert_eq!(image["through"], false);
    assert!(
        image.get("image").is_none(),
        "the list leaves the bytes out"
    );
    // The file's 4000 pixels per metre; centred on the point, projected.
    near(&image["scale"], 0.25);
    near(&image["size"][0], 50.0);
    near(&image["size"][1], 25.0);
    near(&image["opacity"], 0.5);
    assert_eq!(image["center"], json!([10.0, 10.0]));
    near_points(
        &image["corners"],
        &[
            [-15.0, 22.5, 0.0],
            [35.0, 22.5, 0.0],
            [35.0, -2.5, 0.0],
            [-15.0, -2.5, 0.0],
        ],
    );
    let file = query(
        &doc,
        json!({"query": "image_reference_image", "name": "Image1"}),
    );
    assert_eq!(file["data"], data());
    assert_eq!(file["sha256"], image["sha256"]);

    // Two pixels on its middle row, 100 mm apart: it doubles about the
    // first, which stays where it was.
    let calibrated = command(
        &mut doc,
        json!({"cmd": "calibrate_image_reference", "name": "Image1",
               "pixels": [[0, 50], [200, 50]], "distance": 100}),
    );
    near(&calibrated["scale"], 0.5);
    assert_eq!(undo_label(&doc), "Calibrate Image1");
    let image = &images(&doc)[0];
    near(&image["size"][0], 100.0);
    near(&image["center"][0], 35.0);
    near(&image["center"][1], 10.0);
    near_points(&image["corners"], &[[-15.0, 35.0, 0.0]]);

    // Two points of the design, off the plane: projected onto it first.
    command(&mut doc, json!({"cmd": "undo"}));
    command(
        &mut doc,
        json!({"cmd": "calibrate_image_reference", "name": "Image1",
               "points": [[-15, 10, 5], [35, 10, -3]], "distance": 25}),
    );
    let image = &images(&doc)[0];
    near(&image["scale"], 0.125);
    near(&image["center"][0], -2.5);

    // Turned a quarter, flipped, a third opaque, drawn through the bodies.
    command(
        &mut doc,
        json!({"cmd": "edit_image_reference", "name": "Image1", "center": [0, 0],
               "angle": 90, "flip": true, "opacity": 0.3, "through": true, "width": 50}),
    );
    assert_eq!(undo_label(&doc), "Edit Image1");
    let image = &images(&doc)[0];
    near(&image["scale"], 0.25);
    near(&image["opacity"], 0.3);
    assert_eq!(image["through"], true);
    // The image's right is -y, its up -x: its top left corner at (-12.5, 25).
    near_points(&image["corners"], &[[-12.5, 25.0, 0.0]]);
    // Nothing changed: no undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_image_reference", "name": "Image1", "opacity": 0.3}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    near(&images(&doc)[0]["opacity"], 0.5);

    command(&mut doc, json!({"cmd": "redo"}));
    command(
        &mut doc,
        json!({"cmd": "set_image_reference_visible", "name": "Image1", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Image1");
    assert_eq!(images(&doc)[0]["visible"], false);
    command(
        &mut doc,
        json!({"cmd": "rename_image_reference", "name": "Image1", "new_name": "Plan"}),
    );
    assert_eq!(undo_label(&doc), "Rename Image1");
    // The next default name is free again.
    let second = command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": data(), "plane": "yz"}),
    );
    assert_eq!(second["name"], "Image1");
    command(
        &mut doc,
        json!({"cmd": "delete_image_reference", "name": "Plan"}),
    );
    assert_eq!(undo_label(&doc), "Delete Plan");
    assert_eq!(images(&doc).len(), 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(images(&doc)[0]["name"], "Plan");
}

#[test]
fn an_image_follows_its_face_and_reads_its_file() {
    let mut doc = block();
    let dir = mitcad_model::testing::unique_temp_dir("image");
    let path = dir.join("mitcad-image-face.bmp");
    std::fs::write(&path, bmp(200, 100)).unwrap();
    let info = query(&doc, json!({"query": "image_info", "path": path}));
    assert_eq!(info["format"], "bmp");
    assert_eq!(info["pixels"], json!([200, 100]));
    near(&info["pixel_size"], 0.25);
    assert_eq!(info["stated"], true);
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "path": path, "plane": top,
               "at": [10, 10, 20], "scale": 0.1, "opacity": 1}),
    );
    mitcad_model::testing::remove_temp(&dir);
    let image = &images(&doc)[0];
    let file_name = image["file_name"].as_str().unwrap();
    assert!(file_name.starts_with("mitcad-image-"), "{file_name}");
    near(&image["scale"], 0.1);
    near_points(&image["corners"], &[[0.0, 15.0, 20.0], [20.0, 15.0, 20.0]]);
    // The block made higher: the image stays on its top.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 30}),
    );
    near_points(&images(&doc)[0]["corners"], &[[0.0, 15.0, 30.0]]);

    // Where a new one would lie: the panel's outline.
    let outline = query(
        &doc,
        json!({"query": "image_reference_outline", "plane": top, "pixels": [200, 100],
               "at": [10, 10, 30], "width": 40}),
    );
    near_points(&outline["corners"], &[[-10.0, 20.0, 30.0]]);
    // The point it is put on, in the plane's frame: the panel's X and Y.
    near(&outline["center"][0], 10.0);
    near(&outline["center"][1], 10.0);
}

#[test]
fn images_refuse_what_they_cannot_keep() {
    let mut doc = block();
    let insert = |extra: Value| {
        let mut command = json!({"cmd": "insert_image_reference", "data": data(), "plane": "xy"});
        for (key, value) in extra.as_object().unwrap() {
            command[key] = value.clone();
        }
        command
    };
    let error = refused(&mut doc, insert(json!({"plane": "x"})));
    assert!(error.contains("the image's plane"), "{error}");
    let gif = mitcad_model::internal::base64::encode(b"GIF89a");
    let error = refused(&mut doc, insert(json!({"data": gif})));
    assert!(error.contains("no PNG, JPEG, BMP, WebP or HEIF"), "{error}");
    let error = refused(&mut doc, insert(json!({"path": "a.png"})));
    assert!(error.contains("not both"), "{error}");
    let error = refused(&mut doc, insert(json!({"scale": 0.1, "width": 20})));
    assert!(error.contains("not both"), "{error}");
    let error = refused(&mut doc, insert(json!({"scale": 0})));
    assert!(error.contains("greater than zero"), "{error}");
    let error = refused(&mut doc, insert(json!({"opacity": 1.5})));
    assert!(error.contains("from 0 to 1"), "{error}");
    let error = refused(&mut doc, insert(json!({"depth": 3})));
    assert!(error.contains("depth"), "{error}");
    command(&mut doc, insert(json!({"name": "Plan"})));
    let error = refused(&mut doc, insert(json!({"name": "Plan"})));
    assert_eq!(error, "an image reference Plan exists");
    let error = refused(
        &mut doc,
        json!({"cmd": "calibrate_image_reference", "name": "Plan",
               "pixels": [[3, 4], [3, 4]], "distance": 10}),
    );
    assert!(error.contains("the same point"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "calibrate_image_reference", "name": "Plan",
               "pixels": [[0, 0], [10, 0]], "distance": -1}),
    );
    assert!(error.contains("greater than zero"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_image_reference", "name": "Gone", "opacity": 0.2}),
    );
    assert_eq!(error, "there is no image reference Gone");
}

#[test]
fn images_are_saved_with_their_bytes_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": data(), "file_name": "plan.bmp",
               "plane": "xz", "center": [5, 6], "angle": 30, "flip": true, "opacity": 0.75}),
    );
    command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": data(), "plane": "xy", "through": true}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_image_reference_visible", "name": "Image2", "visible": false}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let stored = &file["image_references"];
    assert_eq!(stored[0]["name"], "Image1");
    assert_eq!(stored[0]["image"]["format"], "bmp");
    assert_eq!(stored[0]["image"]["width"], 200);
    assert_eq!(stored[0]["image"]["data"], data());
    assert!(stored[0].get("visible").is_none(), "shown is the default");
    assert_eq!(stored[1]["visible"], false);

    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(images(&loaded), images(&doc));

    // Older files without images load as before.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(images(&old).is_empty());
    // Bytes that are not the image the file says, or a name twice.
    let wrong = saved.replacen("\"width\": 200,", "\"width\": 201,", 1);
    let Err(error) = Document::from_json(&wrong, MockKernel::default()) else {
        panic!("a wrong size was read");
    };
    assert!(
        error.to_string().contains("not the 201 × 100 bmp"),
        "{error}"
    );
    let twice = saved.replace("\"Image2\"", "\"Image1\"");
    let Err(error) = Document::from_json(&twice, MockKernel::default()) else {
        panic!("a name twice was read");
    };
    assert!(error.to_string().contains("more than once"), "{error}");
}

#[test]
fn heif_images_are_inserted_upright_saved_and_read_back() {
    let mut doc = block();
    // A photo coded 4032 x 3024 and turned a quarter: shown 3024 x 4032.
    let data = mitcad_model::internal::base64::encode(&heif(4032, 3024, true, 10));
    let inserted = command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": data, "file_name": "photo.heic",
               "plane": "xy", "width": 75.6}),
    );
    assert_eq!(inserted["name"], "Image1");
    let image = &images(&doc)[0];
    assert_eq!(image["format"], "heif");
    assert_eq!(image["file_name"], "photo.heic");
    assert_eq!(image["pixels"], json!([3024, 4032]));
    near(&image["scale"], 0.025);
    near(&image["size"][0], 75.6);
    near(&image["size"][1], 100.8);
    // The file's bytes as they were, for the application to decode.
    let file = query(
        &doc,
        json!({"query": "image_reference_image", "name": "Image1"}),
    );
    assert_eq!(file["data"], data);

    let saved = doc.to_json();
    let stored: Value = serde_json::from_str(&saved).unwrap();
    let stored = &stored["image_references"][0]["image"];
    assert_eq!(stored["format"], "heif");
    assert_eq!(stored["width"], 3024);
    assert_eq!(stored["height"], 4032);
    assert_eq!(stored["data"], data);
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(images(&loaded), images(&doc));
    // The stored size checked against the bytes as for the other formats.
    let wrong = saved.replacen("\"width\": 3024,", "\"width\": 4032,", 1);
    let Err(error) = Document::from_json(&wrong, MockKernel::default()) else {
        panic!("a wrong size was read");
    };
    assert!(
        error.to_string().contains("not the 4032 × 4032 heif"),
        "{error}"
    );

    // A file's header for the panel: no pixel size stated, 96 to the inch.
    let dir = mitcad_model::testing::unique_temp_dir("image");
    let path = dir.join("mitcad-image-header.heic");
    std::fs::write(&path, heif(64, 32, false, 8)).unwrap();
    let info = query(&doc, json!({"query": "image_info", "path": path}));
    mitcad_model::testing::remove_temp(&dir);
    assert_eq!(info["format"], "heif");
    assert_eq!(info["pixels"], json!([64, 32]));
    assert_eq!(info["stated"], false);
    near(&info["pixel_size"], 25.4 / 96.0);
}

#[test]
fn heif_images_it_cannot_keep_are_refused() {
    let mut doc = block();
    let insert = |bytes: &[u8]| {
        json!({"cmd": "insert_image_reference", "plane": "xy",
               "data": mitcad_model::internal::base64::encode(bytes)})
    };
    let error = refused(&mut doc, insert(&heif(64, 32, false, 12)));
    assert!(error.contains("12 bits per channel"), "{error}");
    let error = refused(&mut doc, insert(&heif(40_000, 32, false, 8)));
    assert!(error.contains("more than 32768"), "{error}");
    let file = heif(64, 32, false, 8);
    let error = refused(&mut doc, insert(&file[..file.len() - 4]));
    assert!(error.contains("ends inside"), "{error}");
    // More than a design embeds.
    let mut large = heif(64, 32, false, 8);
    large.extend(boxed(b"free", &vec![0; 32 << 20]));
    let error = refused(&mut doc, insert(&large));
    assert!(error.contains("more than the 32 MB"), "{error}");
    assert!(images(&doc).is_empty());
}
