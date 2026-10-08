// SPDX-License-Identifier: MIT
//! Appearances (mitcad#46): the library's physically based parameters, the
//! document's own appearances created, edited, assigned and deleted as
//! undo steps, saved in the project file, and older files.

use serde_json::{Value, json};

use crate::Document;
use crate::diff::diff_states;
use crate::testing::MockKernel;

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

fn query(doc: &Document<MockKernel>, query: &str) -> Value {
    serde_json::from_str(&doc.query(&json!({"query": query}).to_string()).unwrap()).unwrap()
}

fn appearance(doc: &Document<MockKernel>, id: &str) -> Option<Value> {
    query(doc, "appearances")
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
}

/// The ids of the document's own appearances.
fn custom(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, "appearances")
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["library"] == false)
        .map(|a| a["id"].as_str().unwrap().to_owned())
        .collect()
}

fn body_appearance(doc: &Document<MockKernel>) -> Value {
    query(doc, "bodies")[0]["appearance"].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, "document")["undo"].clone()
}

/// A 20 x 20 x 20 block (F2.b0) on a rectangle on XY.
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

#[test]
fn the_library_lists_physically_based_parameters() {
    let doc = block();
    let all = query(&doc, "appearances");
    let all = all.as_array().unwrap();
    assert!(all.len() >= 20, "{}", all.len());
    assert!(all.iter().all(|a| a["library"] == true));
    // The first ones are those the application listed before, in order.
    let first: Vec<&str> = all[..3].iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert_eq!(
        first,
        ["steel_satin", "aluminum_anodized", "brass_polished"]
    );
    let red = appearance(&doc, "paint_red").unwrap();
    let c = |v: u8| f64::from(v) / 255.0;
    assert_eq!(red["base_color"], json!([c(200), c(40), c(40)]));
    assert_eq!(red["display_color"], red["base_color"]);
    assert_eq!(red["metalness"], 0.0);
    assert_eq!(red["name"], "Paint - Red");
    assert_eq!(red["bodies"], json!([]));
    let chrome = appearance(&doc, "chrome").unwrap();
    assert_eq!(chrome["metalness"], 1.0);
    assert!(chrome["roughness"].as_f64().unwrap() < 0.1);
    let glass = appearance(&doc, "glass_clear").unwrap();
    assert_eq!(glass["transmission"], 1.0);
    assert_eq!(glass["ior"], 1.5);
    for a in all {
        for field in [
            "metalness",
            "roughness",
            "specular",
            "transmission",
            "ior",
            "coat",
            "coat_roughness",
            "emission",
            "emission_color",
            "opacity",
        ] {
            assert!(a.get(field).is_some(), "{} has no {field}", a["id"]);
        }
    }
}

#[test]
fn appearances_are_created_edited_assigned_deleted_and_undone() {
    let mut doc = block();
    let created = command(
        &mut doc,
        json!({"cmd": "create_appearance", "based_on": "chrome", "name": "Dull Chrome",
               "roughness": 0.3}),
    );
    assert_eq!(created["id"], "custom1");
    assert_eq!(undo_label(&doc), "Create Appearance Dull Chrome");
    let made = appearance(&doc, "custom1").unwrap();
    let chrome = appearance(&doc, "chrome").unwrap();
    assert_eq!(made["library"], false);
    assert_eq!(made["metalness"], 1.0);
    assert_eq!(made["roughness"], 0.3);
    assert_eq!(made["base_color"], chrome["base_color"]);

    // Without a base: the default look, the next free id and name.
    let second = command(&mut doc, json!({"cmd": "create_appearance"}));
    assert_eq!(second["id"], "custom2");
    let plain = appearance(&doc, "custom2").unwrap();
    assert_eq!(plain["name"], "Appearance2");
    assert_eq!(plain["base_color"], json!([0.8, 0.8, 0.8]));
    assert_eq!(plain["roughness"], 0.5);
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "my_red", "name": "My Red",
               "base_color": [0.9, 0.1, 0.1], "coat": 1.0}),
    );
    assert_eq!(custom(&doc), ["custom1", "custom2", "my_red"]);

    // Edits change what is given; an edit that changes nothing is no step.
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "my_red", "roughness": 0.15,
               "emission": 2.0, "emission_color": [1.0, 0.5, 0.0], "opacity": 0.5}),
    );
    assert_eq!(undo_label(&doc), "Edit Appearance My Red");
    let red = appearance(&doc, "my_red").unwrap();
    assert_eq!(red["roughness"], 0.15);
    assert_eq!(red["coat"], 1.0);
    assert_eq!(red["emission"], 2.0);
    assert_eq!(red["opacity"], 0.5);
    let revision = query(&doc, "document")["revision"].clone();
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "my_red", "roughness": 0.15}),
    );
    assert_eq!(query(&doc, "document")["revision"], revision);
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "my_red", "name": "Signal Red"}),
    );
    assert_eq!(undo_label(&doc), "Edit Appearance My Red");
    assert_eq!(appearance(&doc, "my_red").unwrap()["name"], "Signal Red");

    // Assigned to the body; the query says which bodies use it.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "my_red"}),
    );
    assert_eq!(body_appearance(&doc), "my_red");
    assert_eq!(
        appearance(&doc, "my_red").unwrap()["bodies"],
        json!(["F2.b0"])
    );

    // Deleting it gives the body the default look in the same step.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_appearance", "id": "my_red"}),
    );
    assert_eq!(deleted["bodies"], json!(["F2.b0"]));
    assert_eq!(undo_label(&doc), "Delete Appearance Signal Red");
    assert!(appearance(&doc, "my_red").is_none());
    assert_eq!(body_appearance(&doc), Value::Null);

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(body_appearance(&doc), "my_red");
    assert_eq!(appearance(&doc, "my_red").unwrap()["name"], "Signal Red");
    command(&mut doc, json!({"cmd": "undo"})); // the assignment
    command(&mut doc, json!({"cmd": "undo"})); // the rename
    assert_eq!(appearance(&doc, "my_red").unwrap()["name"], "My Red");
    command(&mut doc, json!({"cmd": "undo"})); // the edit
    assert_eq!(appearance(&doc, "my_red").unwrap()["roughness"], 0.5);
    command(&mut doc, json!({"cmd": "undo"})); // the creation
    assert_eq!(custom(&doc), ["custom1", "custom2"]);
    for _ in 0..5 {
        command(&mut doc, json!({"cmd": "redo"}));
    }
    assert!(appearance(&doc, "my_red").is_none());
    assert_eq!(body_appearance(&doc), Value::Null);
    assert_eq!(custom(&doc), ["custom1", "custom2"]);

    // A library appearance is assigned the same way.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "plastic_red"}),
    );
    assert_eq!(
        appearance(&doc, "plastic_red").unwrap()["bodies"],
        json!(["F2.b0"])
    );
}

#[test]
fn what_cannot_be_an_appearance_is_refused() {
    let mut doc = block();
    let cases = [
        (
            json!({"cmd": "edit_appearance", "id": "chrome", "roughness": 0.5}),
            "library appearance",
        ),
        (
            json!({"cmd": "delete_appearance", "id": "paint_red"}),
            "library appearance",
        ),
        (
            json!({"cmd": "create_appearance", "id": "chrome"}),
            "an appearance 'chrome' exists",
        ),
        (
            json!({"cmd": "create_appearance", "name": "Chrome"}),
            "an appearance is named 'Chrome'",
        ),
        (
            json!({"cmd": "create_appearance", "id": "my red"}),
            "no appearance id",
        ),
        (
            json!({"cmd": "create_appearance", "based_on": "unobtainium"}),
            "there is no appearance 'unobtainium'",
        ),
        (
            json!({"cmd": "create_appearance", "roughness": 1.5}),
            "roughness must be between 0 and 1",
        ),
        (
            json!({"cmd": "create_appearance", "ior": 0.9}),
            "ior must be between 1 and 5",
        ),
        (
            json!({"cmd": "create_appearance", "base_color": [0.5, 2.0, 0.0]}),
            "base_color",
        ),
        (
            json!({"cmd": "create_appearance", "name": " "}),
            "needs a name",
        ),
        (
            json!({"cmd": "create_appearance", "shininess": 0.5}),
            "shininess",
        ),
        (
            json!({"cmd": "edit_appearance", "id": "custom9", "roughness": 0.5}),
            "there is no appearance 'custom9'",
        ),
        (
            json!({"cmd": "edit_appearance", "roughness": 0.5}),
            "needs the appearance's id",
        ),
    ];
    for (cmd, expected) in cases {
        let error = refused(&mut doc, cmd.clone());
        assert!(error.contains(expected), "{cmd}: {error}");
    }
    command(&mut doc, json!({"cmd": "create_appearance"}));
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "custom1", "based_on": "chrome"}),
    );
    assert!(error.contains("only for create_appearance"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "custom1", "metalness": -0.1}),
    );
    assert!(error.contains("metalness"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "create_appearance", "name": "Appearance1"}),
    );
    assert!(error.contains("named 'Appearance1'"), "{error}");
    assert_eq!(custom(&doc), ["custom1"]);
}

#[test]
fn appearances_are_saved_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    assert!(!plain.contains("\"appearances\""), "none: left out");
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "oak_board", "name": "Oak Board",
               "based_on": "wood_oak", "texture": {"path": "textures/oak.png", "size": [200, 50],
               "rotation": 0.5, "projection": "planar"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "based_on": "glass_clear", "base_color": [0.8, 1.0, 0.9]}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "oak_board"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let oak = &file["appearances"][0];
    assert_eq!(oak["id"], "oak_board");
    assert_eq!(
        oak["texture"],
        json!({"path": "textures/oak.png", "size": [200.0, 50.0], "rotation": 0.5,
               "projection": "planar"})
    );
    assert_eq!(file["appearances"][1]["transmission"], 1.0);
    assert!(file["appearances"][1].get("texture").is_none());
    assert_eq!(file["bodies"][0]["appearance"], "oak_board");

    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(query(&loaded, "appearances"), query(&doc, "appearances"));
    assert_eq!(body_appearance(&loaded), "oak_board");

    // A texture is cleared with null.
    command(
        &mut loaded,
        json!({"cmd": "edit_appearance", "id": "oak_board", "texture": null}),
    );
    assert!(
        appearance(&loaded, "oak_board")
            .unwrap()
            .get("texture")
            .is_none()
    );

    // Older files without appearances, and bodies that name an appearance
    // in neither list (an import's), load as before.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(custom(&old).is_empty());
    let foreign = saved.replace(
        "\"appearance\": \"oak_board\"",
        "\"appearance\": \"imported_look\"",
    );
    let mut loaded = Document::from_json(&foreign, MockKernel::default()).unwrap();
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(body_appearance(&loaded), "imported_look");

    // Parameters left out of a file are the default's.
    let mut file: Value = serde_json::from_str(&saved).unwrap();
    file["appearances"] = json!([{"id": "short", "name": "Short", "metalness": 1.0}]);
    let loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    let short = appearance(&loaded, "short").unwrap();
    assert_eq!(short["metalness"], 1.0);
    assert_eq!(short["roughness"], 0.5);
    assert_eq!(short["ior"], 1.5);

    // The comparison of versions lists them.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.appearances.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Oak Board (oak_board): added",
            "Appearance1 (custom1): added"
        ]
    );
    assert!(diff.summary.contains("+2 appearances"), "{}", diff.summary);

    // Broken entries are refused with where they are.
    for (broken, expected) in [
        (
            json!([{"id": "chrome", "name": "Mine"}]),
            "appearances[0]: 'chrome' is a library appearance's id",
        ),
        (
            json!([{"id": "a", "name": "A"}, {"id": "a", "name": "B"}]),
            "appearances[1]: appearance 'a' is listed more than once",
        ),
        (
            json!([{"id": "a", "name": "A", "roughness": 2.0}]),
            "appearances[0]: roughness",
        ),
        (
            json!([{"id": "a", "name": "A", "gloss": 2.0}]),
            "appearances[0]",
        ),
        (json!([{"id": "a b", "name": "A"}]), "appearances[0]"),
    ] {
        file["appearances"] = broken;
        let Err(error) = Document::from_json(&file.to_string(), MockKernel::default()) else {
            panic!("{} was read", file["appearances"]);
        };
        assert!(error.to_string().contains(expected), "{error}");
    }
}

// Appearances of single faces (mitcad#53).

const SIDE: &str = "F2:side(c1[c4,c2])";

fn face_appearances(doc: &Document<MockKernel>) -> Value {
    query(doc, "bodies")[0]
        .get("face_appearances")
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn faces_get_appearances_of_their_own_as_undo_steps() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "steel_satin"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
               "appearance": "paint_red"}),
    );
    assert_eq!(undo_label(&doc), "Set Appearance of Face of Body1");
    assert_eq!(
        face_appearances(&doc),
        json!([{"face": SIDE, "appearance": "paint_red", "faces": [SIDE]}])
    );
    // The body keeps its own appearance for its other faces.
    assert_eq!(body_appearance(&doc), "steel_satin");
    assert_eq!(
        appearance(&doc, "paint_red").unwrap()["faces"],
        json!([{"body": "F2.b0", "face": SIDE}])
    );
    // The same again is no undo step; two faces are one.
    let depth = query(&doc, "document")["undo_depth"].clone();
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
               "appearance": "paint_red"}),
    );
    assert_eq!(query(&doc, "document")["undo_depth"], depth);
    let end = format!("F2:end({R})");
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE, end],
               "appearance": "paint_blue"}),
    );
    assert_eq!(undo_label(&doc), "Set Appearance of 2 Faces of Body1");
    let faces = face_appearances(&doc);
    assert_eq!(faces.as_array().unwrap().len(), 2);
    assert!(
        faces
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["appearance"] == "paint_blue")
    );

    // Undo and redo.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        face_appearances(&doc),
        json!([{"face": SIDE, "appearance": "paint_red", "faces": [SIDE]}])
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(face_appearances(&doc), Value::Null);
    command(&mut doc, json!({"cmd": "redo"}));
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(face_appearances(&doc).as_array().unwrap().len(), 2);

    // Clearing one face, then all.
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE], "appearance": null}),
    );
    assert_eq!(undo_label(&doc), "Clear Appearance of Face of Body1");
    assert_eq!(face_appearances(&doc)[0]["face"], end);
    command(
        &mut doc,
        json!({"cmd": "clear_face_appearances", "uid": "F2.b0"}),
    );
    assert_eq!(undo_label(&doc), "Clear Face Appearances of Body1");
    assert_eq!(face_appearances(&doc), Value::Null);
    assert_eq!(body_appearance(&doc), "steel_satin");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(face_appearances(&doc)[0]["face"], end);

    // Faces that are not there, or are no face names, are refused.
    for (faces, expected) in [
        (
            json!(["F2:side(c9[c1,c2])"]),
            "Body1 has no face F2:side(c9[c1,c2])",
        ),
        (json!(["nonsense"]), "'nonsense' is no face name"),
        (json!([]), "no faces are given"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": faces,
                   "appearance": "paint_red"}),
        );
        assert!(error.contains(expected), "{error}");
    }
    let error = refused(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F9.b0", "faces": [SIDE],
               "appearance": "paint_red"}),
    );
    assert!(error.contains("does not exist"), "{error}");

    // Deleting an appearance a face uses gives the face its body's.
    let id = command(
        &mut doc,
        json!({"cmd": "create_appearance", "based_on": "paint_green"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE], "appearance": id}),
    );
    let deleted = command(&mut doc, json!({"cmd": "delete_appearance", "id": id}));
    assert_eq!(deleted["faces"], json!([{"body": "F2.b0", "face": SIDE}]));
    assert_eq!(face_appearances(&doc)[0]["face"], end);
    assert_eq!(face_appearances(&doc).as_array().unwrap().len(), 1);
}

#[test]
fn face_appearances_follow_faces_a_sketch_edit_renames() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
               "appearance": "paint_red"}),
    );
    // A fillet between c1 and c2 in the sketch: c1's side face now ends on
    // the fillet's arc, so its name changes.
    command(
        &mut doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": "c1", "b": "c2", "radius": 3}),
    );
    let faces = face_appearances(&doc);
    assert_eq!(faces[0]["face"], SIDE, "the assigned name is kept");
    let now = faces[0]["faces"].as_array().unwrap();
    assert_eq!(now.len(), 1, "{faces}");
    let renamed = now[0].as_str().unwrap();
    assert_ne!(renamed, SIDE);
    assert!(renamed.starts_with("F2:side(c1["), "{renamed}");
    // Undoing the fillet finds the face by its own name again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(face_appearances(&doc)[0]["faces"], json!([SIDE]));
    // A face that is gone finds nothing (the body keeps the override).
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["c1"]}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(face_appearances(&doc)[0]["faces"], json!([SIDE]));
}

#[test]
fn face_appearances_are_saved_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
               "appearance": "rubber"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["bodies"][0]["face_appearances"],
        json!({SIDE: "rubber"})
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(face_appearances(&loaded), face_appearances(&doc));
    // Older files have none.
    let mut old = Document::from_json(&plain, MockKernel::default()).unwrap();
    command(&mut old, json!({"cmd": "recompute"}));
    assert_eq!(face_appearances(&old), Value::Null);
    assert!(!plain.contains("face_appearances"));
    // A name that is no face name is refused.
    let broken = saved.replace(SIDE, "F2:");
    let Err(error) = Document::from_json(&broken, MockKernel::default()) else {
        panic!("a broken face name was read");
    };
    assert!(error.to_string().contains("no face name"), "{error}");
    // The comparison of versions lists the body's change.
    let diff = diff_states(&before, doc.state());
    assert_eq!(diff.bodies.len(), 1, "{:?}", diff.bodies);
}

/// The first bytes of a PNG file and some more: what the model checks of
/// an embedded image.
fn png_bytes() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R', 1, 2, 3]);
    bytes
}

#[test]
fn texture_images_are_embedded_kept_and_saved() {
    let mut doc = block();
    let data = crate::base64::encode(&png_bytes());
    let sha = crate::sha256::Sha256::of(&png_bytes()).to_string();
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "checker", "name": "Checker",
               "texture": {"path": "checker.png", "size": [20, 20], "data": data}}),
    );
    let listed = appearance(&doc, "checker").unwrap();
    assert_eq!(
        listed["texture"],
        json!({"path": "checker.png", "size": [20.0, 20.0], "rotation": 0.0,
               "projection": "box", "embedded": true, "image_sha256": sha})
    );
    let image = serde_json::from_str::<Value>(
        &doc.query(&json!({"query": "appearance_image", "id": "checker"}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(image, json!({"data": data, "format": "png", "sha256": sha}));

    // The listed form keeps the image: a change of the size alone.
    let mut texture = listed["texture"].clone();
    texture["size"] = json!([40, 10]);
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "checker", "texture": texture}),
    );
    let listed = appearance(&doc, "checker").unwrap();
    assert_eq!(listed["texture"]["size"], json!([40.0, 10.0]));
    assert_eq!(listed["texture"]["image_sha256"], sha);
    // A copy keeps it too.
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "checker2", "name": "Checker 2",
               "based_on": "checker"}),
    );
    assert_eq!(
        appearance(&doc, "checker2").unwrap()["texture"]["image_sha256"],
        sha
    );

    // Saved in the project file, and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["appearances"][0]["texture"]["data"], data);
    assert!(file["appearances"][0]["texture"].get("embedded").is_none());
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);

    // Without the data and `embedded` the texture is the file again.
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "checker",
               "texture": {"path": "checker.png", "size": [40, 10]}}),
    );
    let listed = appearance(&doc, "checker").unwrap();
    assert!(listed["texture"].get("embedded").is_none(), "{listed}");
    assert!(
        doc.query(&json!({"query": "appearance_image", "id": "checker"}).to_string())
            .is_err()
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "checker",
               "texture": {"path": "c.png", "embedded": true}}),
    );
    assert!(error.contains("no embedded image to keep"), "{error}");
    // What is no PNG or JPEG image is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "checker",
               "texture": {"path": "c.png", "data": crate::base64::encode(b"GIF89a")}}),
    );
    assert!(error.contains("no PNG or JPEG"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "checker",
               "texture": {"path": "c.png", "data": "not base64!"}}),
    );
    assert!(error.contains("no base64"), "{error}");
}
