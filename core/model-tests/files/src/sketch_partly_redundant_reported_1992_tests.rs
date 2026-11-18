// SPDX-License-Identifier: MIT
//! The reporter's design of GitHub mitcad/Mitcad#420 (mitcad#1992;
//! `MITCAD_MITCAD_REPORTED`/`github-420`, when it is there): a rectangle
//! and a construction arc about the origin, its ends made symmetric about
//! the y axis. The Symmetry partly repeats the arc's own equation (its
//! centre is on the axis) and still takes a degree of freedom away: the
//! sketch tells it as information, not as a warning that it follows from
//! the others.

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
        .join("github-420")
        .join("Bug-Odd-warning-when-centering-an-arc-20261117-064940.zip");
    if !zip.is_file() {
        eprintln!("skipped: {} is not here", zip.display());
        return None;
    }
    let archive = mitcad_zip::Archive::open(&zip).unwrap();
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name == "design.mitcad")
        .unwrap_or_else(|| panic!("no design in {}", zip.display()));
    let design = dir.join("design.mitcad");
    std::fs::write(&design, archive.read(entry).unwrap()).unwrap();
    let mut doc = Document::load_design(&design, MockKernel::default())
        .unwrap_or_else(|e| panic!("{}: {e}", design.display()));
    doc.recompute();
    Some(doc)
}

#[test]
fn the_reported_arc_s_symmetry_is_information_not_a_warning() {
    let dir = unique_temp_dir("1992-reported");
    let Some(d) = reported(&dir) else {
        remove_temp(&dir);
        return;
    };
    let s: Value = serde_json::from_str(
        &d.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert!(s["error"].is_null(), "{s}");
    // The rectangle fixed; the arc keeps its radius and its opening.
    assert_eq!(s["dof"], 2, "{s}");
    // The Symmetry of the arc's ends (k9): redundant in part, one degree
    // of freedom still taken away.
    assert_eq!(s["redundant"], json!(["k9"]), "{s}");
    assert_eq!(s["partly_redundant"], json!({"k9": 1}), "{s}");
    for w in s["warning_messages"].as_array().unwrap() {
        assert!(!w.as_str().unwrap().contains("follow"), "{w}: {s}");
    }
    let notices = s["notices"].as_array().unwrap();
    assert!(
        notices
            .iter()
            .all(|n| n["level"] != "warning" && n["level"] != "error"),
        "{s}"
    );
    let info: Vec<&Value> = notices.iter().filter(|n| n["level"] == "info").collect();
    assert_eq!(info.len(), 1, "{s}");
    let message = info[0]["message"].as_str().unwrap();
    assert!(
        message.starts_with("the Symmetry")
            && message
                .ends_with("partly repeats the others; it still removes one degree of freedom"),
        "{message}"
    );
    assert_eq!(info[0]["target"], "k9", "{s}");
    remove_temp(&dir);
}
