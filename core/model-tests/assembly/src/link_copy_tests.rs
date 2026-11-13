// SPDX-License-Identifier: MIT
//! A design linked from outside the project, copied into it (mitcad#1719,
//! GitHub mitcad/Mitcad#251) with the mock kernel. A link is followed only
//! to a file in the design's project (mitcad#929); Insert Component asks
//! the `link_check` query first and, for a file outside, inserts with
//! `copy_into_project`: the file and the designs it links outside too are
//! copied into the project's `parts/` folder (a suffix when the name is
//! taken), as single files with their B-rep data inside, and the copy is
//! linked by a path from the design's folder. Saved and opened again, the
//! link is followed (no "not followed" line), and Update Link takes a
//! change of the copy.

use std::path::Path;

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FileFormat, InsertOptions};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

/// A design of `blocks` blocks of 10 × 10 × 5, 20 mm apart.
fn design(blocks: usize) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for i in 0..blocks {
        let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = doc
            .add_rectangle(sketch, [20.0 * i as f64, 0.0], &num(10.0), &num(10.0))
            .unwrap()
            .region;
        doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
            .unwrap();
    }
    doc
}

/// The design file at `path` opened as the application opens it: read
/// with its project's store, its links followed from its folder.
fn open(path: &Path) -> (Document<MockKernel>, Vec<String>) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut doc = Document::from_json_at(&text, path, MockKernel::default()).unwrap();
    let messages = doc.update_links(path.parent()).unwrap();
    doc.recompute();
    (doc, messages)
}

/// The paths a design file's linked components name.
fn link_paths(path: &Path) -> Vec<String> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    value["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["link"]["path"].as_str().map(str::to_owned))
        .collect()
}

fn version(path: &Path) -> Value {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    value["version"].clone()
}

#[test]
fn a_design_outside_the_project_is_copied_in_and_its_link_followed() {
    let dir = mitcad_model::testing::unique_temp_path("1719-copy");
    // The assembly's project, with a design of its own already named
    // plate.mitcad in parts/.
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    let designs = project.join("designs");
    let parts = project.join("parts");
    std::fs::create_dir_all(&designs).unwrap();
    std::fs::create_dir_all(&parts).unwrap();
    let own_plate = design(1).to_json();
    std::fs::write(parts.join("plate.mitcad"), &own_plate).unwrap();
    // Outside it, in a project of their own (B-rep data in that project's
    // store): plate.mitcad, which links bolt.mitcad beside it.
    let other = dir.join("other");
    mitcad_model::Project::init(&other).unwrap();
    let bolt = other.join("bolt.mitcad");
    design(1).save_file(&bolt, FileFormat::Project).unwrap();
    let mut plate_doc = design(1);
    let options = InsertOptions {
        base: Some(other.clone()),
        ..InsertOptions::default()
    };
    plate_doc
        .insert_component(bolt.to_str().unwrap(), &options)
        .unwrap();
    let plate = other.join("plate.mitcad");
    plate_doc.save_file(&plate, FileFormat::Project).unwrap();
    assert_eq!(link_paths(&plate), ["bolt.mitcad"]);
    assert_eq!(version(&plate), json!(3));

    let base = designs.to_string_lossy().to_string();
    let mut holder = Document::new(MockKernel::default());
    // The query Insert Component asks first: not followed, and where the
    // copy would go.
    let check = query(
        &holder,
        json!({"query": "link_check", "path": plate.to_string_lossy(), "base": base}),
    );
    assert_eq!(check["followed"], json!(false), "{check}");
    assert_eq!(check["folder"], json!(parts.to_string_lossy()), "{check}");
    // A file in the project is followed.
    let inside = query(
        &holder,
        json!({"query": "link_check", "path": parts.join("plate.mitcad").to_string_lossy(),
               "base": base}),
    );
    assert_eq!(inside["followed"], json!(true), "{inside}");

    // A design never saved has no project to copy into.
    let refused = command(
        &mut holder,
        json!({"cmd": "insert_component", "path": plate.to_string_lossy(),
               "copy_into_project": true}),
    )
    .unwrap_err();
    assert!(refused.contains("not saved yet"), "{refused}");

    let answer = command(
        &mut holder,
        json!({"cmd": "insert_component", "path": plate.to_string_lossy(), "base": base,
               "copy_into_project": true}),
    )
    .unwrap();
    // Named after the file asked for, not after its copy.
    assert_eq!(answer["name"], json!("plate"), "{answer}");
    let copy = parts.join("plate_2.mitcad");
    let bolt_copy = parts.join("bolt.mitcad");
    assert_eq!(
        answer["copied"],
        json!([
            {"from": plate.to_string_lossy(), "to": copy.to_string_lossy()},
            {"from": bolt.to_string_lossy(), "to": bolt_copy.to_string_lossy()},
        ]),
        "{answer}"
    );
    let component = answer["component"].as_str().unwrap().parse().unwrap();
    let link = holder
        .assembly()
        .component(component)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert_eq!(link.path, "../parts/plate_2.mitcad");
    // The project's own plate.mitcad is not written over; the copies are
    // single files whose links name the copies.
    assert_eq!(
        std::fs::read_to_string(parts.join("plate.mitcad")).unwrap(),
        own_plate
    );
    assert_eq!(link_paths(&copy), ["bolt.mitcad"]);
    assert_eq!(version(&copy), json!(2));
    assert_eq!(version(&bolt_copy), json!(2));
    // The originals stay as they were.
    assert_eq!(version(&plate), json!(3));
    assert_eq!(link_paths(&plate), ["bolt.mitcad"]);

    // Saved and opened again: the link is followed.
    let holder_path = designs.join("holder.mitcad");
    holder.save_file(&holder_path, FileFormat::Project).unwrap();
    let (mut opened, messages) = open(&holder_path);
    assert!(messages.is_empty(), "{messages:?}");
    let links = query(&opened, json!({"query": "links", "base": base}));
    assert_eq!(links["links"][0]["problem"], Value::Null, "{links}");
    assert_eq!(
        links["links"][0]["path"],
        json!("../parts/plate_2.mitcad"),
        "{links}"
    );
    assert_eq!(links["links"][0]["changed"], json!(false), "{links}");

    // The copy changes: Update Link takes it.
    design(3).save_file(&copy, FileFormat::Project).unwrap();
    let links = query(&opened, json!({"query": "links", "base": base}));
    assert_eq!(links["links"][0]["changed"], json!(true), "{links}");
    let done = command(
        &mut opened,
        json!({"cmd": "update_link", "component": "plate", "base": base}),
    )
    .unwrap();
    assert_eq!(done["updated"], json!([component]), "{done}");
    assert_eq!(opened.component_bodies(component).len(), 3);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn a_linked_file_in_the_project_or_a_copy_is_inserted_as_it_is() {
    let dir = mitcad_model::testing::unique_temp_path("1719-inside");
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    let parts = project.join("parts");
    std::fs::create_dir_all(&parts).unwrap();
    let part = parts.join("part.mitcad");
    std::fs::write(&part, design(1).to_json()).unwrap();
    let outside = dir.join("outside.mitcad");
    std::fs::write(&outside, design(1).to_json()).unwrap();
    let base = project.to_string_lossy().to_string();
    let mut holder = Document::new(MockKernel::default());
    // In the project: nothing is copied.
    let answer = command(
        &mut holder,
        json!({"cmd": "insert_component", "path": part.to_string_lossy(), "base": base,
               "copy_into_project": true}),
    )
    .unwrap();
    assert_eq!(answer["copied"], json!([]), "{answer}");
    // Not linked (Insert as a copy): nothing is copied either.
    let answer = command(
        &mut holder,
        json!({"cmd": "insert_component", "path": outside.to_string_lossy(), "base": base,
               "link": false, "copy_into_project": true}),
    )
    .unwrap();
    assert_eq!(answer["copied"], json!([]), "{answer}");
    let entries: Vec<_> = std::fs::read_dir(&parts).unwrap().collect();
    assert_eq!(entries.len(), 1);
    mitcad_model::testing::remove_temp(&dir);
}
