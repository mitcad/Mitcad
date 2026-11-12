// SPDX-License-Identifier: MIT
//! The preview a design file carries for file managers (mitcad#1539): the
//! application's Save writes the 3D view's picture into the file's
//! `preview`, which opening keeps beside the design like the statistics;
//! a save that changes nothing keeps the one the file has, undo leaves
//! it, other saves write it as it is; a file without one reads as before,
//! and one that cannot be read is left out with a warning.

use serde_json::{Value, json};

use mitcad_model::file::{DesignPreview, FileFormat, SaveStamp};
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model::{Document, test_support::block};
use mitcad_model_api::DocumentApi;

/// A PNG file's signature and header chunk, as large as the picture
/// says: all the preview reads of it.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn preview(width: u32, height: u32) -> DesignPreview {
    DesignPreview::from_png(&png(width, height)).unwrap()
}

fn stamp(time: &str) -> SaveStamp {
    SaveStamp {
        time: time.to_owned(),
        by: "anna".to_owned(),
        with: "MitCad 9.8.7".to_owned(),
    }
}

fn file_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) {
    doc.command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
}

#[test]
fn a_save_given_a_preview_writes_it_and_opening_keeps_it() {
    let dir = unique_temp_dir("design-preview");
    let path = dir.join("block.mitcad");
    let mut b = block();
    let first = preview(256, 192);
    b.doc
        .save_file_previewed(
            &path,
            FileFormat::Single,
            &stamp("2026-11-12T09:00:00Z"),
            Some(first.clone()),
        )
        .unwrap();
    let text = file_text(&path);
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["preview"]["format"], "png");
    assert_eq!(value["preview"]["width"], 256);
    assert_eq!(value["preview"]["height"], 192);
    assert!(
        value["preview"]["data"]
            .as_str()
            .unwrap()
            .starts_with("iVBORw0KGgo")
    );
    // The last field, after the definition and the statistics.
    assert!(text.find("\"preview\"").unwrap() > text.find("\"statistics\"").unwrap());
    assert_eq!(b.doc.design_preview(), Some(first.clone()));

    // Opened: the file's preview, no warning, no unknown field. Saved
    // again with nothing changed and another view: the same bytes, the
    // preview as it was.
    let opened = Document::load_design(&path, MockKernel::default()).unwrap();
    assert_eq!(opened.design_preview(), Some(first.clone()));
    assert!(
        opened.load_warnings().is_empty(),
        "{:?}",
        opened.load_warnings()
    );
    assert!(opened.unknown_fields().fields.is_empty());
    let before = std::fs::read(&path).unwrap();
    opened
        .save_file_previewed(
            &path,
            FileFormat::Single,
            &stamp("2026-11-12T10:00:00Z"),
            Some(preview(128, 128)),
        )
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(opened.design_preview(), Some(first.clone()));

    // A change saved with another view: the new preview.
    command(
        &mut b.doc,
        json!({"cmd": "set_design_summary", "title": "Block"}),
    );
    let second = preview(200, 150);
    b.doc
        .save_file_previewed(
            &path,
            FileFormat::Single,
            &stamp("2026-11-12T11:00:00Z"),
            Some(second.clone()),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&file_text(&path)).unwrap();
    assert_eq!(value["preview"]["width"], 200);
    assert_eq!(
        Document::load_design(&path, MockKernel::default())
            .unwrap()
            .design_preview(),
        Some(second.clone())
    );
    // Undo leaves it: no part of the design.
    command(&mut b.doc, json!({"cmd": "undo"}));
    assert_eq!(b.doc.design_preview(), Some(second.clone()));
    // A save without a picture (no 3D view) and a plain save (autosave,
    // the command line tool) write the one the design has.
    b.doc
        .save_file_stamped(&path, FileFormat::Single, &stamp("2026-11-12T12:00:00Z"))
        .unwrap();
    let value: Value = serde_json::from_str(&file_text(&path)).unwrap();
    assert_eq!(value["preview"]["width"], 200);
    let other = dir.join("copy.mitcad");
    b.doc.save_file(&other, FileFormat::Single).unwrap();
    assert_eq!(
        Document::load_design(&other, MockKernel::default())
            .unwrap()
            .design_preview(),
        Some(second)
    );
    remove_temp(&dir);
}

#[test]
fn a_file_without_a_preview_reads_and_saves_as_before() {
    let dir = unique_temp_dir("design-preview-old");
    let path = dir.join("old.mitcad");
    let b = block();
    b.doc.save_file(&path, FileFormat::Single).unwrap();
    let text = file_text(&path);
    assert!(!text.contains("\"preview\""));
    let old = Document::load_design(&path, MockKernel::default()).unwrap();
    assert_eq!(old.design_preview(), None);
    assert!(old.load_warnings().is_empty(), "{:?}", old.load_warnings());
    // Saved again without a picture: still none, the text as before.
    old.save_file(&path, FileFormat::Single).unwrap();
    assert_eq!(file_text(&path), text);
    remove_temp(&dir);
}

#[test]
fn a_preview_that_cannot_be_read_is_left_out_with_a_warning() {
    let mut value: Value = serde_json::from_str(&block().doc.to_json()).unwrap();
    value["preview"] = json!({"format": "png", "data": "not base64"});
    let doc = Document::from_json(&value.to_string(), MockKernel::default()).unwrap();
    assert_eq!(doc.design_preview(), None);
    assert_eq!(doc.load_warnings().len(), 1, "{:?}", doc.load_warnings());
    assert!(doc.load_warnings()[0].starts_with("the design's preview is left out: "));
    assert!(doc.unknown_fields().fields.is_empty());
    assert!(!doc.to_json().contains("\"preview\""));

    // Null, as a hand-edited file may have it: none, no warning.
    value["preview"] = Value::Null;
    let doc = Document::from_json(&value.to_string(), MockKernel::default()).unwrap();
    assert_eq!(doc.design_preview(), None);
    assert!(doc.load_warnings().is_empty());
}
