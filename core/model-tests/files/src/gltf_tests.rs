// SPDX-License-Identifier: MIT
//! glTF files imported as mesh bodies and components (mitcad#1826), with
//! the mock kernel: files of the reader's test writer and the committed
//! public-domain sample.

use std::path::PathBuf;

use serde_json::{Value, json};

use mitcad_gltf::testfile::{self, Form, TestNode, TestScene};
use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// `scene` written as `name` into a folder of its own: the tests run on
/// several threads of one process, and two of them writing `crate.glb` into
/// one folder read each other's half-written file.
fn written(scene: &TestScene, name: &str, form: &Form) -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mitcad-gltf-import-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (bytes, files) = testfile::write(scene, form);
    for (file, data) in files {
        std::fs::write(dir.join(file), data).unwrap();
    }
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

fn component_named(doc: &Document<MockKernel>, name: &str) -> Option<ComponentUid> {
    doc.assembly()
        .components
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.uid)
}

fn body_named<'a>(bodies: &'a Value, name: &str) -> &'a Value {
    bodies
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == name)
        .unwrap_or_else(|| panic!("a body {name} in {bodies}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_node_tree_comes_in_as_components_with_mesh_bodies_and_appearances() {
    for (name, form) in [
        ("assembly.glb", Form::Glb),
        ("assembly.gltf", Form::External("assembly.bin".into())),
    ] {
        let path = written(&testfile::assembly(), name, &form);
        let mut doc = Document::new(MockKernel::default());
        let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
        assert_eq!(result["structure"], "components", "{name}: {result}");
        // The two brackets are one component placed twice; the root node
        // is the design itself, not a component.
        let components = result["components"].as_array().unwrap();
        assert_eq!(components.len(), 1, "{name}: {result}");
        assert_eq!(components[0]["name"], "Bracket");
        assert_eq!(components[0]["occurrences"].as_array().unwrap().len(), 2);
        assert_eq!(components[0]["bodies"], 1);
        assert_eq!(result["occurrences"], 2);
        // The label and the scaled pin are bodies of the design: the
        // scaled node is no component.
        let names: Vec<&str> = result["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Label", "Pin", "Plate"], "{name}");
        let warnings = result["warnings"].as_array().unwrap();
        let said = |text: &str| warnings.iter().any(|w| w.as_str().unwrap().contains(text));
        // The label's box has an image but no texture coordinates.
        assert!(
            said("a textured mesh has no texture coordinates"),
            "{warnings:?}"
        );
        assert!(!said("texture coordinates are not read"), "{warnings:?}");
        assert!(
            said("not components: their meshes came in moved into place"),
            "{warnings:?}"
        );

        // The occurrences at 100 and 300 mm along x, turned from Y up to
        // Z up.
        let bracket = component_named(&doc, "Bracket").unwrap();
        let placements: Vec<_> = doc
            .assembly()
            .occurrences_of(bracket)
            .map(|o| o.transform)
            .collect();
        assert_eq!(placements.len(), 2);
        for (placement, x) in placements.iter().zip([100.0, 300.0]) {
            assert!(close(placement.translation[0], x), "{placement:?}");
            assert!(close(placement.linear[1][2], -1.0), "{placement:?}");
            assert!(close(placement.linear[2][1], 1.0), "{placement:?}");
        }

        // The bodies' triangles in millimetres are checked by the plan's
        // tests (core/model/src/exchange/gltf_import.rs): the mock kernel
        // keeps no geometry.
        assert_eq!(
            doc.body_component(doc.bodies().iter().find(|b| b.name == "Plate").unwrap().uid),
            Some(bracket)
        );

        // The materials as appearances: Red's sRGB colour and roughness,
        // the label's image embedded.
        let bodies = query(&doc, json!({"query": "bodies"}));
        let appearances = query(&doc, json!({"query": "appearances"}));
        let look = |body: &str| {
            let id = body_named(&bodies, body)["appearance"]
                .as_str()
                .unwrap()
                .to_owned();
            appearances
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .cloned()
                .unwrap()
        };
        let red = look("Plate");
        assert_eq!(red["name"], "Red");
        let r = red["base_color"][0].as_f64().unwrap();
        assert!((r - mitcad_gltf::srgb(0.8)).abs() < 1e-9, "{red}");
        assert!(close(red["roughness"].as_f64().unwrap(), 0.4));
        assert!(close(red["metalness"].as_f64().unwrap(), 0.0));
        assert_eq!(look("Pin")["id"], red["id"]);
        let textured = look("Label");
        assert_eq!(textured["name"], "Label");
        assert_eq!(textured["texture"]["embedded"], true, "{textured}");
        assert_eq!(textured["texture"]["path"], "image1.png");
        // Without texture coordinates its image is projected as MitCad
        // projects textures.
        assert_eq!(textured["texture"]["projection"], "box", "{textured}");
        assert_eq!(doc.kernel().count("textured_triangle_body"), 0);

        // One undo step takes it all back.
        assert_eq!(doc.undo_label(), Some(format!("Import {name}").as_str()));
        doc.undo();
        assert!(doc.bodies().is_empty());
        assert!(doc.assembly().components.is_empty());
        assert!(doc.custom_appearances().is_empty());
    }
}

#[test]
fn meshes_without_a_tree_are_one_base_feature() {
    let mut scene = testfile::assembly();
    scene.nodes = vec![
        TestNode::named("Plate", Some(0), &[]),
        TestNode {
            translation: Some([0.0, 0.1, 0.0]),
            ..TestNode::named("Label", Some(1), &[])
        },
    ];
    scene.roots = vec![0, 1];
    let path = written(&scene, "flat.glb", &Form::Glb);
    let mut doc = Document::new(MockKernel::default());
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": path, "name": "Scan"}),
    )
    .unwrap();
    assert!(result.get("structure").is_none(), "{result}");
    assert_eq!(result["name"], "Scan");
    assert_eq!(result["bodies"].as_array().unwrap().len(), 2);
    assert_eq!(result["bodies"][0]["kind"], "mesh");
    assert!(doc.assembly().components.is_empty());
}

#[test]
fn into_a_design_with_content_a_gltf_file_comes_in_its_own_component() {
    let path = written(&testfile::assembly(), "robot arm.glb", &Form::Glb);
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert!(doc.import_wraps_by_default(&path));
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["component"]["name"], "robot arm", "{result}");
    let wrapper = component_named(&doc, "robot arm").unwrap();
    // A second copy of the bracket's component inside it, its name made
    // unique.
    let bracket = component_named(&doc, "Bracket (1)").expect("the second bracket");
    assert!(
        doc.assembly()
            .occurrences_of(bracket)
            .all(|o| o.parent == wrapper)
    );
    // The answer of file_structure says so too.
    let answer = query(&doc, json!({"query": "file_structure", "path": path}));
    assert_eq!(
        answer,
        json!({"assembly": false, "content": true, "wrap": true})
    );
}

#[test]
fn a_new_design_with_its_empty_component1_takes_a_gltf_file_as_it_is() {
    // New Design's document (the app's Ctrl+N): Component1, empty and
    // active, is no content, so nothing is wrapped and nothing asked.
    let path = written(&testfile::assembly(), "robot new.glb", &Form::Glb);
    let mut doc = Document::new_design(MockKernel::default());
    assert!(!doc.has_content());
    assert!(!doc.import_wraps_by_default(&path));
    assert!(!doc.import_wraps_by_default("/models/bracket.step"));
    let answer = query(&doc, json!({"query": "file_structure", "path": path}));
    assert_eq!(
        answer,
        json!({"assembly": false, "content": false, "wrap": false})
    );
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert!(result.get("component").is_none(), "{result}");
    assert!(component_named(&doc, "robot new").is_none());
    // The file's components went into Component1, the active one; now the
    // design has content.
    let component1 = component_named(&doc, "Component1").unwrap();
    let bracket = component_named(&doc, "Bracket").expect("the file's bracket");
    assert!(
        doc.assembly()
            .occurrences_of(bracket)
            .all(|o| o.parent == component1)
    );
    assert!(doc.has_content());
    doc.undo();
    assert!(!doc.has_content());

    // Another component beside Component1 is content.
    doc.add_component(
        Some("Frame"),
        ComponentUid::ROOT,
        mitcad_model::transform::Transform::IDENTITY,
    )
    .unwrap();
    assert!(doc.has_content());
    assert!(doc.import_wraps_by_default(&path));
}

#[test]
fn a_file_without_triangles_is_refused_in_words() {
    let mut scene = testfile::assembly();
    for mesh in &mut scene.meshes {
        for p in &mut mesh.primitives {
            p.mode = Some(1);
        }
    }
    let path = written(&scene, "lines.glb", &Form::Glb);
    let mut doc = Document::new(MockKernel::default());
    let error = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap_err();
    assert!(
        error.contains("lines.glb has no triangles to import")
            && error.contains("points and lines are left out"),
        "{error}"
    );
    assert!(doc.bodies().is_empty());
    assert_eq!(doc.undo_depth(), 0);

    let path = written(&scene, "damaged.glb", &Form::Glb);
    std::fs::write(&path, b"glTF\x02\0\0\0").unwrap();
    let error = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap_err();
    assert!(
        error.contains("damaged.glb: the glTF binary file is damaged"),
        "{error}"
    );
}

#[test]
fn texture_coordinates_and_images_of_metalness_and_roughness_come_in() {
    let path = written(&testfile::textured_cube(), "crate.glb", &Form::Glb);
    let mut doc = Document::new(MockKernel::default());
    let file = query(&doc, json!({"query": "gltf_file", "path": path}));
    assert_eq!(file["notes"], json!([]), "{file}");
    assert_eq!(file["nodes"][0]["texture_coordinates"], true);
    assert_eq!(
        file["nodes"][0]["materials"][0]["metallic_roughness_texture"],
        true
    );

    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    // Nothing left out, nothing to say.
    assert!(result.get("warnings").is_none(), "{result}");
    // The body was made with its texture coordinates.
    assert_eq!(doc.kernel().count("textured_triangle_body"), 1);

    let bodies = query(&doc, json!({"query": "bodies"}));
    let appearances = query(&doc, json!({"query": "appearances"}));
    let id = body_named(&bodies, "Crate")["appearance"].clone();
    let wood = appearances
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap();
    assert_eq!(wood["name"], "Wood");
    let texture = &wood["texture"];
    // Projected by the body's own texture coordinates.
    assert_eq!(texture["projection"], "uv", "{texture}");
    assert_eq!(texture["path"], "image1.png");
    // One image of metalness (blue) and roughness (green), and the
    // occlusion its red channel.
    assert_eq!(texture["roughness"]["path"], "image2.png", "{texture}");
    assert_eq!(texture["roughness"]["channel"], "green", "{texture}");
    assert_eq!(texture["metalness"]["channel"], "blue", "{texture}");
    assert_eq!(texture["occlusion"]["channel"], "red", "{texture}");
    assert_eq!(texture["roughness"]["embedded"], true);
    assert_eq!(
        texture["roughness"]["image_sha256"],
        texture["metalness"]["image_sha256"]
    );
    assert!(texture["normal"].is_null(), "{texture}");
}

#[test]
fn images_of_a_material_without_a_base_colour_image_are_said_not_read() {
    // The cube without its base colour image: no texture set, so its
    // metallic-roughness image is not held, and no texture coordinates
    // are kept.
    let mut plain = testfile::textured_cube();
    plain.materials[0].image = None;
    let path = written(&plain, "plain.glb", &Form::Glb);
    let mut doc = Document::new(MockKernel::default());
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(doc.kernel().count("textured_triangle_body"), 0);
    assert_eq!(
        result["warnings"],
        json!([
            "images of a material without a base colour image are not read: the material keeps its values"
        ]),
        "{result}"
    );
    let appearances = query(&doc, json!({"query": "appearances"}));
    let wood = appearances
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "Wood")
        .unwrap();
    assert!(wood["texture"].is_null(), "{wood}");
}

/// The public-domain sample (core/gltf/samples, CC0).
fn sample() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../gltf/samples/two-blocks.gltf")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn the_public_domain_sample_comes_in_as_mesh_bodies_and_a_component() {
    let mut doc = Document::new(MockKernel::default());
    let file = query(&doc, json!({"query": "gltf_file", "path": sample()}));
    let paths: Vec<&str> = file["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["Stand", "Stand/Base", "Stand/Post", "Marker"]);
    assert_eq!(file["nodes"][1]["triangles"], 12);
    assert_eq!(file["notes"], json!([]));

    let result = command(&mut doc, json!({"cmd": "import_file", "path": sample()})).unwrap();
    assert_eq!(result["structure"], "components", "{result}");
    assert_eq!(result["components"][0]["name"], "Stand");
    assert_eq!(result["components"][0]["bodies"], 2);
    let names: Vec<&str> = result["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Marker", "Base", "Post"]);
    assert!(result.get("warnings").is_none(), "{result}");
    let bodies = query(&doc, json!({"query": "bodies"}));
    let appearances = query(&doc, json!({"query": "appearances"}));
    let id = body_named(&bodies, "Post")["appearance"].clone();
    let orange = appearances
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap();
    assert_eq!(orange["name"], "Orange");
    assert_eq!(body_named(&bodies, "Marker")["appearance"], id);
}

/// The appearance of the textured cube's material, as the `appearances`
/// query lists it, after `scene` is imported into a new document.
fn imported_wood(scene: &TestScene, name: &str) -> (Document<MockKernel>, Value) {
    let path = written(scene, name, &Form::Glb);
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    let appearances = query(&doc, json!({"query": "appearances"}));
    let wood = appearances
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "Wood")
        .unwrap()
        .clone();
    (doc, wood)
}

#[test]
fn metalness_and_roughness_factors_scale_their_maps() {
    // glTF multiplies the image's channels by the factors (mitcad#1874):
    // the cube's roughness 0.5 and metalness 0 go with their maps, which
    // the appearance's values do not scale.
    let (_, wood) = imported_wood(&testfile::textured_cube(), "factors.glb");
    let texture = &wood["texture"];
    assert_eq!(texture["roughness"]["factor"], 0.5, "{texture}");
    assert_eq!(texture["metalness"]["factor"], 0.0, "{texture}");
    assert!(texture["occlusion"].get("factor").is_none(), "{texture}");

    // Factors of 1 are not written: the maps are the values.
    let mut ones = testfile::textured_cube();
    ones.materials[0].metallic = 1.0;
    ones.materials[0].roughness = 1.0;
    let (_, wood) = imported_wood(&ones, "ones.glb");
    let texture = &wood["texture"];
    assert!(texture["roughness"].get("factor").is_none(), "{texture}");
    assert!(texture["metalness"].get("factor").is_none(), "{texture}");
    assert_eq!(texture["roughness"]["channel"], "green");

    // A factor is set and taken back by edit_appearance; out of 0 to 1 or
    // on the normal map it is refused in words.
    let (mut doc, wood) = imported_wood(&testfile::textured_cube(), "edit.glb");
    let id = wood["id"].as_str().unwrap().to_owned();
    let mut edited = wood["texture"].clone();
    edited["roughness"]["factor"] = json!(0.25);
    command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": id, "texture": edited}),
    )
    .unwrap();
    let listed = query(&doc, json!({"query": "appearances"}));
    let again = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id.as_str())
        .unwrap();
    assert_eq!(again["texture"]["roughness"]["factor"], 0.25, "{again}");
    let mut wrong = wood["texture"].clone();
    wrong["roughness"]["factor"] = json!(1.5);
    let error = command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": id, "texture": wrong}),
    )
    .unwrap_err();
    assert!(
        error.contains("texture: roughness: the factor must be from 0 to 1"),
        "{error}"
    );
    let mut normal = wood["texture"].clone();
    normal["normal"] = wood["texture"]["roughness"].clone();
    normal["normal"]["factor"] = json!(0.5);
    let error = command(
        &mut doc,
        json!({"cmd": "edit_appearance", "id": id, "texture": normal}),
    )
    .unwrap_err();
    assert!(
        error.contains("texture: normal: only the roughness and metalness maps take a factor"),
        "{error}"
    );
}

#[test]
fn usdz_export_writes_a_meshs_own_texture_coordinates() {
    // The cube imported with its texture coordinates (mitcad#1862): its
    // appearance's projection is uv, and the USDZ export writes the mesh's
    // own coordinates (mitcad#1874; the mock's are x and y across the
    // body's box, 0 to 1, on its one face's two triangles), not a box
    // projection of the image.
    use mitcad_usdz::sdf::Value as Usd;
    let (mut doc, wood) = imported_wood(&testfile::textured_cube(), "crate.glb");
    assert_eq!(wood["texture"]["projection"], "uv");
    let dir = mitcad_model::testing::unique_temp_dir("gltf-usdz");
    let path = dir.join("crate.usdz").to_str().unwrap().to_owned();
    let result = command(&mut doc, json!({"cmd": "export", "path": path})).unwrap();
    assert_eq!(result["format"], "usdz", "{result}");
    let package = mitcad_usdz::read_package(&std::fs::read(&path).unwrap()).unwrap();
    let layer = &package.layer;
    let mesh = layer
        .specs
        .iter()
        .filter(|s| s.spec_type == mitcad_usdz::sdf::SpecType::Prim)
        .map(|s| s.path.to_string())
        .find(|p| p.ends_with("/Crate"))
        .expect("the cube's mesh");
    let field = |name: &str| {
        layer
            .field(&format!("{mesh}.{name}"), "default")
            .unwrap_or_else(|| panic!("{mesh} has no {name}"))
            .clone()
    };
    let Usd::Vec3fArray(points) = field("points") else {
        panic!("points")
    };
    let Usd::IntArray(corners) = field("faceVertexIndices") else {
        panic!("faceVertexIndices")
    };
    let Usd::Vec2fArray(st) = field("primvars:st") else {
        panic!("primvars:st")
    };
    assert_eq!(st.len(), corners.len());
    assert!(!st.is_empty());
    // The box of the points written: the mock's face spans it in x and y.
    let low = |axis: usize| points.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min);
    let high = |axis: usize| {
        points
            .iter()
            .map(|p| p[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    };
    let across = |p: [f32; 3], axis: usize| (p[axis] - low(axis)) / (high(axis) - low(axis));
    for (k, uv) in st.iter().enumerate() {
        let p = points[corners[k] as usize];
        assert!(
            (uv[0] - across(p, 0)).abs() < 1e-5 && (uv[1] - across(p, 1)).abs() < 1e-5,
            "corner {k}: {uv:?} at {p:?}"
        );
    }
    // The coordinates differ across the cube: not all one corner's.
    assert!(st.iter().any(|uv| uv[0] > 0.5) && st.iter().any(|uv| uv[0] < 0.5));
}
