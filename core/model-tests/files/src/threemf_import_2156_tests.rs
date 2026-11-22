// SPDX-License-Identifier: MIT
//! 3MF files imported as mesh bodies and components (mitcad#2156), with
//! the mock kernel: files written by `mitcad_3mf::write`, one with an open
//! mesh, and files that are no 3MF package.

use serde_json::{Value, json};

use mitcad_3mf::{Mesh, Model, Part, Placement};
use mitcad_model::Document;
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

/// `data` written as `name` into a folder of its own (the tests run on
/// several threads of one process).
fn written(data: &[u8], name: &str) -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mitcad-3mf-import-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, data).unwrap();
    path.to_string_lossy().into_owned()
}

/// A cube of side `size` from the origin, its triangles facing outwards.
fn cube(size: f64) -> Mesh {
    let vertices = (0..8)
        .map(|i| {
            [
                size * f64::from(i & 1),
                size * f64::from((i >> 1) & 1),
                size * f64::from((i >> 2) & 1),
            ]
        })
        .collect();
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    Mesh {
        vertices,
        triangles: quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect(),
    }
}

fn at(translation: [f64; 3]) -> Placement {
    Placement {
        translation,
        ..Placement::IDENTITY
    }
}

/// A quarter turn about z, then `translation`.
fn turned(translation: [f64; 3]) -> Placement {
    Placement {
        linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        translation,
    }
}

/// Two coloured parts: a red plate placed once, a blue pin placed twice
/// (once turned).
fn plate_and_pins() -> Model {
    Model {
        name: "Fixture".to_owned(),
        parts: vec![
            Part {
                name: "Plate".to_owned(),
                color: Some([1.0, 0.0, 0.0]),
                mesh: cube(10.0),
                placements: vec![at([0.0, 0.0, 0.0])],
                metadata: Vec::new(),
            },
            Part {
                name: "Pin".to_owned(),
                color: Some([0.0, 0.0, 1.0]),
                mesh: cube(2.0),
                placements: vec![at([2.5, 3.0, 10.0]), turned([7.5, 3.0, 10.0])],
                metadata: Vec::new(),
            },
        ],
        ..Model::default()
    }
}

fn body_named<'a>(bodies: &'a Value, name: &str) -> &'a Value {
    bodies
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == name)
        .unwrap_or_else(|| panic!("a body {name} in {bodies}"))
}

#[test]
fn a_3mf_file_comes_in_as_mesh_bodies_with_appearances_and_a_repeated_part_as_a_component() {
    let path = written(
        &mitcad_3mf::write(&plate_and_pins()).unwrap(),
        "fixture.3mf",
    );
    let mut doc = Document::new(MockKernel::default());
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["structure"], "components", "{result}");
    // The plate a body of the design, the pin a component placed twice.
    let names: Vec<&str> = result["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Plate", "Pin"], "{result}");
    assert!(
        result["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["kind"] == "mesh"),
        "{result}"
    );
    let components = result["components"].as_array().unwrap();
    assert_eq!(components.len(), 1, "{result}");
    assert_eq!(components[0]["name"], "Pin");
    assert_eq!(components[0]["occurrences"].as_array().unwrap().len(), 2);
    assert_eq!(components[0]["bodies"], 1);
    assert_eq!(result["occurrences"], 2);
    assert!(result.get("warnings").is_none(), "{result}");
    // Not wrapped: a mesh file goes into the active component.
    assert!(result.get("component").is_none(), "{result}");

    // The occurrences where the file places the pin, within 1e-6 mm.
    let pin = doc
        .assembly()
        .components
        .iter()
        .find(|c| c.name == "Pin")
        .map(|c| c.uid)
        .unwrap();
    let placements: Vec<_> = doc
        .assembly()
        .occurrences_of(pin)
        .map(|o| o.transform)
        .collect();
    assert_eq!(placements.len(), 2);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let wanted = [at([2.5, 3.0, 10.0]), turned([7.5, 3.0, 10.0])];
    for (placement, wanted) in placements.iter().zip(wanted) {
        for r in 0..3 {
            assert!(
                close(placement.translation[r], wanted.translation[r]),
                "{placement:?}"
            );
            for c in 0..3 {
                assert!(
                    close(placement.linear[r][c], wanted.linear[r][c]),
                    "{placement:?}"
                );
            }
        }
    }
    // The pin's body in its component, the plate's in the design's root.
    let bodies = doc.bodies();
    let pin_body = bodies.iter().find(|b| b.name == "Pin").unwrap().uid;
    assert_eq!(doc.body_component(pin_body), Some(pin));

    // The base materials as appearances named after them, with their
    // colours; the bodies keep the colours too.
    let bodies = query(&doc, json!({"query": "bodies"}));
    let appearances = query(&doc, json!({"query": "appearances"}));
    let look = |body: &str| {
        let id = body_named(&bodies, body)["appearance"]
            .as_str()
            .unwrap_or_else(|| panic!("{body} has an appearance: {bodies}"))
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
    assert_eq!(red["name"], "Plate", "{red}");
    assert_eq!(red["base_color"], json!([1.0, 0.0, 0.0]), "{red}");
    let blue = look("Pin");
    assert_eq!(blue["name"], "Pin", "{blue}");
    assert_eq!(blue["base_color"], json!([0.0, 0.0, 1.0]), "{blue}");

    // One undo step takes it all back.
    assert_eq!(doc.undo_label(), Some("Import fixture.3mf"));
    doc.undo();
    assert!(doc.bodies().is_empty());
    assert!(doc.assembly().components.is_empty());
    assert!(doc.custom_appearances().is_empty());
}

#[test]
fn a_3mf_part_placed_once_is_one_base_feature_named_by_the_import() {
    let mut model = plate_and_pins();
    model.parts[1].placements.truncate(1);
    let path = written(&mitcad_3mf::write(&model).unwrap(), "flat.3MF");
    let mut doc = Document::new(MockKernel::default());
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": path, "name": "Fixture parts"}),
    )
    .unwrap();
    assert!(result.get("structure").is_none(), "{result}");
    assert_eq!(result["name"], "Fixture parts");
    assert_eq!(result["bodies"].as_array().unwrap().len(), 2);
    assert!(doc.assembly().components.is_empty());
}

/// The model part of a package as text, and the package with it replaced.
fn with_model(data: &[u8], change: impl Fn(&str) -> String) -> Vec<u8> {
    let archive = mitcad_zip::Archive::new(data.to_vec()).unwrap();
    let mut zip = mitcad_zip::Writer::new();
    for entry in archive.entries() {
        let mut bytes = archive.read(entry).unwrap();
        if entry.name == "3D/3dmodel.model" {
            bytes = change(&String::from_utf8(bytes).unwrap()).into_bytes();
        }
        zip.add(&entry.name, &bytes, mitcad_zip::Method::Stored)
            .unwrap();
    }
    zip.finish().unwrap()
}

#[test]
fn an_open_mesh_comes_in_as_a_mesh_body_with_a_warning() {
    let model = Model {
        parts: vec![Part {
            name: "Shell".to_owned(),
            color: None,
            mesh: cube(10.0),
            placements: vec![Placement::IDENTITY],
            metadata: Vec::new(),
        }],
        ..Model::default()
    };
    // The top's first triangle left out: an opening.
    let data = with_model(&mitcad_3mf::write(&model).unwrap(), |xml| {
        xml.replacen("<triangle v1=\"4\" v2=\"5\" v3=\"7\"/>", "", 1)
    });
    assert!(mitcad_3mf::read(&data).is_err());
    let path = written(&data, "shell.3mf");
    let mut doc = Document::new(MockKernel::default());
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["bodies"][0]["name"], "Shell", "{result}");
    assert_eq!(result["bodies"][0]["kind"], "mesh", "{result}");
    let warnings = result["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{result}");
    let warning = warnings[0].as_str().unwrap();
    assert!(
        warning.starts_with("Shell: its mesh is not closed"),
        "{warning}"
    );
    assert!(
        warning.contains("Convert Mesh cannot make a solid"),
        "{warning}"
    );
}

#[test]
fn a_file_that_is_no_3mf_package_is_refused_in_words() {
    let mut doc = Document::new(MockKernel::default());
    let stl = written(b"solid cube\nendsolid cube\n", "cube.3mf");
    let error = command(&mut doc, json!({"cmd": "import_file", "path": stl})).unwrap_err();
    assert!(
        error.contains("cube.3mf cannot be imported as a 3MF file: not a zip package"),
        "{error}"
    );
    // A required extension the reader does not read.
    let data = with_model(&mitcad_3mf::write(&plate_and_pins()).unwrap(), |xml| {
        xml.replacen(
            "<model ",
            "<model xmlns:s=\"http://schemas.microsoft.com/3dmanufacturing/slice/2015/07\" \
             requiredextensions=\"s\" ",
            1,
        )
    });
    let sliced = written(&data, "sliced.3mf");
    let error = command(&mut doc, json!({"cmd": "import_file", "path": sliced})).unwrap_err();
    assert!(
        error.contains("requires the 3MF Slice extension, which MitCad does not read"),
        "{error}"
    );
    assert!(doc.bodies().is_empty());
}
