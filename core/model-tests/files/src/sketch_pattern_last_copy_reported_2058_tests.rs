// SPDX-License-Identifier: MIT
//! The reporter's design of GitHub mitcad/Mitcad#470 and #471 (mitcad#2058;
//! `MITCAD_MITCAD_REPORTED`/`github-470`, when it is there): one sketch
//! with two rectangular pattern records whose every instance is skipped
//! (every copy deleted), saved so by 0.66.0, and a newer pattern. One of
//! the emptied records holds the spacing parameters d3 and d4, which
//! nothing else uses and which could not be deleted; the other uses the
//! sketch's dimensions d1 and d2. Opened, the design keeps them; the next
//! edit of the sketch removes both emptied records with a note each and
//! releases d3 and d4, while the newer pattern and its spacings d6 and d7
//! stay as they were.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model_api::DocumentApi;

/// The reporter's design unpacked from its report's zip into `dir` and
/// loaded with the mock kernel; None, and a note, when the private test
/// files are not here.
fn reported(dir: &Path) -> Option<Document<MockKernel>> {
    let root = std::env::var("MITCAD_MITCAD_REPORTED").ok()?;
    let zip = PathBuf::from(root)
        .join("github-470")
        .join("plank.mitcad.zip");
    if !zip.is_file() {
        eprintln!("skipped: {} is not here", zip.display());
        return None;
    }
    let archive = mitcad_zip::Archive::open(&zip).unwrap();
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name.ends_with(".mitcad"))
        .unwrap_or_else(|| panic!("no design in {}", zip.display()));
    let design = dir.join("design.mitcad");
    std::fs::write(&design, archive.read(entry).unwrap()).unwrap();
    let mut doc = Document::load_design(&design, MockKernel::default())
        .unwrap_or_else(|e| panic!("{}: {e}", design.display()));
    doc.recompute();
    Some(doc)
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The parameter `name` in the `parameters` query.
fn parameter(doc: &Document<MockKernel>, name: &str) -> Option<Value> {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
}

/// The pattern records' ids in the `sketch` query.
fn patterns(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "sketch", "uid": "F1"}))["patterns"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| p["id"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The pattern record `id` as the design file stores it.
fn saved_record(doc: &Document<MockKernel>, id: &str) -> Value {
    let design: Value = serde_json::from_str(&doc.to_json()).unwrap();
    design["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == "F1")
        .and_then(|f| f["patterns"].as_array())
        .and_then(|a| a.iter().find(|p| p["id"] == id))
        .cloned()
        .unwrap_or_else(|| panic!("no pattern {id} in the file"))
}

#[test]
fn the_reported_emptied_patterns_go_with_the_next_edit_and_free_d3_and_d4() {
    let dir = unique_temp_dir("2058-reported");
    let Some(mut d) = reported(&dir) else {
        remove_temp(&dir);
        return;
    };
    // Opened, nothing changes: d3 and d4 belong to the emptied k9, which
    // makes no copy, as k8 does not; k12 is the newer pattern.
    assert_eq!(patterns(&d), ["k8", "k9", "k12"]);
    let k9 = saved_record(&d, "k9");
    assert_eq!(k9["spacing"], json!(["d3", "d4"]), "{k9}");
    assert_eq!(k9["skipped"], json!([[0, 1], [1, 0], [1, 1]]), "{k9}");
    assert_eq!(k9["copies"], json!([]), "{k9}");
    for name in ["d3", "d4"] {
        assert!(parameter(&d, name).is_some(), "{name}");
    }
    let k12 = saved_record(&d, "k12");
    let d6 = parameter(&d, "d6").unwrap();
    let d7 = parameter(&d, "d7").unwrap();

    // An edit of the sketch removes both emptied records, a note each.
    let added: Value = serde_json::from_str(
        &d.command(
            &json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [500, 500]}).to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        added["note_messages"],
        json!([
            "the pattern was removed with its last copy",
            "the pattern was removed with its last copy"
        ]),
        "{added}"
    );
    assert_eq!(patterns(&d), ["k12"]);
    // d3 and d4 are gone; the sketch's dimensions d1 and d2, its driven d5
    // and the newer pattern's d6 and d7 stay, and k12 is unchanged.
    for name in ["d3", "d4"] {
        assert!(parameter(&d, name).is_none(), "{name} is still there");
    }
    for name in ["d1", "d2", "d5"] {
        assert!(parameter(&d, name).is_some(), "{name} is gone");
    }
    assert_eq!(parameter(&d, "d6").unwrap(), d6);
    assert_eq!(parameter(&d, "d7").unwrap(), d7);
    assert_eq!(saved_record(&d, "k12"), k12);
    remove_temp(&dir);
}
