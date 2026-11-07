// SPDX-License-Identifier: MIT
//! Linked designs read again (mitcad#1416) with the mock kernel: a holder
//! and the design it links, both saved in a project (version 3, their
//! B-rep data in the project's store, the holder in a folder of the
//! project as a design of the project is), the linked design changed and
//! saved after the holder. Opened from its file, the holder takes the
//! changed design's bodies with a load warning naming the file; while it is
//! open, the `links` query says which linked file changed, and
//! `update_link` reads it again as one undo step (Update Link) or every
//! linked design (Update All Links). The owner's reported pair is read by
//! the ctest `cli.linked_design_update_reported`
//! (`tools/cli/linked-design-update-test.cmake`).

use std::path::Path;

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::ComponentUid;
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

/// A block of 10 × 10 × 5 at x = `x` in the active component.
fn block(doc: &mut Document<MockKernel>, x: f64) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
}

/// A design of `blocks` blocks 20 mm apart.
fn design(blocks: usize) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for i in 0..blocks {
        block(&mut doc, 20.0 * i as f64);
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

/// The project: `parts/electronics.mitcad` (one block) linked into
/// `designs/holder.mitcad` as the component `electronics`, and
/// `parts/cover.mitcad` (one block) linked as `cover`; both saved as the
/// project saves them.
struct Pair {
    dir: std::path::PathBuf,
    holder: std::path::PathBuf,
    linked: std::path::PathBuf,
    cover: std::path::PathBuf,
    electronics: ComponentUid,
    cover_component: ComponentUid,
}

fn pair(name: &str) -> Pair {
    let dir = mitcad_model::testing::unique_temp_path(name);
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    let designs = project.join("designs");
    let parts = project.join("parts");
    std::fs::create_dir_all(&designs).unwrap();
    std::fs::create_dir_all(&parts).unwrap();
    let linked = parts.join("electronics.mitcad");
    design(1).save_file(&linked, FileFormat::Project).unwrap();
    let cover = parts.join("cover.mitcad");
    design(1).save_file(&cover, FileFormat::Project).unwrap();
    let mut holder = Document::new(MockKernel::default());
    let options = InsertOptions {
        base: Some(designs.clone()),
        ..InsertOptions::default()
    };
    let (electronics, _) = holder
        .insert_component(linked.to_str().unwrap(), &options)
        .unwrap();
    let (cover_component, _) = holder
        .insert_component(cover.to_str().unwrap(), &options)
        .unwrap();
    let path = designs.join("holder.mitcad");
    let saved = holder.save_file(&path, FileFormat::Project).unwrap();
    // Version 3: the linked bodies are in the project's store.
    let value: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(value["version"], json!(3), "{saved}");
    Pair {
        dir,
        holder: path,
        linked,
        cover,
        electronics,
        cover_component,
    }
}

#[test]
fn opening_the_holder_takes_a_linked_design_changed_since() {
    let pair = pair("1416-open");
    // Opened as saved: nothing changed, nothing is taken.
    let (unchanged, messages) = open(&pair.holder);
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(unchanged.component_bodies(pair.electronics).len(), 1);

    // The linked design gets a second block and is saved after the holder.
    design(2)
        .save_file(&pair.linked, FileFormat::Project)
        .unwrap();
    let (opened, messages) = open(&pair.holder);
    assert_eq!(
        messages,
        ["electronics is updated from ../parts/electronics.mitcad"],
        "{messages:?}"
    );
    assert_eq!(opened.component_bodies(pair.electronics).len(), 2);
    assert_eq!(opened.component_bodies(pair.cover_component).len(), 1);
    // As a load warning, which names the file.
    assert!(
        opened
            .load_warnings()
            .iter()
            .any(|w| w.contains("electronics.mitcad")),
        "{:?}",
        opened.load_warnings()
    );
    // Opened as read: no undo step.
    assert_eq!(opened.undo_depth(), 0);
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn update_link_reads_a_changed_linked_design_as_one_undo_step() {
    let pair = pair("1416-update");
    let (mut doc, messages) = open(&pair.holder);
    assert!(messages.is_empty(), "{messages:?}");
    let base = pair.holder.parent().unwrap().to_string_lossy().to_string();

    // Nothing changed: the query says so, Update Link changes nothing.
    let links = query(&doc, json!({"query": "links", "base": base}));
    let rows = links["links"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{links}");
    assert_eq!(rows[0]["component"], json!(pair.electronics), "{links}");
    assert_eq!(rows[0]["path"], json!("../parts/electronics.mitcad"));
    assert_eq!(rows[0]["changed"], json!(false), "{links}");
    assert_eq!(rows[0]["problem"], Value::Null, "{links}");
    let file = rows[0]["file"].as_str().unwrap();
    assert!(file.ends_with("electronics.mitcad"), "{file}");
    let done = command(
        &mut doc,
        json!({"cmd": "update_link", "component": pair.electronics.to_string(), "base": base}),
    )
    .unwrap();
    assert_eq!(done["updated"], json!([]), "{done}");
    assert_eq!(
        done["messages"],
        json!(["electronics is up to date with ../parts/electronics.mitcad"]),
        "{done}"
    );
    assert_eq!(doc.undo_depth(), 0);

    // Both linked designs change on disk while the holder is open.
    design(2)
        .save_file(&pair.linked, FileFormat::Project)
        .unwrap();
    design(3)
        .save_file(&pair.cover, FileFormat::Project)
        .unwrap();
    // The base defaults to the folder the design was read from.
    let links = query(&doc, json!({"query": "links"}));
    let changed: Vec<&Value> = links["links"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["changed"] == json!(true))
        .collect();
    assert_eq!(changed.len(), 2, "{links}");
    // Nothing is taken on its own.
    assert_eq!(doc.component_bodies(pair.electronics).len(), 1);

    // Update Link: that component only, one undo step.
    let done = command(
        &mut doc,
        json!({"cmd": "update_link", "component": "electronics", "base": base}),
    )
    .unwrap();
    assert_eq!(done["updated"], json!([pair.electronics]), "{done}");
    assert_eq!(
        done["messages"],
        json!(["electronics is updated from ../parts/electronics.mitcad"]),
        "{done}"
    );
    assert_eq!(doc.component_bodies(pair.electronics).len(), 2);
    assert_eq!(doc.component_bodies(pair.cover_component).len(), 1);
    assert_eq!(doc.undo_depth(), 1);
    let links = query(&doc, json!({"query": "links", "base": base}));
    assert_eq!(links["links"][0]["changed"], json!(false), "{links}");
    assert_eq!(links["links"][1]["changed"], json!(true), "{links}");
    // Undone in one step: the old bodies and the link changed again.
    doc.undo();
    assert_eq!(doc.component_bodies(pair.electronics).len(), 1);
    let links = query(&doc, json!({"query": "links", "base": base}));
    assert_eq!(links["links"][0]["changed"], json!(true), "{links}");

    // Update All Links: both, one undo step.
    let done = command(&mut doc, json!({"cmd": "update_link", "base": base})).unwrap();
    assert_eq!(
        done["updated"],
        json!([pair.electronics, pair.cover_component]),
        "{done}"
    );
    assert_eq!(doc.component_bodies(pair.electronics).len(), 2);
    assert_eq!(doc.component_bodies(pair.cover_component).len(), 3);
    assert_eq!(doc.undo_depth(), 1);
    doc.undo();
    assert_eq!(doc.component_bodies(pair.electronics).len(), 1);
    assert_eq!(doc.component_bodies(pair.cover_component).len(), 1);
    let _ = std::fs::remove_dir_all(&pair.dir);
}

#[test]
fn update_link_refuses_what_is_no_linked_design_and_keeps_an_unreadable_one() {
    let pair = pair("1416-refused");
    let (mut doc, _) = open(&pair.holder);
    let base = pair.holder.parent().unwrap().to_string_lossy().to_string();
    let error = command(
        &mut doc,
        json!({"cmd": "update_link", "component": "C0", "base": base}),
    )
    .unwrap_err();
    assert!(error.contains("is not a linked design"), "{error}");
    // A file gone: the query names the problem, Update Link keeps the
    // bodies and says why.
    std::fs::remove_file(&pair.linked).unwrap();
    let links = query(&doc, json!({"query": "links", "base": base}));
    let problem = links["links"][0]["problem"].as_str().unwrap_or_default();
    assert!(problem.contains("cannot read"), "{links}");
    assert_eq!(links["links"][0]["changed"], json!(false), "{links}");
    let done = command(
        &mut doc,
        json!({"cmd": "update_link", "component": "electronics", "base": base}),
    )
    .unwrap();
    assert_eq!(done["updated"], json!([]), "{done}");
    let message = done["messages"][0].as_str().unwrap();
    assert!(
        message.contains("cannot read") && message.contains("keeps the bodies"),
        "{done}"
    );
    assert_eq!(doc.component_bodies(pair.electronics).len(), 1);
    assert_eq!(doc.undo_depth(), 0);
    let _ = std::fs::remove_dir_all(&pair.dir);
}
