// SPDX-License-Identifier: MIT
//! Work recovered from an autosave keeps what its file has that this build
//! does not read (mitcad#2587): the autosaved text has neither the fields a
//! newer MitCad wrote (mitcad#1132) nor the CAM section that could not be
//! read (mitcad#2581), so the application has the document keep them from
//! its file (`keep_fields_not_read`), and a save over that file drops
//! them only when asked, as for the design read from it.

use serde_json::{Value, json};

use mitcad_model::file::FileFormat;
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model::{Document, test_support::block};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|answer| serde_json::from_str(&answer).unwrap())
        .map_err(|e| e.to_string())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

const REASON: &str = "two setups or operations have the same id";

/// A block's design file with a CAM section this build cannot read (two
/// setups with one id) and a field a newer MitCad wrote.
fn file_not_read() -> String {
    let mut value: Value = serde_json::from_str(&block().doc.to_json()).unwrap();
    value["cam"] = json!({"version": 1, "setups": [
        {"id": 1, "name": "Setup 1"}, {"id": 1, "name": "Setup 2"}]});
    value["future_notes"] = json!([{"id": "N1", "text": "Check the wall"}]);
    value.to_string()
}

#[test]
fn recovered_work_keeps_what_its_file_has_that_was_not_read() {
    let dir = unique_temp_dir("recovered-fields-not-read");
    let path = dir.join("cam.mitcad");
    let other = dir.join("copy.mitcad");
    std::fs::write(&path, file_not_read()).unwrap();
    let opened = Document::load_design(&path, MockKernel::default()).unwrap();
    assert_eq!(
        opened.fields_dropped_by_saving(&path),
        ["cam", "future_notes"]
    );

    // The autosaved text has neither: read back, the recovered document
    // would save over the file unasked.
    let mut recovered = Document::from_json(&opened.to_json(), MockKernel::default()).unwrap();
    assert!(recovered.load_warnings().is_empty());
    assert!(recovered.fields_dropped_by_saving(&path).is_empty());

    // Kept from its file: the save over it drops them, as for the design
    // read from it, with the same words; another file drops nothing.
    let before = recovered.revision();
    let answer = command(
        &mut recovered,
        json!({"cmd": "keep_fields_not_read", "path": path.display().to_string()}),
    )
    .unwrap();
    assert_eq!(answer["fields"], json!(["cam", "future_notes"]));
    assert_eq!(
        answer["unreadable"],
        json!([{"field": "cam", "reason": REASON}])
    );
    let absolute = std::path::absolute(&path).unwrap();
    assert_eq!(answer["file"], json!(absolute.display().to_string()));
    assert_eq!(recovered.unknown_fields(), opened.unknown_fields());
    assert_eq!(recovered.revision(), before, "no change of the design");
    assert_eq!(recovered.undo_depth(), 0, "no undo step");
    let saving = |doc: &Document<MockKernel>, path: &std::path::Path| {
        query(
            doc,
            json!({"query": "unknown_fields", "path": path.display().to_string()}),
        )
    };
    assert_eq!(saving(&recovered, &path), saving(&opened, &path));
    assert!(recovered.fields_dropped_by_saving(&other).is_empty());

    // Saved over the file (Save and Drop Them): the file has neither, and
    // the document keeps nothing more.
    recovered.save_file(&path, FileFormat::Single).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(written.get("cam").is_none() && written.get("future_notes").is_none());
    assert!(recovered.fields_dropped_by_saving(&path).is_empty());

    // A file that reads keeps nothing; a missing one has nothing to drop.
    let mut again = Document::from_json(&opened.to_json(), MockKernel::default()).unwrap();
    let answer = command(
        &mut again,
        json!({"cmd": "keep_fields_not_read", "path": path.display().to_string()}),
    )
    .unwrap();
    assert_eq!(answer["fields"], json!([]));
    let missing = dir.join("missing.mitcad");
    let answer = command(
        &mut again,
        json!({"cmd": "keep_fields_not_read", "path": missing.display().to_string()}),
    )
    .unwrap();
    assert_eq!(answer["fields"], json!([]));
    assert_eq!(answer["file"], Value::Null);

    // Text that is no design file this build opens is refused, and the
    // document keeps what it had.
    std::fs::write(&other, file_not_read()).unwrap();
    command(
        &mut again,
        json!({"cmd": "keep_fields_not_read", "path": other.display().to_string()}),
    )
    .unwrap();
    std::fs::write(&path, "{ not json").unwrap();
    let refused = command(
        &mut again,
        json!({"cmd": "keep_fields_not_read", "path": path.display().to_string()}),
    )
    .unwrap_err();
    assert!(refused.contains("not valid JSON"), "{refused}");
    assert_eq!(
        again.fields_dropped_by_saving(&other),
        ["cam", "future_notes"]
    );
    remove_temp(&dir);
}
