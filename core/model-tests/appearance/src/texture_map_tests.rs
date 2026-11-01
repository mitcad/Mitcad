// SPDX-License-Identifier: MIT
//! Texture map sets (mitcad#313): an appearance's base colour image with
//! normal, roughness, metalness and occlusion maps, embedded in the design
//! file once per image by its digest, with where the set came from.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::appearance::{ImageData, MAX_EMBEDDED_IMAGE, Texture};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_diff::diff_states;

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

fn query(doc: &Document<MockKernel>, query: Value) -> Result<Value, String> {
    doc.query(&query.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn listed(doc: &Document<MockKernel>, id: &str) -> Value {
    query(doc, json!({"query": "appearances"}))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no appearance {id}"))
}

fn texture_of<'a>(doc: &'a Document<MockKernel>, id: &str) -> &'a Texture {
    doc.custom_appearances()
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.texture.as_ref())
        .unwrap_or_else(|| panic!("{id} has no texture"))
}

/// A small PNG file's first bytes, told apart by `tag`: what the model
/// checks of an embedded image.
fn png(tag: u8) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R', tag, 2, 3]);
    bytes
}

fn base64(bytes: &[u8]) -> String {
    mitcad_model::internal::base64::encode(bytes)
}

fn sha(bytes: &[u8]) -> String {
    mitcad_model::internal::sha256::Sha256::of(bytes).to_string()
}

const MAPS: [(&str, u8); 5] = [
    ("base_color", 1),
    ("normal", 2),
    ("roughness", 3),
    ("metalness", 4),
    ("occlusion", 5),
];

/// A texture with all maps embedded and a source.
fn full_set() -> Value {
    json!({
        "path": "oak_diff_2k.png", "size": [300, 300], "rotation": 0.25, "projection": "box",
        "data": base64(&png(1)),
        "normal": {"path": "oak_nor_gl_2k.png", "data": base64(&png(2))},
        "roughness": {"path": "oak_rough_2k.png", "data": base64(&png(3))},
        "metalness": {"path": "oak_metal_2k.png", "data": base64(&png(4))},
        "occlusion": {"path": "oak_ao_2k.png", "data": base64(&png(5))},
        "source": {"id": "polyhaven", "asset": "oak_veneer_01", "resolution": "2k",
                   "license": "CC0-1.0", "url": "https://polyhaven.com/a/oak_veneer_01"}
    })
}

#[test]
fn a_full_map_set_is_created_edited_saved_loaded_and_undone() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "oak", "name": "Oak", "texture": full_set()}),
    );
    // Listed with every map embedded by digest, the source as given.
    let texture = listed(&doc, "oak")["texture"].clone();
    assert_eq!(texture["embedded"], true);
    assert_eq!(texture["image_sha256"], sha(&png(1)));
    for (map, tag) in &MAPS[1..] {
        assert_eq!(
            texture[map],
            json!({"path": format!("oak_{}_2k.png", match *map {
                       "normal" => "nor_gl", "roughness" => "rough",
                       "metalness" => "metal", _ => "ao" }),
                   "embedded": true, "image_sha256": sha(&png(*tag))}),
            "{map}"
        );
    }
    assert_eq!(
        texture["source"],
        json!({"id": "polyhaven", "asset": "oak_veneer_01", "resolution": "2k",
               "license": "CC0-1.0", "url": "https://polyhaven.com/a/oak_veneer_01"})
    );
    // `appearance_image` gives each map's image.
    for (map, tag) in MAPS {
        let image = query(
            &doc,
            json!({"query": "appearance_image", "id": "oak", "map": map}),
        )
        .unwrap();
        assert_eq!(
            image,
            json!({"data": base64(&png(tag)), "format": "png", "sha256": sha(&png(tag))}),
            "{map}"
        );
    }
    assert_eq!(
        query(&doc, json!({"query": "appearance_image", "id": "oak"})).unwrap()["sha256"],
        sha(&png(1))
    );
    let error = query(
        &doc,
        json!({"query": "appearance_image", "id": "oak", "map": "bump"}),
    )
    .unwrap_err();
    assert!(error.contains("“bump” is no map"), "{error}");

    // The listed form sent back keeps every image: a change of the size.
    let before_edit = doc.state().clone();
    let mut edited = texture.clone();
    edited["size"] = json!([600, 600]);
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "oak", "texture": edited}),
    );
    let after = listed(&doc, "oak")["texture"].clone();
    assert_eq!(after["size"], json!([600.0, 600.0]));
    for (map, _) in &MAPS[1..] {
        assert_eq!(after[map], texture[map], "{map}");
    }
    // The images are the same copies, not new ones.
    let old = before_edit.appearances[0].texture.as_ref().unwrap();
    for (map, data) in texture_of(&doc, "oak").images() {
        assert!(data.same(old.image_data(map).unwrap()), "{map}");
    }
    // A map is removed by leaving it out; undo brings it back.
    let mut fewer = after.clone();
    fewer.as_object_mut().unwrap().remove("occlusion");
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "oak", "texture": fewer}),
    );
    assert!(texture_of(&doc, "oak").occlusion.is_none());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(listed(&doc, "oak")["texture"], after);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(listed(&doc, "oak")["texture"], texture);
    command(&mut doc, json!({"cmd": "redo"}));

    // Saved: the maps refer to the file's `images` by digest.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let images = file["images"].as_array().unwrap();
    assert_eq!(
        images
            .iter()
            .map(|i| i["sha256"].clone())
            .collect::<Vec<_>>(),
        MAPS.map(|(_, tag)| json!(sha(&png(tag))))
    );
    assert_eq!(images[1]["data"], base64(&png(2)));
    let saved_texture = &file["appearances"][0]["texture"];
    assert_eq!(saved_texture["image"], sha(&png(1)));
    assert!(saved_texture.get("data").is_none());
    assert_eq!(
        saved_texture["normal"],
        json!({"path": "oak_nor_gl_2k.png", "image": sha(&png(2))})
    );
    assert_eq!(saved_texture["source"]["license"], "CC0-1.0");
    // Read back as it was, and written the same.
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(listed(&loaded, "oak"), listed(&doc, "oak"));

    // The comparison of versions shows each map's image by its digest.
    let before_normal = doc.state().clone();
    let mut new_normal = listed(&doc, "oak")["texture"].clone();
    new_normal["normal"] = json!({"path": "n.png", "data": base64(&png(6))});
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "oak", "texture": new_normal}),
    );
    let diff = format!("{:?}", diff_states(&before_normal, doc.state()).appearances);
    assert!(
        diff.contains(&format!("sha256 {}", sha(&png(2))))
            && diff.contains(&format!("sha256 {}", sha(&png(6))))
            && !diff.contains(&base64(&png(6))),
        "{diff}"
    );
}

#[test]
fn one_image_is_kept_once_however_many_textures_use_it() {
    let mut doc = Document::new(MockKernel::default());
    let wood = base64(&png(7));
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "a", "name": "A",
               "texture": {"path": "wood.png", "data": wood}}),
    );
    // The same image sent again is shared, not copied.
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "b", "name": "B",
               "texture": {"path": "other/wood.png", "size": [50, 50], "data": wood}}),
    );
    assert!(
        texture_of(&doc, "a")
            .data
            .as_ref()
            .unwrap()
            .same(texture_of(&doc, "b").data.as_ref().unwrap())
    );
    // By digest: another appearance's image, also as a map.
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "c", "name": "C",
               "texture": {"path": "c.png", "data": base64(&png(8)),
                           "roughness": {"path": "wood.png", "embedded": true,
                                         "image_sha256": sha(&png(7))}}}),
    );
    assert!(
        texture_of(&doc, "c")
            .roughness
            .as_ref()
            .unwrap()
            .data
            .as_ref()
            .unwrap()
            .same(texture_of(&doc, "a").data.as_ref().unwrap())
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "c",
               "texture": {"path": "c.png", "normal": {"path": "n.png", "embedded": true,
                           "image_sha256": sha(b"nothing")}}}),
    );
    assert!(
        error.contains("texture: normal: the design embeds no image with the SHA-256"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "c",
               "texture": {"path": "c.png", "normal": {"path": "n.png", "embedded": true}}}),
    );
    assert!(
        error.contains("texture: normal: there is no embedded image to keep"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": "c",
               "texture": {"path": "c.png", "image": sha(&png(7))}}),
    );
    assert!(error.contains("for design files only"), "{error}");

    // The design file keeps the shared image once.
    let saved = doc.to_json();
    assert_eq!(saved.matches(&wood).count(), 1, "{saved}");
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["images"].as_array().unwrap().len(), 2);
    assert_eq!(file["appearances"][1]["texture"]["image"], sha(&png(7)));
    // Read back, the textures share one copy again.
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert!(
        texture_of(&loaded, "b").data.as_ref().unwrap().same(
            texture_of(&loaded, "c")
                .roughness
                .as_ref()
                .unwrap()
                .data
                .as_ref()
                .unwrap()
        )
    );
    assert_eq!(loaded.to_json(), saved);

    // Undo keeps sharing: the earlier states hold the same copies.
    let shared = texture_of(&doc, "a").data.clone().unwrap();
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(texture_of(&doc, "a").data.as_ref().unwrap().same(&shared));
    command(&mut doc, json!({"cmd": "redo"}));
    assert!(texture_of(&doc, "b").data.as_ref().unwrap().same(&shared));

    // An older file that embeds one image twice inline loads with one copy
    // and is written with `images` from then on.
    let mut old: Value = serde_json::from_str(&saved).unwrap();
    old.as_object_mut().unwrap().remove("images");
    old["appearances"] = json!([
        {"id": "a", "name": "A", "texture": {"path": "wood.png", "data": wood}},
        {"id": "b", "name": "B", "texture": {"path": "wood.png", "data": wood}},
    ]);
    let loaded = Document::from_json(&old.to_string(), MockKernel::default()).unwrap();
    assert!(
        texture_of(&loaded, "a")
            .data
            .as_ref()
            .unwrap()
            .same(texture_of(&loaded, "b").data.as_ref().unwrap())
    );
    assert_eq!(loaded.to_json().matches(&wood).count(), 1);
}

#[test]
fn a_file_with_one_texture_is_written_as_before() {
    let mut doc = Document::new(MockKernel::default());
    let data = base64(&png(1));
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "checker", "name": "Checker",
               "texture": {"path": "checker.png", "size": [20, 20], "data": data}}),
    );
    // A file with one embedded texture, written as before map sets: read,
    // it is written the same.
    let old = doc.to_json();
    let loaded = Document::from_json(&old, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), old);
    // Maps given as files, and a source, need no `images` either.
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "tiles", "name": "Tiles",
               "texture": {"path": "tiles.png", "normal": {"path": "tiles_normal.png"},
                           "source": {"id": "folder", "asset": "tiles"}}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(file.get("images").is_none(), "{saved}");
    assert_eq!(
        file["appearances"][0]["texture"],
        json!({"path": "checker.png", "size": [20.0, 20.0], "rotation": 0.0,
               "projection": "box", "data": data})
    );
    assert_eq!(
        file["appearances"][1]["texture"]["normal"],
        json!({"path": "tiles_normal.png"})
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
}

#[test]
fn broken_images_of_a_file_are_refused() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "create_appearance", "id": "oak", "name": "Oak", "texture": full_set()}),
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let read = |change: &dyn Fn(&mut Value)| {
        let mut broken = file.clone();
        change(&mut broken);
        match Document::from_json(&broken.to_string(), MockKernel::default()) {
            Ok(_) => panic!("{broken} was read"),
            Err(e) => e.to_string(),
        }
    };
    let error = read(&|f| {
        f["images"].as_array_mut().unwrap().remove(1);
    });
    assert!(
        error.contains("appearances[0]: texture: normal: image")
            && error.contains("is not in `images`"),
        "{error}"
    );
    let error = read(&|f| f["images"][2]["data"] = json!(base64(&png(9))));
    assert!(
        error.contains("images[2]: the image's SHA-256 is"),
        "{error}"
    );
    let error = read(&|f| {
        let first = f["images"][0].clone();
        f["images"].as_array_mut().unwrap().push(first);
    });
    assert!(error.contains("images[5]: image"), "{error}");
    assert!(error.contains("listed more than once"), "{error}");
    let error = read(&|f| f["images"][0]["data"] = json!(base64(b"GIF89a")));
    assert!(
        error.contains("images[0]: the embedded image is no PNG or JPEG"),
        "{error}"
    );
    let error = read(&|f| f["appearances"][0]["texture"]["data"] = json!(base64(&png(1))));
    assert!(error.contains("both `data` and `image`"), "{error}");
    let error = read(&|f| f["images"][0]["format"] = json!("png"));
    assert!(error.contains("images[0]"), "{error}");
    let error = read(&|f| f["appearances"][0]["texture"]["source"]["url"] = json!("file:///x"));
    assert!(error.contains("http or https"), "{error}");
}

#[test]
fn what_cannot_be_a_map_set_is_refused() {
    let mut doc = Document::new(MockKernel::default());
    for (texture, expected) in [
        (
            json!({"path": "a.png", "normal": {"path": " "}}),
            "texture: normal: the image path is empty",
        ),
        (
            json!({"path": "a.png", "roughness": {"path": "r.png", "data": base64(b"GIF89a")}}),
            "texture: roughness: the embedded image is no PNG or JPEG",
        ),
        (
            json!({"path": "a.png", "bump": {"path": "b.png"}}),
            "unknown field `bump`",
        ),
        (
            json!({"path": "a.png", "source": {"id": "polyhaven"}}),
            "missing field `asset`",
        ),
        (
            json!({"path": "a.png", "source": {"id": "poly haven", "asset": "x"}}),
            "is no source id",
        ),
        (
            json!({"path": "a.png", "source": {"id": "folder", "asset": "x", "license": ""}}),
            "the license is empty",
        ),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "create_appearance", "name": "A", "texture": texture}),
        );
        assert!(error.contains(expected), "{texture}: {error}");
    }
}

#[test]
fn the_size_limit_holds_per_image() {
    let large = |megabytes: usize| {
        let mut bytes = png(1);
        bytes.resize(megabytes << 20, 0);
        ImageData::from_bytes(&bytes)
    };
    let twenty = large(20);
    let mut texture = Texture {
        path: "base.png".to_owned(),
        data: Some(twenty.clone()),
        ..Texture::default()
    };
    texture.roughness = Some(mitcad_model::appearance::TextureMap {
        path: "rough.png".to_owned(),
        data: Some(large(21)),
        ..Default::default()
    });
    let appearance = mitcad_model::appearance::Appearance {
        name: "Large".to_owned(),
        texture: Some(texture),
        ..Default::default()
    };
    // 41 MB in all, each image under the limit.
    appearance.check().unwrap();
    let mut too_large = appearance.clone();
    too_large.texture.as_mut().unwrap().normal = Some(mitcad_model::appearance::TextureMap {
        path: "normal.png".to_owned(),
        data: Some(large((MAX_EMBEDDED_IMAGE >> 20) + 1)),
        ..Default::default()
    });
    let error = too_large.check().unwrap_err();
    assert!(
        error.contains("texture: normal: the embedded image is 33 MB, more than the 32 MB"),
        "{error}"
    );
}
