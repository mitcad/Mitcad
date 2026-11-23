// SPDX-License-Identifier: MIT
//! The timeline as a script and scripts run into a design (mitcad#2245):
//! `export_script` gives the steps that rebuild a design on an empty one
//! (features with their uids and names, parameters with their
//! expressions, components, groups, the marker and expectations), `upto`
//! stops at the marker or a feature, `omitted` names what a replay lacks,
//! a design that cannot be exported is refused in words; `add_feature`
//! with a `uid`; `run_script` as one undo step that a failing step leaves
//! undone; a split's pieces keep their ids in the replay (mitcad#2268).
//! Every CLI script's design (`tools/cli/tests`) is exported and
//! replayed into an equal design on the mock kernel; with the geometry
//! kernel the ctests `cli.script_round_trip.<script>` do the same.

use std::path::PathBuf;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::BoundingBox;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::classes::{ApiClass, command_class_with};
use mitcad_model_api::script_export::{export_script, round_trip, script_text};
use mitcad_model_api::{DocumentApi, ScriptApi};

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn kernel() -> MockKernel {
    let kernel = MockKernel::default();
    kernel.measures.set(true);
    kernel
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn uids(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["uid"].as_str().unwrap().to_owned())
        .collect()
}

fn extrude(distance: Value) -> Value {
    json!({"cmd": "add_feature", "def": {"type": "extrude",
           "profiles": [{"sketch": "F1", "region": REGION}],
           "extent": {"type": "distance", "distance": distance}, "operation": "new_body"}})
}

/// A user parameter `height`, Sketch1 (F1) with a rectangle, Extrude1
/// (F2) `height` high, Sketch2 (F3) deleted, a suppressed Extrude2 (F4)
/// that joins 5 mm more with its parameter named `extra`, a renamed body,
/// a timeline group and the marker before Extrude2.
fn part() -> Document<MockKernel> {
    let mut doc = Document::new(kernel());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "height", "expression": "20 mm",
               "comment": "the block's height"}),
    );
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    command(&mut doc, extrude(json!("height")));
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F3"}));
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": "extra = 5 mm"},
               "operation": "join"}}),
    );
    command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F4"}));
    command(
        &mut doc,
        json!({"cmd": "rename_body", "uid": "F2.b0", "name": "Block"}),
    );
    command(
        &mut doc,
        json!({"cmd": "group_features", "features": ["F1", "F2"], "name": "Base"}),
    );
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    doc
}

/// The exported script of a document, run on a new one.
fn replayed(doc: &Document<MockKernel>, upto: Option<&str>) -> Document<MockKernel> {
    let exported = export_script(doc, upto).unwrap();
    let mut replay = Document::new(kernel());
    replay
        .run_script(&script_text(&exported.script))
        .unwrap_or_else(|e| panic!("{e}\n{}", script_text(&exported.script)));
    replay
}

#[test]
fn a_feature_added_with_a_uid_keeps_it_and_counting_goes_on_after_it() {
    let mut doc = Document::new(kernel());
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "uid": "F5",
               "def": {"type": "sketch", "plane": "xy", "entities": []}}),
    );
    assert_eq!(added["uid"], "F5");
    // The next one counts on after it.
    command(&mut doc, json!({"cmd": "sketch.create"}));
    assert_eq!(uids(&doc), ["F5", "F6"]);
    // A gap below is free; a uid in use is refused.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "uid": "F2",
               "def": {"type": "sketch", "plane": "xy", "entities": []}}),
    );
    assert_eq!(uids(&doc), ["F5", "F6", "F2"]);
    let taken = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "uid": "F6",
               "def": {"type": "sketch", "plane": "xy", "entities": []}}),
    );
    assert!(taken.contains("has the uid F6 already"), "{taken}");
    command(&mut doc, json!({"cmd": "sketch.create"}));
    assert_eq!(uids(&doc), ["F5", "F6", "F2", "F7"]);
    // Undone, the uid is free again and counting starts where it did.
    for _ in 0..4 {
        command(&mut doc, json!({"cmd": "undo"}));
    }
    assert!(uids(&doc).is_empty());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    assert_eq!(uids(&doc), ["F1"]);
}

#[test]
fn an_exported_design_replays_into_an_equal_design() {
    let doc = part();
    let exported = export_script(&doc, None).unwrap();
    let text = script_text(&exported.script);
    assert_eq!(exported.features, 3);
    assert!(exported.left_out.is_empty());
    // The steps a person reads: the user parameter, the features with
    // their uids and names, the slot's parameter named, the group, the
    // marker, the expectations.
    for wanted in [
        r#""cmd":"add_parameter""#,
        r#""name":"height""#,
        r#""uid":"F4""#,
        r#""extra = 5 mm""#,
        r#""cmd":"suppress_feature""#,
        r#""cmd":"rename_body""#,
        r#""cmd":"group_features""#,
        r#""cmd":"set_marker","position":2"#,
        r#""expect":{"bodies":1}"#,
    ] {
        assert!(text.contains(wanted), "{wanted} in\n{text}");
    }
    // The deleted Sketch2 leaves a gap the replay keeps.
    let replay = replayed(&doc, None);
    assert_eq!(uids(&replay), ["F1", "F2", "F4"]);
    let mut fresh = Document::new(kernel());
    let found = round_trip(&doc, &mut fresh).unwrap();
    assert_eq!(found.skipped, None);
    assert!(
        found.report.starts_with("Round trip: 3 features"),
        "{}",
        found.report
    );
    let body = &query(&replay, json!({"query": "bodies"}))[0];
    assert_eq!(body["name"], "Block");
    let groups = query(&replay, json!({"query": "timeline"}))["groups"].clone();
    assert_eq!(groups[0]["name"], "Base");
}

#[test]
fn upto_stops_at_the_marker_or_at_a_feature() {
    let doc = part();
    // The marker is after Extrude1: Extrude2 stays out.
    let at_marker = export_script(&doc, Some("marker")).unwrap();
    assert_eq!(at_marker.features, 2);
    let replay = replayed(&doc, Some("marker"));
    assert_eq!(uids(&replay), ["F1", "F2"]);
    // A feature by its name or its uid: it and the ones before it.
    for upto in ["Sketch1", "F1"] {
        let exported = export_script(&doc, Some(upto)).unwrap();
        assert_eq!(exported.features, 1, "{upto}");
        let replay = replayed(&doc, Some(upto));
        assert_eq!(uids(&replay), ["F1"], "{upto}");
        assert_eq!(
            query(&replay, json!({"query": "bodies"})),
            json!([]),
            "{upto}"
        );
    }
    let unknown = export_script(&doc, Some("Loft7")).unwrap_err().to_string();
    assert!(unknown.contains("no feature Loft7"), "{unknown}");
}

#[test]
fn omitted_names_an_image_reference_in_words() {
    let mut doc = part();
    command(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": bmp_data(), "file_name": "plan.bmp",
               "plane": "xy"}),
    );
    let exported = export_script(&doc, None).unwrap();
    assert!(
        exported
            .omitted
            .iter()
            .any(|line| line == "image references (1)"),
        "{:?}",
        exported.omitted
    );
    // The script says so too.
    assert!(
        exported.script[1]["comment"]
            .as_str()
            .unwrap()
            .contains("image references (1)")
    );
    // Through the command: in the reply.
    let reply = command(&mut doc, json!({"cmd": "export_script"}));
    assert!(
        reply["omitted"]
            .as_array()
            .unwrap()
            .contains(&json!("image references (1)"))
    );
    assert_eq!(reply["features"], 3);
    assert!(reply["script"].is_array());
}

/// A small BMP image as base64.
fn bmp_data() -> String {
    let (width, height) = (200u32, 100u32);
    let row = (width * 3).div_ceil(4) * 4;
    let pixels = row * height;
    let mut bytes = b"BM".to_vec();
    bytes.extend_from_slice(&(54 + pixels).to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 24, 0]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&pixels.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.resize(bytes.len() + pixels as usize, 0x80);
    mitcad_model::internal::base64::encode(&bytes)
}

#[test]
fn components_replay_with_their_uids_and_a_deleted_one_is_refused_in_words() {
    let mut doc = Document::new(kernel());
    command(&mut doc, json!({"cmd": "create_component", "name": "Arm"}));
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    command(&mut doc, extrude(json!(20)));
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1",
               "transform": [[1, 0, 0, 100], [0, 1, 0, 0], [0, 0, 1, 0]]}),
    );
    let exported = export_script(&doc, None).unwrap();
    let text = script_text(&exported.script);
    assert!(text.contains(r#""cmd":"create_component""#), "{text}");
    assert!(text.contains(r#""component":"C1""#), "{text}");
    assert!(text.contains(r#""cmd":"copy_occurrence""#), "{text}");
    let mut fresh = Document::new(kernel());
    let found = round_trip(&doc, &mut fresh).unwrap();
    assert_eq!(found.skipped, None, "{text}");
    // The body in both occurrences, the copy as O2.
    let instances = query(&fresh, json!({"query": "instances"}));
    assert_eq!(instances.as_array().unwrap().len(), 2, "{instances}");
    assert!(instances.to_string().contains("O2"), "{instances}");

    // A component made and deleted before another: the replay could not
    // give the later one its uid.
    let mut doc = Document::new(kernel());
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Gone", "activate": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Kept", "activate": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": "O1"}),
    );
    let refused = export_script(&doc, None).unwrap_err().to_string();
    assert!(
        refused.starts_with("cannot export the timeline as a script: "),
        "{refused}"
    );
    let through_command = rejected(&mut doc, json!({"cmd": "export_script"}));
    assert!(
        through_command.contains("cannot export the timeline as a script"),
        "{through_command}"
    );
}

#[test]
fn a_design_with_sketch_blocks_is_refused_in_words() {
    let mut doc = Document::new(kernel());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [10, 10],
               "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_sketch_block", "sketch": "F1", "items": ["c1", "c2", "c3", "c4"],
               "base": [10, 10]}),
    );
    let refused = export_script(&doc, None).unwrap_err().to_string();
    assert_eq!(
        refused,
        "cannot export the timeline as a script: the design has sketch blocks, which a script \
         cannot make again"
    );
}

#[test]
fn run_script_is_one_undo_step_and_a_failing_step_changes_nothing() {
    let mut doc = Document::new(kernel());
    let steps = json!([
        {"comment": "A block."},
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
        extrude(json!(20)),
        {"expect": {"bodies": 1}}
    ]);
    let ran = command(
        &mut doc,
        json!({"cmd": "run_script", "steps": steps, "label": "Run block"}),
    );
    assert_eq!(ran["steps"], 5);
    assert_eq!(ran["results"].as_array().unwrap().len(), 3);
    assert_eq!(uids(&doc), ["F1", "F2"]);
    assert_eq!(doc.undo_label(), Some("Run block"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(uids(&doc).is_empty());
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(uids(&doc), ["F1", "F2"]);

    // The third step names a sketch that is not there: the first two are
    // taken back, and the error gives the step's number.
    let depth = doc.undo_depth();
    let failed = rejected(
        &mut doc,
        json!({"cmd": "run_script", "steps": [
            {"cmd": "sketch.create"},
            {"comment": "Then a feature on a sketch that does not exist."},
            {"cmd": "sketch.add_circle", "sketch": "F9", "center": [0, 0], "diameter": 5}
        ]}),
    );
    assert!(failed.starts_with("Step 3: "), "{failed}");
    assert_eq!(uids(&doc), ["F1", "F2"]);
    assert_eq!(doc.undo_depth(), depth);
    // A failing expectation stops it the same way.
    let failed = rejected(
        &mut doc,
        json!({"cmd": "run_script", "steps": [
            {"cmd": "sketch.create"}, {"expect": {"bodies": 7}}
        ]}),
    );
    assert!(failed.starts_with("Step 2: expected 7 bodies"), "{failed}");
    assert_eq!(uids(&doc), ["F1", "F2"]);
    // A step that undoes what was there before the script.
    let failed = rejected(
        &mut doc,
        json!({"cmd": "run_script", "steps": [{"cmd": "undo"}]}),
    );
    assert!(failed.starts_with("Step 1: "), "{failed}");
    assert_eq!(uids(&doc), ["F1", "F2"]);
    // Without a label its undo step is Run Script.
    command(
        &mut doc,
        json!({"cmd": "run_script", "steps": [{"cmd": "sketch.create"}]}),
    );
    assert_eq!(doc.undo_label(), Some("Run Script"));
}

#[test]
fn scripts_are_not_offered_to_external_clients() {
    for request in [
        json!({"cmd": "export_script"}),
        json!({"cmd": "export_script", "path": "out.json"}),
        json!({"cmd": "run_script", "steps": []}),
    ] {
        assert_ne!(
            command_class_with(&request),
            Some(ApiClass::Model),
            "{request}"
        );
    }
}

/// The CLI's scripts (`tools/cli/tests/*.json`).
fn cli_scripts() -> Vec<PathBuf> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/cli/tests");
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    scripts.sort();
    scripts
}

/// A CLI script's design on the mock kernel: its model commands in turn up
/// to the first one the mock kernel refuses. Expectations (the geometry
/// kernel's numbers) and commands that read or write files are passed
/// over. None for a file that is no script.
fn mock_design(path: &PathBuf) -> Option<Document<MockKernel>> {
    let text = std::fs::read_to_string(path).ok()?;
    let Ok(Value::Array(steps)) = serde_json::from_str::<Value>(&text) else {
        return None;
    };
    let mut doc = Document::new(kernel());
    for step in steps {
        if step.get("cmd").is_none() {
            continue;
        }
        if command_class_with(&step) != Some(ApiClass::Model) {
            continue;
        }
        if doc.command(&step.to_string()).is_err() {
            break;
        }
    }
    Some(doc)
}

#[test]
fn every_cli_scripts_design_is_exported_and_replayed_into_an_equal_design() {
    let (mut replayed, mut skipped, mut refused) = (0, 0, 0);
    let mut failures = Vec::new();
    for path in cli_scripts() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some(doc) = mock_design(&path) else {
            continue;
        };
        let mut fresh = Document::new(kernel());
        match round_trip(&doc, &mut fresh) {
            Ok(found) if found.skipped.is_some() => skipped += 1,
            Ok(_) => replayed += 1,
            Err(e)
                if e.to_string()
                    .starts_with("cannot export the timeline as a script: ") =>
            {
                refused += 1;
            }
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    // The counts for the change's notes: `cargo test -- --nocapture`.
    println!(
        "CLI scripts' designs: {replayed} replayed equal, {skipped} with imported bodies left \
         out, {refused} refused in words"
    );
    assert!(replayed > 100, "only {replayed} replayed");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The cut's pieces of the block as the mock leaves them (bounding boxes
/// in geometric order), moved `dx` along x: the main piece and two
/// slivers.
fn cut_pieces(dx: f64, order: [usize; 3]) -> Vec<BoundingBox> {
    let boxes = [
        BoundingBox {
            min: [dx, 0.0, 0.0],
            max: [50.0 + dx, 40.0, 20.0],
        },
        BoundingBox {
            min: [54.0 + dx, 0.0, 0.0],
            max: [55.0 + dx, 1.0, 20.0],
        },
        BoundingBox {
            min: [57.0 + dx, 39.0, 0.0],
            max: [58.0 + dx, 40.0, 20.0],
        },
    ];
    order.iter().map(|i| boxes[*i]).collect()
}

#[test]
fn split_pieces_keep_their_ids_in_the_replay() {
    // A block (F2.b0) cut into three pieces by Extrude2 (F3): the main
    // piece keeps the block's id, the slivers are F3.b0 and F3.b1. An
    // upstream edit brings the pieces in another geometric order (a sliver
    // first); the cut keeps the ids by what it remembers (mitcad#408).
    let mut doc = Document::new(kernel());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "depth", "expression": "30 mm"}),
    );
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    command(&mut doc, extrude(json!(20)));
    doc.kernel()
        .piece_boxes
        .replace(Some(cut_pieces(0.0, [0, 1, 2])));
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": "depth"},
               "operation": "cut", "participants": ["F2.b0"]}}),
    );
    doc.kernel()
        .piece_boxes
        .replace(Some(cut_pieces(1.0, [2, 0, 1])));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "depth", "expression": "25 mm"}),
    );

    // The script carries the cut's memory (mitcad#2268), so the replay's
    // first evaluation gives the block the main piece as the design does,
    // not the first piece in geometric order.
    let exported = export_script(&doc, None).unwrap();
    let text = script_text(&exported.script);
    assert!(text.contains(r#""pieces":[{"body":"F2.b0""#), "{text}");
    // The replay names the cut's new pieces in its own order (a sliver
    // first): its bodies pass through names no body has on their way to
    // the design's, so no rename meets a name the other piece holds.
    assert!(text.contains(r#" (F3.b0)""#), "{text}");
    let replay_kernel = || {
        let kernel = kernel();
        kernel.piece_boxes.replace(Some(cut_pieces(1.0, [2, 0, 1])));
        kernel
    };
    let mut fresh = Document::new(replay_kernel());
    let found = round_trip(&doc, &mut fresh).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(found.skipped, None);
    let cut: FeatureUid = "F3".parse().unwrap();
    assert_eq!(
        fresh.state().split_pieces_of(cut),
        doc.state().split_pieces_of(cut)
    );

    // Without the memory the replay numbers the pieces in geometric order:
    // the block is a sliver, which the script's expectations catch.
    let mut steps = exported.script.clone();
    for step in &mut steps {
        if let Some(fields) = step.as_object_mut() {
            fields.remove("pieces");
        }
    }
    let mut bare = Document::new(replay_kernel());
    let error = bare.run_script(&script_text(&steps)).unwrap_err();
    assert!(error.to_string().contains("F2.b0"), "{error}");

    // The memory goes with a uid, names only the body split and the
    // feature's own bodies, and its undo leaves nothing behind.
    let sketch = json!({"type": "sketch", "plane": "xy", "entities": []});
    let group = |piece: &str| {
        json!([{"body": "F2.b0", "size": 10.0,
                "pieces": [{"body": piece, "volume": 1.0, "centre": [0.0, 0.0, 0.0]}]}])
    };
    let without_uid = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "def": sketch, "pieces": group("F9.b0")}),
    );
    assert!(without_uid.contains("give uid too"), "{without_uid}");
    let foreign = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "uid": "F9", "def": sketch, "pieces": group("F5.b0")}),
    );
    assert!(foreign.contains("neither the body split"), "{foreign}");
    let nine: FeatureUid = "F9".parse().unwrap();
    assert_eq!(doc.state().split_pieces_of(nine), None);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "uid": "F9", "def": sketch, "pieces": group("F9.b0")}),
    );
    assert!(doc.state().split_pieces_of(nine).is_some());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(doc.state().split_pieces_of(nine), None);
    command(&mut doc, json!({"cmd": "redo"}));
    assert!(doc.state().split_pieces_of(nine).is_some());
}
