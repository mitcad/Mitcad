// SPDX-License-Identifier: MIT
//! Comments (mitcad#1119) on the mock kernel: added on every kind of
//! target, replied to, addressed, resolved, reopened and deleted, each an
//! undo step of its own; the `comments` query with its filters, `open` and
//! `revision`; saved and read back; orphaned when a feature removes the
//! face they are on, and retargeted; and a model change with its
//! `address_comment` in one array undone as one step.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::VertexName;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn comments(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "comments"}))
}

fn comment(doc: &Document<MockKernel>, id: &str) -> Value {
    comments(doc)["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no comment {id}"))
}

fn ids(answer: &Value) -> Vec<String> {
    answer["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect()
}

fn document(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    document(doc)["undo"].clone()
}

fn near(value: &Value, expected: [f64; 3]) {
    let found: Vec<f64> = value
        .as_array()
        .unwrap_or_else(|| panic!("no point: {value}"))
        .iter()
        .map(|c| c.as_f64().unwrap())
        .collect();
    assert_eq!(found.len(), 3, "{value}");
    for (f, e) in found.iter().zip(expected) {
        assert!((f - e).abs() <= 1e-6, "{value} is not {expected:?}");
    }
}

/// The top face of a 20 x 20 x 20 block (F2.b0) on XY.
fn top() -> Value {
    json!({"body": "F2.b0", "face": format!("F2:end({R})")})
}

fn bottom() -> Value {
    json!({"body": "F2.b0", "face": format!("F2:start({R})")})
}

/// Sketch1 (F1) with a 20 x 20 rectangle, Extrude1 (F2) 20 mm up: Body1
/// (F2.b0) from (0, 0, 0) to (20, 20, 20); and the parameter `width`.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "20 mm"}, "operation": "new_body"}},
            {"cmd": "add_parameter", "name": "width", "value": "20 mm"},
        ]),
    );
    doc
}

/// Shell1 (F3): Body1 hollowed with its top face removed.
fn shell_top(doc: &mut Document<MockKernel>) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [format!("F2:end({R})")], "inside": 2}}),
    );
}

fn add(doc: &mut Document<MockKernel>, text: &str, target: Value) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_comment", "text": text, "target": target}),
    );
    added["id"].as_str().unwrap().to_owned()
}

/// RFC 3339 UTC in whole seconds: `2026-11-01T17:59:00Z`.
fn is_utc(text: &Value) -> bool {
    let text = text.as_str().unwrap_or_default();
    let bytes = text.as_bytes();
    bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[10] == b'T'
        && bytes[19] == b'Z'
        && text[..4].chars().all(|c| c.is_ascii_digit())
}

/// Done when: add, reply, address, resolve, reopen and delete, each its own
/// undo step with its label; ids never given again; the filters, `open`.
#[test]
fn a_comment_is_added_replied_to_addressed_resolved_reopened_and_deleted() {
    let mut doc = block();
    let id = add(&mut doc, "  make this face flat to 0.05  ", top());
    assert_eq!(id, "C1");
    assert_eq!(undo_label(&doc), "Add Comment C1");
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["text"], "make this face flat to 0.05");
    assert_eq!(c1["target"], top());
    assert_eq!(c1["status"], "open");
    assert_eq!(c1["orphaned"], false);
    assert_eq!(c1["author"], "user");
    assert_eq!(c1["resolution"], Value::Null);
    assert_eq!(c1["replies"], json!([]));
    assert!(is_utc(&c1["created"]), "{c1}");
    assert_eq!(c1["label"], "Body1 › Extrude1 end face");
    near(&c1["place"], [10.0, 10.0, 20.0]);
    assert_eq!(comments(&doc)["open"], 1);

    command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "also the bottom one"}),
    );
    assert_eq!(undo_label(&doc), "Reply to C1");
    let replies = comment(&doc, "C1")["replies"].clone();
    assert_eq!(replies[0]["author"], "user");
    assert_eq!(replies[0]["text"], "also the bottom one");
    assert!(is_utc(&replies[0]["created"]));

    // An agent addresses it: the status, what it did and who.
    command(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C1", "resolution": "Flattened both faces",
               "author": "claude-code"}),
    );
    assert_eq!(undo_label(&doc), "Address C1");
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["status"], "addressed");
    assert_eq!(c1["resolution"], "Flattened both faces");
    assert_eq!(c1["addressed_by"], "claude-code");
    assert_eq!(comments(&doc)["open"], 0);
    let error = refused(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C1", "resolution": "  "}),
    );
    assert!(error.contains("needs a text"), "{error}");

    command(&mut doc, json!({"cmd": "resolve_comment", "id": "C1"}));
    assert_eq!(undo_label(&doc), "Resolve C1");
    assert_eq!(comment(&doc, "C1")["status"], "resolved");
    // Resolved again: no step; addressed only once reopened.
    let depth = document(&doc)["undo_depth"].clone();
    command(&mut doc, json!({"cmd": "resolve_comment", "id": "C1"}));
    assert_eq!(document(&doc)["undo_depth"], depth);
    let error = refused(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C1", "resolution": "Again"}),
    );
    assert!(error.contains("C1 is resolved: reopen it first"), "{error}");

    command(
        &mut doc,
        json!({"cmd": "reopen_comment", "id": "C1", "text": "the bottom is not flat yet"}),
    );
    assert_eq!(undo_label(&doc), "Reopen C1");
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["status"], "open");
    assert_eq!(c1["replies"].as_array().unwrap().len(), 2);
    assert_eq!(c1["replies"][1]["text"], "the bottom is not flat yet");
    // The last resolution stays until the next address replaces it.
    assert_eq!(c1["resolution"], "Flattened both faces");

    let c2 = add(&mut doc, "round these edges", json!({"feature": "F2"}));
    assert_eq!(c2, "C2");
    command(
        &mut doc,
        json!({"cmd": "edit_comment", "id": "C2", "text": "round these edges 2 mm"}),
    );
    assert_eq!(undo_label(&doc), "Edit Comment C2");
    let unchanged = command(
        &mut doc,
        json!({"cmd": "edit_comment", "id": "C2", "text": "round these edges 2 mm"}),
    );
    assert_eq!(unchanged["changed"], false);
    assert_eq!(undo_label(&doc), "Edit Comment C2");

    // The filters: by status (one or several) and by target; `open`
    // counts all.
    command(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C2", "resolution": "Fillet1, 2 mm"}),
    );
    let open = query(&doc, json!({"query": "comments", "status": "open"}));
    assert_eq!(ids(&open), ["C1"]);
    assert_eq!(open["open"], 1);
    let both = query(
        &doc,
        json!({"query": "comments", "status": ["open", "addressed"]}),
    );
    assert_eq!(ids(&both), ["C1", "C2"]);
    let on_feature = query(
        &doc,
        json!({"query": "comments", "target": {"feature": "F2"}}),
    );
    assert_eq!(ids(&on_feature), ["C2"]);
    assert_eq!(on_feature["open"], 1);

    command(&mut doc, json!({"cmd": "delete_comment", "id": "C2"}));
    assert_eq!(undo_label(&doc), "Delete Comment C2");
    assert_eq!(ids(&comments(&doc)), ["C1"]);
    // Its id is not given again.
    assert_eq!(add(&mut doc, "another", top()), "C3");
    for (command_value, expected) in [
        (
            json!({"cmd": "resolve_comment", "id": "C2"}),
            "there is no comment C2",
        ),
        (
            json!({"cmd": "delete_comment", "id": "x"}),
            "x is no comment id",
        ),
        (
            json!({"cmd": "add_comment", "text": " ", "target": top()}),
            "needs a text",
        ),
    ] {
        let error = refused(&mut doc, command_value);
        assert!(error.contains(expected), "{error}");
    }
}

/// Done when: each target kind, with its label and place now; a target
/// that is not there or is no target is refused with why.
#[test]
fn every_kind_of_target_is_found_with_its_label_and_place() {
    let mut doc = block();
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let sketch = &file["features"][0];
    let entity = |kind: &str| -> String {
        sketch["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == kind)
            .and_then(|e| e["id"].as_str())
            .unwrap()
            .to_owned()
    };
    let (curve, point) = (entity("line"), entity("point"));
    let constraint = sketch["constraints"][0].clone();
    let side = "F2:side(c1[c4,c2])";
    let edge = format!("E{{F2:end({R})|{side}}}");
    let vertex: VertexName = format!("V{{F2:end({R})|{side}|F2:side(c4[c3,c1])}}")
        .parse()
        .unwrap();
    doc.kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.to_string(), [0.0, 0.0, 20.0]);

    let curve_label = format!("Sketch1 › line {curve}");
    let point_label = format!("Sketch1 › point {point}");
    let constraint_label = format!(
        "Sketch1 › {} {}",
        constraint["type"].as_str().unwrap(),
        constraint["id"].as_str().unwrap()
    );
    let cases = [
        (
            json!({"feature": "F2"}),
            "Extrude1",
            Some([10.0, 10.0, 10.0]),
        ),
        (json!({"body": "F2.b0"}), "Body1", Some([10.0, 10.0, 10.0])),
        (top(), "Body1 › Extrude1 end face", Some([10.0, 10.0, 20.0])),
        (json!({"body": "F2.b0", "edge": edge}), "Body1 › edge", None),
        (
            json!({"body": "F2.b0", "vertex": vertex.to_string()}),
            "Body1 › vertex",
            Some([0.0, 0.0, 20.0]),
        ),
        (
            json!({"sketch": "F1", "curve": curve}),
            curve_label.as_str(),
            None,
        ),
        (
            json!({"sketch": "F1", "point": point}),
            point_label.as_str(),
            None,
        ),
        (
            json!({"sketch": "F1", "constraint": constraint["id"]}),
            constraint_label.as_str(),
            None,
        ),
        (json!({"parameter": "width"}), "width", None),
        (
            json!({"at": [5, 5, 20], "component": "", "occurrence": ""}),
            "Point",
            Some([5.0, 5.0, 20.0]),
        ),
    ];
    for (i, (target, label, place)) in cases.iter().enumerate() {
        let id = add(&mut doc, &format!("comment {i}"), target.clone());
        let found = comment(&doc, &id);
        assert_eq!(found["label"], *label, "{found}");
        assert_eq!(found["orphaned"], false, "{found}");
        match place {
            Some(place) => near(&found["place"], *place),
            // A parameter has no place; the others' depend on the kernel.
            None if target.get("parameter").is_some() => {
                assert_eq!(found["place"], Value::Null, "{found}")
            }
            None => {}
        }
    }
    // Sketch entities sit where the sketch solved them.
    let on_curve = comment(&doc, "C6");
    assert!(on_curve["place"].is_array(), "{on_curve}");
    // A point's target is kept as stored: the root's component left out.
    assert_eq!(
        comment(&doc, "C10")["target"],
        json!({"at": [5.0, 5.0, 20.0]})
    );
    // A dimension, when the rectangle has one.
    if let Some(dimension) = sketch["dimensions"][0]["id"].as_str() {
        let id = add(
            &mut doc,
            "a dimension",
            json!({"sketch": "F1", "dimension": dimension}),
        );
        assert_eq!(
            comment(&doc, &id)["label"],
            format!("Sketch1 › dimension {dimension}")
        );
    }

    for (target, expected) in [
        (json!({"feature": "F9"}), "the feature F9 is gone"),
        (json!({"body": "F7.b0"}), "the body F7.b0 is gone"),
        (
            json!({"body": "F2.b0", "face": "F2:end(r{c9})"}),
            "the face F2:end(r{c9}) is gone from Body1",
        ),
        (
            json!({"sketch": "F1", "curve": "c99"}),
            "c99 is gone from Sketch1",
        ),
        (
            json!({"sketch": "F2", "curve": "c1"}),
            "Extrude1 is no sketch",
        ),
        (json!({"parameter": "depth"}), "the parameter depth is gone"),
        (
            json!({"at": [0, 0, 0], "component": "C5"}),
            "the component C5 is gone",
        ),
        (json!({"feature": "F2", "body": "F2.b0"}), "a target is"),
        (json!({"sketch": "F1", "curve": "p5"}), "is no curve"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_comment", "text": "x", "target": target}),
        );
        assert!(error.contains(expected), "{target}: {error}");
        assert!(error.starts_with("the comment's target: "), "{error}");
    }
    let error = refused(
        &mut doc,
        json!({"cmd": "add_comment", "text": "x", "target": {"face": "F2:end(r{c1})", "colour": 1}}),
    );
    assert!(error.contains("colour"), "{error}");
}

/// Done when: undo and redo take every comment change back and forth, and
/// `revision` grows with each change (also undo and redo) and stays while
/// nothing changes.
#[test]
fn undo_and_redo_take_comment_changes_back_and_the_revision_follows() {
    let mut doc = block();
    let revision = |doc: &Document<MockKernel>| comments(doc)["revision"].as_u64().unwrap();
    let r0 = revision(&doc);
    assert_eq!(revision(&doc), r0, "nothing changed");
    add(&mut doc, "make this hole an M6 thread", top());
    let r1 = revision(&doc);
    assert!(r1 > r0);
    command(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C1", "resolution": "Thread M6x1"}),
    );
    let r2 = revision(&doc);
    assert!(r2 > r1);
    assert_eq!(revision(&doc), r2);

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(comment(&doc, "C1")["status"], "open");
    let r3 = revision(&doc);
    assert!(r3 > r2);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(ids(&comments(&doc)).is_empty());
    assert_eq!(comments(&doc)["open"], 0);
    assert!(revision(&doc) > r3);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(comment(&doc, "C1")["status"], "open");
    command(&mut doc, json!({"cmd": "redo"}));
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["status"], "addressed");
    assert_eq!(c1["resolution"], "Thread M6x1");
    // A comment change recomputes nothing.
    let before = doc.recompute_count();
    command(&mut doc, json!({"cmd": "resolve_comment", "id": "C1"}));
    assert_eq!(doc.recompute_count(), before);
}

/// Done when: saved and read back the same; ids go on after the largest
/// ever used; a design without comments keeps no field of them; a broken
/// file is refused.
#[test]
fn comments_are_saved_and_read_back() {
    let mut doc = block();
    let saved_without = doc.to_json();
    let file: Value = serde_json::from_str(&saved_without).unwrap();
    assert!(file.get("comments").is_none() && file.get("next_comment").is_none());

    add(&mut doc, "make this face flat", top());
    add(&mut doc, "a free point", json!({"at": [5, 5, 20]}));
    add(&mut doc, "largest", json!({"parameter": "width"}));
    command(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C1", "resolution": "Flat", "author": "claude-code"}),
    );
    command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C2", "text": "why here?", "author": "claude-code"}),
    );
    command(&mut doc, json!({"cmd": "delete_comment", "id": "C3"}));
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["comments"].as_array().unwrap().len(), 2);
    assert_eq!(file["comments"][0]["id"], "C1");
    assert_eq!(file["comments"][0]["status"], "addressed");
    assert_eq!(file["comments"][0]["addressed_by"], "claude-code");
    near(&file["comments"][0]["place"], [10.0, 10.0, 20.0]);
    assert!(file["comments"][0].get("orphaned").is_none());
    // C3 was the largest: the next is kept.
    assert_eq!(file["next_comment"], 4);

    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    loaded.recompute();
    assert_eq!(loaded.to_json(), saved);
    let listed = |doc: &Document<MockKernel>| comments(doc)["comments"].clone();
    assert_eq!(listed(&loaded), listed(&doc));
    assert_eq!(add(&mut loaded, "next", top()), "C4");

    // Refused: an id twice, a field it does not know, an empty text.
    let twice = saved.replace("\"id\": \"C2\"", "\"id\": \"C1\"");
    assert_ne!(twice, saved);
    let error = Document::from_json(&twice, MockKernel::default())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("comment C1 is listed more than once"),
        "{error}"
    );
    let unknown = saved.replace("\"text\": \"a free point\"", "\"text\": \"a\", \"pin\": 1");
    assert_ne!(unknown, saved);
    assert!(Document::from_json(&unknown, MockKernel::default()).is_err());
    let empty = saved.replace("\"text\": \"a free point\"", "\"text\": \" \"");
    assert_ne!(empty, saved);
    let error = Document::from_json(&empty, MockKernel::default())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("comments[1] (C2): the text is empty"),
        "{error}"
    );
}

/// Done when: a feature that removes the face makes the comment orphaned at
/// its last known place (also in the file), undo finds it again, and
/// retargeting clears it.
#[test]
fn a_comment_whose_face_a_feature_removes_is_orphaned_and_retargeted() {
    let mut doc = block();
    add(&mut doc, "chamfer this face's edges", top());
    shell_top(&mut doc);
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["orphaned"], true, "{c1}");
    assert!(
        c1["error"].as_str().unwrap().contains("is gone from Body1"),
        "{c1}"
    );
    assert_eq!(c1["label"], "Body1 › Extrude1 end face");
    near(&c1["place"], [10.0, 10.0, 20.0]);
    assert_eq!(c1["status"], "open");
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["comments"][0]["orphaned"], true);
    near(&file["comments"][0]["place"], [10.0, 10.0, 20.0]);
    // An orphan cannot be the target of a new comment, but stays listed.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_comment", "text": "x", "target": top()}),
    );
    assert!(error.contains("is gone"), "{error}");

    // Without the shell the face is there again; with it, gone again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(comment(&doc, "C1")["orphaned"], false);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(comment(&doc, "C1")["orphaned"], true);

    // Retargeted onto the bottom face: no orphan, its place there.
    let error = refused(
        &mut doc,
        json!({"cmd": "retarget_comment", "id": "C1", "target": top()}),
    );
    assert!(error.contains("is gone"), "{error}");
    let retargeted = command(
        &mut doc,
        json!({"cmd": "retarget_comment", "id": "C1", "target": bottom()}),
    );
    assert_eq!(retargeted["changed"], true);
    assert_eq!(undo_label(&doc), "Retarget C1");
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["orphaned"], false);
    assert_eq!(c1["target"], bottom());
    assert!(c1.get("error").is_none(), "{c1}");
    near(&c1["place"], [10.0, 10.0, 0.0]);
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(file["comments"][0].get("orphaned").is_none());
    // Undo takes the orphan back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(comment(&doc, "C1")["orphaned"], true);
}

/// Done when: an agent's model change and its `address_comment` in one
/// `model_command` array (whose steps MCP merges with `merge_undo`) undo
/// and redo as one step.
#[test]
fn a_model_change_and_its_address_in_one_array_undo_together() {
    let mut doc = block();
    add(
        &mut doc,
        "hollow this block, 2 mm walls",
        json!({"feature": "F2"}),
    );
    let depth = document(&doc)["undo_depth"].as_u64().unwrap();
    let features = document(&doc)["features"].clone();
    command(
        &mut doc,
        json!([
            {"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
              "faces": [format!("F2:end({R})")], "inside": 2}},
            {"cmd": "address_comment", "id": "C1", "resolution": "Shell1, 2 mm walls, top open",
             "author": "claude-code"},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "merge_undo", "depth": depth, "label": "Address C1"}),
    );
    assert_eq!(document(&doc)["undo_depth"], depth + 1);
    assert_eq!(undo_label(&doc), "Address C1");
    assert_eq!(comment(&doc, "C1")["status"], "addressed");

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(document(&doc)["features"], features);
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["status"], "open");
    assert_eq!(c1["resolution"], Value::Null);
    assert_eq!(document(&doc)["undo_depth"], depth);

    command(&mut doc, json!({"cmd": "redo"}));
    assert_ne!(document(&doc)["features"], features);
    assert_eq!(comment(&doc, "C1")["status"], "addressed");
}
