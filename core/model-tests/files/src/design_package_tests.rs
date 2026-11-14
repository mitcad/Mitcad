// SPDX-License-Identifier: MIT
//! The design packed into a zip for a report (mitcad#1795, GitHub
//! mitcad/Mitcad#328) with the mock kernel: the `design_package` query
//! writes the open design as it is now, the designs it links (and theirs)
//! at their paths in the project, the B-rep data they refer to in the
//! project's store, its textures' images, the project's marker and the
//! caller's files; nothing from other designs, no link it does not follow,
//! no file outside the project. The zip unpacked opens as a project again:
//! the design reads its linked designs from it.

use std::path::Path;

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model::{Document, FileFormat, InsertOptions};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Result<Value, String> {
    doc.query(&query.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
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

/// `path` linked into `doc`, whose folder is `base`.
fn link(doc: &mut Document<MockKernel>, path: &Path, base: &Path) {
    let options = InsertOptions {
        base: Some(base.to_path_buf()),
        ..InsertOptions::default()
    };
    doc.insert_component(path.to_str().unwrap(), &options)
        .unwrap();
}

/// The zip's entries unpacked into `folder`.
fn unpack(zip: &Path, folder: &Path) -> Vec<String> {
    let archive = mitcad_zip::Archive::open(zip).unwrap();
    let mut names = Vec::new();
    for entry in archive.entries() {
        let target = folder.join(&entry.name);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, archive.read(entry).unwrap()).unwrap();
        names.push(entry.name.clone());
    }
    names
}

/// A file's bytes as they are.
fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn the_design_its_links_and_their_data_open_again_from_the_zip() {
    let dir = unique_temp_dir("1795-package");
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    let designs = project.join("designs");
    let parts = project.join("parts");
    let textures = project.join("textures");
    for folder in [&designs, &parts, &textures] {
        std::fs::create_dir_all(folder).unwrap();
    }
    // parts/plate.mitcad links parts/bolt.mitcad beside it, saved as the
    // project versions it: its linked bodies' B-rep data in the store.
    let bolt = parts.join("bolt.mitcad");
    design(1).save_file(&bolt, FileFormat::Project).unwrap();
    let plate = parts.join("plate.mitcad");
    let mut plate_doc = design(2);
    link(&mut plate_doc, &bolt, &parts);
    plate_doc.save_file(&plate, FileFormat::Project).unwrap();
    let plate_text = std::fs::read_to_string(&plate).unwrap();
    let stored = mitcad_model::file::brep_references(&plate_text).unwrap();
    assert!(!stored.is_empty(), "the plate refers to data in the store");
    // Another design of the project that the holder does not link.
    design(1)
        .save_file(&designs.join("other.mitcad"), FileFormat::Project)
        .unwrap();
    // A design outside the project, linked by an absolute path: not
    // followed, so not packed.
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let outside = elsewhere.join("outside.mitcad");
    design(1).save_file(&outside, FileFormat::Single).unwrap();
    // A texture in the project and one outside it.
    std::fs::write(textures.join("wood.png"), b"\x89PNG wood").unwrap();
    std::fs::write(elsewhere.join("secret.png"), b"\x89PNG secret").unwrap();

    let holder_path = designs.join("holder.mitcad");
    let mut holder = design(1);
    link(&mut holder, &plate, &designs);
    link(&mut holder, &outside, &designs);
    command(
        &mut holder,
        json!({"cmd": "create_appearance", "id": "wood", "name": "Wood", "based_on": "wood_oak",
               "texture": {"path": "../textures/wood.png"}}),
    );
    command(
        &mut holder,
        json!({"cmd": "create_appearance", "id": "secret", "name": "Secret",
               "based_on": "wood_oak",
               "texture": {"path": elsewhere.join("secret.png").to_string_lossy()}}),
    );
    holder.save_file(&holder_path, FileFormat::Project).unwrap();
    // A change after the save: the zip has the design as it is now.
    let sketch = holder.add_sketch(SketchPlane::Xy).unwrap().uid;
    let now = holder.to_json();

    let zip = dir.join("reports").join("Bug-plate-20261113-120000.zip");
    let shot = dir.join("screenshot.png");
    std::fs::write(&shot, b"\x89PNG shot").unwrap();
    let answer = query(
        &holder,
        json!({"query": "design_package", "file": holder_path.to_string_lossy(),
               "zip": zip.to_string_lossy(),
               "extra": [{"name": "mitcad-report/report.md", "text": "### Description\n\nIt broke.\n"},
                         {"name": "mitcad-report/screenshot.png", "path": shot.to_string_lossy()}]}),
    )
    .unwrap();
    assert_eq!(answer["zip"], json!(zip.to_string_lossy()), "{answer}");
    assert_eq!(answer["design"], json!("designs/holder.mitcad"), "{answer}");
    assert_eq!(answer["size"], json!(bytes(&zip).len()), "{answer}");
    assert_eq!(answer["missing"], json!([]), "{answer}");
    assert_eq!(answer["left_out"], json!(["secret.png"]), "{answer}");
    let files: Vec<String> = answer["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        files[0], "designs/holder.mitcad",
        "the open design first: {files:?}"
    );
    for name in [
        "parts/plate.mitcad",
        "parts/bolt.mitcad",
        "textures/wood.png",
        ".mitcad/project.json",
        "mitcad-report/report.md",
        "mitcad-report/screenshot.png",
    ] {
        assert!(files.iter().any(|f| f == name), "{name} missing: {files:?}");
    }
    for sha256 in &stored {
        let name = mitcad_model::file::brep_path(sha256);
        assert!(files.contains(&name), "{name} missing: {files:?}");
    }
    // Nothing from other designs, the project's own state or outside.
    for name in &files {
        assert!(
            !name.contains("other.mitcad")
                && !name.contains("outside")
                && !name.contains("secret")
                && !name.starts_with(".mitcad/local"),
            "{name} should not be packed"
        );
    }
    assert_eq!(
        files.len(),
        7 + stored.len(),
        "the design, two linked designs, a texture, the marker, two report files and the \
         plate's data: {files:?}"
    );

    // Unpacked, it is the project's subset: the files as they are, the
    // open design as it is now.
    let unpacked = dir.join("unpacked");
    let names = unpack(&zip, &unpacked);
    assert_eq!(names, files);
    assert_eq!(
        std::fs::read_to_string(unpacked.join("designs/holder.mitcad")).unwrap(),
        now
    );
    assert_eq!(bytes(&unpacked.join("parts/plate.mitcad")), bytes(&plate));
    assert_eq!(bytes(&unpacked.join("parts/bolt.mitcad")), bytes(&bolt));
    assert_eq!(bytes(&unpacked.join("textures/wood.png")), b"\x89PNG wood");
    assert_eq!(
        bytes(&unpacked.join("mitcad-report/screenshot.png")),
        b"\x89PNG shot"
    );
    assert_eq!(
        bytes(&unpacked.join(".mitcad/project.json")),
        bytes(&project.join(".mitcad/project.json"))
    );

    // It opens as a project again: the holder follows its link to the
    // plate there, and the plate reads its data from the unpacked store.
    let opened_path = unpacked.join("designs").join("holder.mitcad");
    let text = std::fs::read_to_string(&opened_path).unwrap();
    let mut opened = Document::from_json_at(&text, &opened_path, MockKernel::default()).unwrap();
    opened.update_links(opened_path.parent()).unwrap();
    opened.recompute();
    assert!(
        opened.feature(sketch).is_some(),
        "the change after the save is there"
    );
    let base = opened_path.parent().unwrap().to_string_lossy().to_string();
    let links = query(&opened, json!({"query": "links", "base": base})).unwrap();
    let plate_link = links["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["path"] == json!("../parts/plate.mitcad"))
        .unwrap_or_else(|| panic!("no link to the plate: {links}"));
    assert_eq!(plate_link["problem"], Value::Null, "{links}");
    assert_eq!(
        plate_link["file"],
        json!(
            unpacked
                .join("parts")
                .join("plate.mitcad")
                .to_string_lossy()
        ),
        "{links}"
    );
    let plate_opened = unpacked.join("parts/plate.mitcad");
    let plate_doc = Document::from_json_at(
        &std::fs::read_to_string(&plate_opened).unwrap(),
        &plate_opened,
        MockKernel::default(),
    )
    .unwrap();
    assert!(
        plate_doc.load_warnings().is_empty(),
        "the plate's data is read from the unpacked store: {:?}",
        plate_doc.load_warnings()
    );
    remove_temp(&dir);
}

#[test]
fn a_design_never_saved_and_wrong_extra_names() {
    let dir = unique_temp_dir("1795-unsaved");
    let doc = design(1);
    let zip = dir.join("feedback.zip");
    let answer = query(
        &doc,
        json!({"query": "design_package", "zip": zip.to_string_lossy(),
               "extra": [{"name": "mitcad-report/log.txt", "text": "line"}]}),
    )
    .unwrap();
    assert_eq!(answer["design"], json!("design.mitcad"), "{answer}");
    assert_eq!(
        answer["files"],
        json!(["design.mitcad", "mitcad-report/log.txt"]),
        "{answer}"
    );
    let archive = mitcad_zip::Archive::open(&zip).unwrap();
    assert_eq!(
        String::from_utf8(archive.read_by_name("design.mitcad").unwrap()).unwrap(),
        doc.to_json()
    );

    for (extra, refused) in [
        (
            json!({"name": "../escape.txt", "text": "x"}),
            "not a file name in a zip",
        ),
        (
            json!({"name": "/abs.txt", "text": "x"}),
            "not a file name in a zip",
        ),
        (
            json!({"name": "design.mitcad", "text": "x"}),
            "in the package already",
        ),
        (json!({"name": "a.txt"}), "give either text or path"),
        (
            json!({"name": "a.txt", "text": "x", "path": "/x"}),
            "give either text or path",
        ),
    ] {
        let error = query(
            &doc,
            json!({"query": "design_package", "zip": zip.to_string_lossy(), "extra": [extra]}),
        )
        .unwrap_err();
        assert!(error.contains(refused), "{extra}: {error}");
    }
    // A refused package leaves the zip written before as it was.
    assert!(mitcad_zip::Archive::open(&zip).is_ok());
    remove_temp(&dir);
}
