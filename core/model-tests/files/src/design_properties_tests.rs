// SPDX-License-Identifier: MIT
//! The design's properties (mitcad#1088, File › Design Properties…): the
//! summary kept as the root's property set Summary, saved and read back,
//! one undo step; the statistics a stamped save writes, kept by a save
//! that changes nothing, and unknown creation of a file saved before them.

use serde_json::{Value, json};

use mitcad_model::file::{FileFormat, SaveStamp};
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model::{Document, test_support::block};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn properties(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "design_properties"}))
}

/// The root's Summary set as Property Sets… lists it: `Key = text
/// (source[, edited])`.
fn summary_rows(doc: &Document<MockKernel>) -> Vec<String> {
    let answer = query(doc, json!({"query": "property_sets", "component": "C0"}));
    answer["levels"][0]["sets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["name"] == "Summary")
        .flat_map(|s| s["properties"].as_array().unwrap().clone())
        .map(|p| {
            let edited = if p["edited"] == true { ", edited" } else { "" };
            format!(
                "{} = {} ({}{edited})",
                p["key"].as_str().unwrap(),
                p["text"].as_str().unwrap(),
                p["source"].as_str().unwrap()
            )
        })
        .collect()
}

fn stamp(time: &str, by: &str) -> SaveStamp {
    SaveStamp {
        time: time.to_owned(),
        by: by.to_owned(),
        with: "MitCad 9.8.7".to_owned(),
    }
}

fn file_value(path: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_summary_is_kept_on_the_root_saved_and_read_back() {
    let mut doc = Document::new(MockKernel::default());
    // Nothing set: every field empty, nothing known of the statistics.
    let empty = properties(&doc);
    assert_eq!(empty["set"], "Summary");
    assert_eq!(
        empty["summary"],
        json!({"title": "", "subject": "", "author": "", "keywords": "", "comments": ""})
    );
    assert_eq!(
        empty["statistics"],
        json!({"created": null, "last_saved": null, "last_saved_by": null,
               "last_saved_with": null})
    );

    // Title, Author and multi-line Comments in one undo step.
    let set = command(
        &mut doc,
        json!({"cmd": "set_design_summary", "title": "Gearbox housing", "author": "A. Maker",
               "comments": "First line\nSecond line", "subject": ""}),
    );
    assert_eq!(set["changed"], 3);
    assert_eq!(set["summary"]["title"], "Gearbox housing");
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Design Properties"
    );
    assert_eq!(
        summary_rows(&doc),
        [
            "Title = Gearbox housing (user)",
            "Author = A. Maker (user)",
            "Comments = First line\nSecond line (user)"
        ]
    );
    // The same texts again change nothing and make no step.
    let again = command(
        &mut doc,
        json!({"cmd": "set_design_summary", "title": "Gearbox housing", "author": "A. Maker"}),
    );
    assert_eq!(again["changed"], 0);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(properties(&doc)["summary"]["title"], "");
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(properties(&doc)["summary"]["author"], "A. Maker");

    // Saved and read back: the root's Summary set in the file.
    let text = doc.to_json();
    let file: Value = serde_json::from_str(&text).unwrap();
    let root = file["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["owner"] == "C0")
        .unwrap()
        .clone();
    assert_eq!(root["sets"][0]["name"], "Summary");
    let back = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(properties(&back)["summary"], properties(&doc)["summary"]);
    assert_eq!(
        properties(&back)["summary"]["comments"],
        "First line\nSecond line"
    );

    // An empty text removes the property; an unknown field is refused.
    command(
        &mut doc,
        json!({"cmd": "set_design_summary", "author": "  "}),
    );
    assert_eq!(
        summary_rows(&doc),
        [
            "Title = Gearbox housing (user)",
            "Comments = First line\nSecond line (user)"
        ]
    );
    let error = doc
        .command(&json!({"cmd": "set_design_summary", "revision": "B"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("revision"), "{error}");
}

#[test]
fn an_imported_summary_shows_and_keeps_its_source_value() {
    // The `.ipt` import's Summary set on the root is the same set.
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "C0", "set": "Summary", "key": "Title",
               "value": "Bracket", "source": "ipt"}),
    );
    assert_eq!(properties(&doc)["summary"]["title"], "Bracket");
    command(
        &mut doc,
        json!({"cmd": "set_design_summary", "title": "Bracket, left"}),
    );
    assert_eq!(summary_rows(&doc), ["Title = Bracket, left (ipt, edited)"]);
}

#[test]
fn a_stamped_save_writes_the_statistics_and_keeps_them_while_nothing_changes() {
    let dir = unique_temp_dir("design-statistics");
    let path = dir.join("block.mitcad");
    let mut b = block();
    // A new design: created at its first save.
    b.doc
        .save_file_stamped(
            &path,
            FileFormat::Single,
            &stamp("2026-11-01T09:00:00Z", "anna"),
        )
        .unwrap();
    let written = file_value(&path)["statistics"].clone();
    assert_eq!(
        written,
        json!({"created": "2026-11-01T09:00:00Z", "last_saved": "2026-11-01T09:00:00Z",
               "last_saved_by": "anna", "last_saved_with": "MitCad 9.8.7"})
    );
    assert_eq!(
        properties(&b.doc)["statistics"],
        written,
        "the document shows what it wrote"
    );

    // Opened: the file's statistics. Saved again with no change: the same
    // bytes, the statistics as they were.
    let opened = Document::load_design(&path, MockKernel::default()).unwrap();
    assert_eq!(properties(&opened)["statistics"], written);
    let before = std::fs::read(&path).unwrap();
    opened
        .save_file_stamped(
            &path,
            FileFormat::Single,
            &stamp("2026-11-02T09:00:00Z", "bea"),
        )
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(properties(&opened)["statistics"], written);

    // A change saved: the last save's time, user and program; created kept.
    command(
        &mut b.doc,
        json!({"cmd": "set_design_summary", "title": "Block"}),
    );
    b.doc
        .save_file_stamped(
            &path,
            FileFormat::Single,
            &stamp("2026-11-03T10:30:00Z", "bea"),
        )
        .unwrap();
    assert_eq!(
        file_value(&path)["statistics"],
        json!({"created": "2026-11-01T09:00:00Z", "last_saved": "2026-11-03T10:30:00Z",
               "last_saved_by": "bea", "last_saved_with": "MitCad 9.8.7"})
    );
    // Undo leaves them: they are no part of the design.
    command(&mut b.doc, json!({"cmd": "undo"}));
    assert_eq!(
        properties(&b.doc)["statistics"]["last_saved"],
        "2026-11-03T10:30:00Z"
    );
    // A plain save (scripts, exports of a design) writes them as they are.
    let other = dir.join("copy.mitcad");
    b.doc.save_file(&other, FileFormat::Single).unwrap();
    assert_eq!(
        file_value(&other)["statistics"]["last_saved"],
        "2026-11-03T10:30:00Z"
    );
    remove_temp(&dir);
}

#[test]
fn a_file_saved_before_the_statistics_reads_with_created_unknown() {
    let dir = unique_temp_dir("design-statistics-old");
    let path = dir.join("old.mitcad");
    // A file without statistics, as earlier versions wrote it.
    let b = block();
    b.doc.save_file(&path, FileFormat::Single).unwrap();
    assert!(file_value(&path).get("statistics").is_none());
    let mut old = Document::load_design(&path, MockKernel::default()).unwrap();
    assert!(properties(&old)["statistics"]["created"].is_null());

    // Saved with a change: the last save known, the creation still not.
    command(
        &mut old,
        json!({"cmd": "set_design_summary", "subject": "Spare part"}),
    );
    old.save_file_stamped(
        &path,
        FileFormat::Single,
        &stamp("2026-11-04T08:00:00Z", "anna"),
    )
    .unwrap();
    let written = file_value(&path)["statistics"].clone();
    assert!(written.get("created").is_none(), "{written}");
    assert_eq!(written["last_saved"], "2026-11-04T08:00:00Z");
    // Read back and saved again: still unknown.
    let mut again = Document::load_design(&path, MockKernel::default()).unwrap();
    assert!(properties(&again)["statistics"]["created"].is_null());
    command(
        &mut again,
        json!({"cmd": "set_design_summary", "subject": "Spare part, new"}),
    );
    again
        .save_file_stamped(
            &path,
            FileFormat::Single,
            &stamp("2026-11-05T08:00:00Z", "anna"),
        )
        .unwrap();
    assert!(file_value(&path)["statistics"].get("created").is_none());

    // An empty design's file (a project's New Design writes one) is a new
    // design: created when first saved with a change.
    let empty = dir.join("empty.mitcad");
    std::fs::write(&empty, mitcad_model::file::empty_design()).unwrap();
    let mut new = Document::load_design(&empty, MockKernel::default()).unwrap();
    command(
        &mut new,
        json!({"cmd": "set_design_summary", "title": "New"}),
    );
    new.save_file_stamped(
        &empty,
        FileFormat::Single,
        &stamp("2026-11-06T08:00:00Z", "anna"),
    )
    .unwrap();
    assert_eq!(
        file_value(&empty)["statistics"]["created"],
        "2026-11-06T08:00:00Z"
    );

    // Statistics that are not an object are refused.
    let mut broken = file_value(&path);
    broken["statistics"] = json!("yesterday");
    let error = Document::from_json(&broken.to_string(), MockKernel::default())
        .err()
        .expect("refused")
        .to_string();
    assert!(error.contains("statistics"), "{error}");
    remove_temp(&dir);
}
