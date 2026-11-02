// SPDX-License-Identifier: MIT
//! Top-level fields of a design file that this build does not know
//! (mitcad#1132): a newer MitCad wrote them, so they are ignored with a
//! warning that names them, kept with the file they were read from, and
//! dropped by a save over that file (the save path's flag the application
//! asks by); fields inside the known ones are still refused.

use serde_json::{Value, json};

use mitcad_model::file::{FileError, FileFormat};
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model::{Document, test_support::block};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A block's design file with what a newer MitCad might add at the top:
/// comments and their counter.
fn newer_file() -> (String, String) {
    let plain = block().doc.to_json();
    let mut value: Value = serde_json::from_str(&plain).unwrap();
    value["comments"] = json!([{"id": "N1", "text": "Check the wall"}]);
    value["next_comment"] = json!(2);
    (plain, value.to_string())
}

fn file_value(path: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

const WARNING: &str = "this design was written by a newer MitCad: the fields `comments`, `next_comment` were not read";

#[test]
fn unknown_top_level_fields_are_ignored_and_named() {
    let (plain, newer) = newer_file();
    let doc = Document::from_json(&newer, MockKernel::default()).unwrap();
    let reference = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert_eq!(doc.state(), reference.state(), "the rest is read as before");
    assert_eq!(doc.load_warnings(), [WARNING]);
    let unknown = doc.unknown_fields();
    assert_eq!(unknown.fields, ["comments", "next_comment"]);
    assert_eq!(unknown.file, None, "text read without a path");
    // Saving writes only what this build knows.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(saved.get("comments").is_none() && saved.get("next_comment").is_none());

    // One field: singular words.
    let mut one: Value = serde_json::from_str(&plain).unwrap();
    one["comments"] = json!([]);
    let doc = Document::from_json(&one.to_string(), MockKernel::default()).unwrap();
    assert_eq!(
        doc.load_warnings(),
        ["this design was written by a newer MitCad: the field `comments` was not read"]
    );

    // A design without them: no warning, nothing kept.
    assert!(reference.load_warnings().is_empty());
    assert!(reference.unknown_fields().fields.is_empty());
    assert_eq!(
        query(&reference, json!({"query": "unknown_fields"})),
        json!({"fields": [], "file": null})
    );

    // Unknown fields inside the known ones are still refused, saying where.
    let mut inner: Value = serde_json::from_str(&plain).unwrap();
    inner["features"][1]["radus"] = json!("d1");
    match Document::from_json(&inner.to_string(), MockKernel::default()) {
        Err(FileError::Schema(message)) => {
            assert!(message.contains("unknown field `radus`"), "{message}")
        }
        other => panic!(
            "an unknown feature field must be refused: {:?}",
            other.err()
        ),
    }
}

#[test]
fn saving_over_the_file_drops_them_and_another_file_keeps_them() {
    let dir = unique_temp_dir("unknown-fields");
    let path = dir.join("newer.mitcad");
    let other = dir.join("copy.mitcad");
    let (_, newer) = newer_file();
    std::fs::write(&path, &newer).unwrap();

    let doc = Document::load_design(&path, MockKernel::default()).unwrap();
    assert_eq!(doc.load_warnings(), [WARNING]);
    let unknown = doc.unknown_fields();
    assert_eq!(unknown.fields, ["comments", "next_comment"]);
    let absolute = std::path::absolute(&path).unwrap();
    assert_eq!(unknown.file.as_deref(), Some(absolute.as_path()));

    // The save path's flag: the file they were read from, by any spelling
    // of its path, drops them; another file does not.
    let dropped = ["comments", "next_comment"];
    assert_eq!(doc.fields_dropped_by_saving(&path), dropped);
    let spelled = dir.join(".").join("newer.mitcad");
    assert_eq!(doc.fields_dropped_by_saving(&spelled), dropped);
    assert!(doc.fields_dropped_by_saving(&other).is_empty());
    let answer = query(
        &doc,
        json!({"query": "unknown_fields", "path": path.display().to_string()}),
    );
    assert_eq!(answer["fields"], json!(dropped));
    assert_eq!(answer["file"], json!(absolute.display().to_string()));
    assert_eq!(
        answer["message"],
        "saving over this design drops the fields `comments`, `next_comment` that a newer \
         MitCad wrote and this MitCad did not read"
    );
    let answer = query(
        &doc,
        json!({"query": "unknown_fields", "path": other.display().to_string()}),
    );
    assert_eq!(answer["fields"], json!([]));
    assert!(answer.get("message").is_none());
    let answer = query(&doc, json!({"query": "unknown_fields"}));
    assert_eq!(answer["fields"], json!(dropped));
    assert_eq!(answer["message"], WARNING);

    // Saved to another file (Save As): the file read keeps its fields, and
    // saving over it would still drop them.
    doc.save_file(&other, FileFormat::Single).unwrap();
    assert!(file_value(&other).get("comments").is_none());
    assert_eq!(file_value(&path)["next_comment"], json!(2));
    assert_eq!(doc.fields_dropped_by_saving(&path), dropped);

    // Saved over it: the fields are gone from the file and the flag is
    // cleared; opened again, the design has no warning.
    doc.save_file(&path, FileFormat::Single).unwrap();
    let written = file_value(&path);
    assert!(written.get("comments").is_none() && written.get("next_comment").is_none());
    assert!(doc.fields_dropped_by_saving(&path).is_empty());
    assert!(doc.unknown_fields().fields.is_empty());
    let reopened = Document::load_design(&path, MockKernel::default()).unwrap();
    assert!(
        reopened.load_warnings().is_empty(),
        "{:?}",
        reopened.load_warnings()
    );
    remove_temp(&dir);
}
