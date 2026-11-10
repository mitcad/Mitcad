// SPDX-License-Identifier: MIT
//! Auto Fix (mitcad#1223) with the mock kernel: a region found as another
//! and as the sum of two, a renamed face, a face the fix renames followed
//! in a later feature, and an ambiguous reference that is left with its
//! error. After Auto Fix the definitions name what was found, the bodies
//! are the same (volume, faces), no warning is left, and one undo restores
//! them.

use serde_json::{Value, json};

use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The front face of the block: line c1 between c4 and c2.
const FRONT: &str = "F2:side(c1[c4,c2])";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn status(doc: &Document<MockKernel>, uid: u64) -> FeatureStatus {
    doc.status(FeatureUid(uid)).cloned().expect("recomputed")
}

fn warnings(doc: &Document<MockKernel>, uid: u64) -> Vec<String> {
    doc.warnings(FeatureUid(uid)).to_vec()
}

/// A feature's definition as the `feature` query gives it.
fn def(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "feature", "uid": uid}))["def"].clone()
}

/// The timeline entry of a feature.
fn timeline_entry(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == json!(uid))
        .cloned()
        .unwrap_or_else(|| panic!("no {uid} in the timeline"))
}

/// The bodies at the marker: uid, volume and the number of faces.
fn bodies(doc: &Document<MockKernel>) -> Vec<(String, f64, usize)> {
    doc.bodies()
        .iter()
        .map(|b| {
            let volume = doc.kernel().mass_properties(b.shape).unwrap().volume;
            (b.uid.to_string(), volume, b.shape.faces.len())
        })
        .collect()
}

/// The keys a region warning says the reference was found as, the main
/// one first.
fn found_keys(warning: &str) -> Vec<String> {
    let (_, rest) = warning
        .split_once(" found as ")
        .expect("a found-as warning");
    let (found, _) = rest.split_once(" after a change in ").unwrap();
    found.split(" + ").map(str::to_owned).collect()
}

fn profile(region: &str) -> Value {
    json!({"sketch": "F1", "region": region})
}

/// Sketch1 (F1) with a 40 x 10 rectangle (c1..c4), the kernel measuring.
fn plate() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 40, "height": 10}),
    );
    doc
}

fn extrude(doc: &mut Document<MockKernel>, profiles: Value) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude", "profiles": profiles,
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    assert_eq!(status(doc, 2), FeatureStatus::Ok);
}

fn auto_fix(doc: &mut Document<MockKernel>, features: &[&str]) -> Value {
    command(doc, json!({"cmd": "auto_fix", "features": features}))
}

/// Sketch1 (F1) with a 60 x 40 rectangle (c1..c4) extruded by Extrude1
/// (F2).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    extrude(&mut doc, json!([profile(RECT)]));
    doc
}

fn round_corner(doc: &mut Document<MockKernel>, a: &str, b: &str) {
    command(
        doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": a, "b": b, "radius": 3}),
    );
}

/// The one side face of Extrude1 (F2) on `curve` in the body.
fn side(doc: &Document<MockKernel>, curve: &str) -> String {
    let prefix = format!("F2:side({curve}[");
    let found: Vec<String> = doc
        .bodies()
        .iter()
        .flat_map(|b| b.shape.faces.iter().map(ToString::to_string))
        .filter(|f| f.starts_with(&prefix))
        .collect();
    assert_eq!(found.len(), 1, "{prefix}: {found:?}");
    found[0].clone()
}

/// A region a sketch change renamed (the left half of the plate, whose
/// dividing line is deleted) is found as the whole plate: Auto Fix writes
/// the plate's key.
#[test]
fn a_region_found_as_another_takes_its_key() {
    let mut doc = plate();
    let line = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, 0], "end": [20, 10]}),
    );
    let left: Vec<String> = query(&doc, json!({"query": "profiles"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["region"].as_str().unwrap().to_owned())
        .filter(|r| r.contains("c4["))
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
    let left = left[0].clone();
    extrude(&mut doc, json!([profile(&left)]));
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [line["made"][0]]}),
    );
    let regions = query(&doc, json!({"query": "profiles"}));
    assert_eq!(regions.as_array().unwrap().len(), 1, "{regions}");
    let whole = regions[0]["region"].as_str().unwrap().to_owned();
    let said = format!("region {left} found as {whole} after a change in Sketch1");
    assert_eq!(warnings(&doc, 2), [said.as_str()]);
    assert_eq!(timeline_entry(&doc, "F2")["auto_fix"], json!(1));
    let before = bodies(&doc);
    let depth = doc.undo_depth();

    let report = auto_fix(&mut doc, &["F2"]);
    assert_eq!(
        report["message"], "Auto Fix: 1 of 1 warning fixed",
        "{report}"
    );
    assert_eq!(report["fixed"], 1);
    assert_eq!(report["changed"], json!(["F2"]));
    assert_eq!(report["left"], json!([]));
    assert_eq!(report["label"], "Auto Fix Extrude1");
    assert_eq!(def(&doc, "F2")["profiles"], json!([profile(&whole)]));
    assert_eq!(status(&doc, 2), FeatureStatus::Ok);
    assert!(warnings(&doc, 2).is_empty());
    assert_eq!(timeline_entry(&doc, "F2")["auto_fix"], Value::Null);
    assert_eq!(bodies(&doc), before);
    // One undo step, which restores the reference and its warning.
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo_label(), Some("Auto Fix Extrude1"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(def(&doc, "F2")["profiles"], json!([profile(&left)]));
    assert_eq!(warnings(&doc, 2), [said.as_str()]);
    assert_eq!(bodies(&doc), before);
}

/// The plate split by a line: its reference is found as the two pieces,
/// and Auto Fix lists both, the main one first, as the computation used
/// them; the mock kernel's body per piece keeps its number.
#[test]
fn a_region_found_as_the_sum_of_two_lists_both() {
    let mut doc = plate();
    extrude(&mut doc, json!([profile(RECT)]));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [30, 0], "end": [30, 10]}),
    );
    let said = warnings(&doc, 2);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with(&format!("region {RECT} found as r{{")),
        "{said:?}"
    );
    let pieces = found_keys(&said[0]);
    assert_eq!(pieces.len(), 2, "{said:?}");
    let before = bodies(&doc);
    assert_eq!(before.len(), 2, "a body per piece: {before:?}");
    let depth = doc.undo_depth();

    let report = auto_fix(&mut doc, &["F2"]);
    assert_eq!(
        report["message"], "Auto Fix: 1 of 1 warning fixed",
        "{report}"
    );
    let expected: Vec<Value> = pieces.iter().map(|k| profile(k.as_str())).collect();
    assert_eq!(def(&doc, "F2")["profiles"], json!(expected));
    assert!(warnings(&doc, 2).is_empty());
    assert_eq!(bodies(&doc), before);
    assert_eq!(doc.undo_depth(), depth + 1);

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(def(&doc, "F2")["profiles"], json!([profile(RECT)]));
    assert_eq!(warnings(&doc, 2), said);
    // Redone, it is fixed again.
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(def(&doc, "F2")["profiles"], json!(expected));
    assert!(warnings(&doc, 2).is_empty());
}

/// The shell's removed face renamed by a rounded corner: Auto Fix writes
/// the new name, and the shelled body is the same.
#[test]
fn a_renamed_face_takes_its_new_name() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [FRONT], "inside": 2}}),
    );
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    round_corner(&mut doc, "c1", "c2");
    // The front face's name now, on the body before the shell.
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    let front = side(&doc, "c1");
    assert_ne!(front, FRONT);
    command(&mut doc, json!({"cmd": "set_marker", "position": 3}));
    let said = format!("face {FRONT} found as {front} after a change in Sketch1");
    assert_eq!(warnings(&doc, 3), [said.as_str()]);
    assert_eq!(timeline_entry(&doc, "F3")["auto_fix"], json!(1));
    let before = bodies(&doc);
    let extrusion = warnings(&doc, 2);
    let depth = doc.undo_depth();

    let report = auto_fix(&mut doc, &["F3"]);
    assert_eq!(
        report["message"], "Auto Fix: 1 of 1 warning fixed",
        "{report}"
    );
    assert_eq!(report["label"], "Auto Fix Shell1");
    assert_eq!(def(&doc, "F3")["faces"], json!([front]));
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    assert!(warnings(&doc, 3).is_empty());
    assert_eq!(bodies(&doc), before);
    // The extrusion was not asked for: it keeps what it says.
    assert_eq!(warnings(&doc, 2), extrusion);
    assert_eq!(doc.undo_depth(), depth + 1);

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(def(&doc, "F3")["faces"], json!([FRONT]));
    assert_eq!(warnings(&doc, 3), [said.as_str()]);
    assert_eq!(bodies(&doc), before);
}

/// The extrusion's region renamed: its end face takes the region's new
/// key, so the shell that removes that face finds it again, and the same
/// Auto Fix writes the face's new name into the shell too.
#[test]
fn a_face_the_fix_renames_is_followed_in_a_later_feature() {
    let mut doc = block();
    let top = format!("F2:end({RECT})");
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [top], "inside": 2}}),
    );
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    round_corner(&mut doc, "c1", "c2");
    let said = warnings(&doc, 2);
    assert_eq!(said.len(), 1, "{said:?}");
    let keys = found_keys(&said[0]);
    assert_eq!(keys.len(), 1, "{said:?}");
    // The main region carries the old key: the shell finds its face.
    assert!(warnings(&doc, 3).is_empty());
    let before = bodies(&doc);
    let depth = doc.undo_depth();

    let report = auto_fix(&mut doc, &["F2"]);
    assert_eq!(
        report["message"], "Auto Fix: 1 of 1 warning fixed; also updated: Shell1",
        "{report}"
    );
    assert_eq!(report["changed"], json!(["F2", "F3"]));
    assert_eq!(report["followed"], json!(["F3"]));
    assert_eq!(report["label"], "Auto Fix Extrude1");
    assert_eq!(def(&doc, "F2")["profiles"], json!([profile(&keys[0])]));
    assert_eq!(
        def(&doc, "F3")["faces"],
        json!([format!("F2:end({})", keys[0])])
    );
    for uid in [2, 3] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
        assert!(warnings(&doc, uid).is_empty(), "F{uid}");
    }
    assert_eq!(bodies(&doc).len(), before.len());
    for ((uid, volume, faces), (uid_before, volume_before, faces_before)) in
        bodies(&doc).iter().zip(&before)
    {
        assert_eq!((uid, faces), (uid_before, faces_before));
        assert!(
            (volume - volume_before).abs() < 1e-6,
            "{volume} {volume_before}"
        );
    }
    // One step undoes both.
    assert_eq!(doc.undo_depth(), depth + 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(def(&doc, "F2")["profiles"], json!([profile(RECT)]));
    assert_eq!(def(&doc, "F3")["faces"], json!([top]));
    assert_eq!(warnings(&doc, 2), said);
}

/// A reference that is ambiguous after a sketch change fails its feature:
/// Auto Fix leaves it, says what to do, and fixes what it can beside it.
#[test]
fn an_ambiguous_reference_is_left_with_its_error() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [FRONT], "inside": 2}}),
    );
    round_corner(&mut doc, "c4", "c1");
    round_corner(&mut doc, "c1", "c2");
    // A notch across c1 splits it into two pieces, each as like the
    // reference.
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, -5],
               "width": 20, "height": 10}),
    );
    let FeatureStatus::Failed(tie) = status(&doc, 3) else {
        panic!("the shell did not fail");
    };
    assert!(
        tie.contains(" is ambiguous after a sketch change: "),
        "{tie}"
    );
    let extrusion = warnings(&doc, 2);
    assert_eq!(extrusion.len(), 1, "{extrusion:?}");
    assert!(extrusion[0].starts_with("region "), "{extrusion:?}");
    let shell = def(&doc, "F3");

    // The shell alone: nothing to change, no undo step.
    let depth = doc.undo_depth();
    let report = auto_fix(&mut doc, &["F3"]);
    assert_eq!(
        report["message"],
        format!(
            "Auto Fix: 0 of 1 warning fixed; Shell1: {}, pick one of them again",
            mitcad_model::notice::user_message(tie.agent_text())
        ),
        "{report}"
    );
    assert_eq!(report["label"], Value::Null);
    assert_eq!(
        report["left"],
        json!([{"feature": "F3", "name": "Shell1", "message": tie.agent_text(),
                "advice": "pick one of them again"}])
    );
    assert_eq!(doc.undo_depth(), depth);

    // With the extrusion: its region is fixed, the shell is left.
    let report = auto_fix(&mut doc, &["F2", "F3"]);
    assert_eq!(
        report["message"],
        format!(
            "Auto Fix: 1 of 2 warnings fixed; Shell1: {}, pick one of them again",
            mitcad_model::notice::user_message(tie.agent_text())
        ),
        "{report}"
    );
    assert_eq!(report["label"], "Auto Fix Extrude1");
    assert!(warnings(&doc, 2).is_empty());
    assert_eq!(status(&doc, 3), FeatureStatus::Failed(tie.clone()));
    assert_eq!(def(&doc, "F3"), shell);
    assert_eq!(doc.undo_depth(), depth + 1);
}

/// Unknown features are refused, and Auto Fix of a design without
/// warnings changes nothing.
#[test]
fn nothing_to_fix_changes_nothing() {
    let mut doc = block();
    let depth = doc.undo_depth();
    let report = auto_fix(&mut doc, &[]);
    assert_eq!(report["message"], "Auto Fix: nothing to fix");
    assert_eq!(report["changed"], json!([]));
    assert_eq!(doc.undo_depth(), depth);
    let error = doc
        .command(&json!({"cmd": "auto_fix", "features": ["F9"]}).to_string())
        .unwrap_err();
    assert!(
        error.to_string().contains("feature F9 does not exist"),
        "{error}"
    );
}
