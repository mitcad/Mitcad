// SPDX-License-Identifier: MIT
//! Comments (mitcad#1119) on the mock kernel: added on every kind of
//! target, replied to, addressed, resolved, reopened and deleted, each an
//! undo step of its own; the `comments` query with its filters, `open` and
//! `revision`; saved and read back; orphaned when a feature removes the
//! face they are on, and retargeted; a model change with its
//! `address_comment` in one array undone as one step; a comment on a
//! parameter following its renames (mitcad#1305); comments on a
//! deleted last feature staying orphaned after a reload (mitcad#1320); and
//! a comment on the whole design, in an empty design and through model
//! changes (mitcad#2010); images on comments and replies, removed, undone,
//! saved and read back, and refused over the limit or when their bytes do
//! not match (mitcad#2011).

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

/// What the application shows of a design file's refusal: the text before
/// its detail (mitcad#2011), which carries the fields and the ids.
fn shown(error: &str) -> String {
    mitcad_model::notice::FeatureError::from_text(error).message
}

/// The shown text says nothing of fields, comment ids or image ids.
fn in_words(text: &str) -> bool {
    !text.contains('[')
        && !text.split(|c: char| !c.is_alphanumeric()).any(|word| {
            word.len() > 1
                && (word.starts_with('C') || word.starts_with('A'))
                && word[1..].chars().all(|c| c.is_ascii_digit())
        })
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
    // The detail names the field and the comment; the text shown says it
    // in words (mitcad#2011).
    assert!(
        error.contains("comments[1] (C2): the text is empty"),
        "{error}"
    );
    let words = shown(&error);
    assert!(
        words.contains("a comment is damaged: its text is empty"),
        "{words}"
    );
    assert!(in_words(&words), "{words}");
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

/// A comment retargeted to a parameter has no place: it does not keep the
/// face's (mitcad#1315), in the query nor in the file.
#[test]
fn a_comment_retargeted_to_a_parameter_has_no_place() {
    let mut doc = block();
    add(&mut doc, "make this face flat", top());
    near(&comment(&doc, "C1")["place"], [10.0, 10.0, 20.0]);
    let retargeted = command(
        &mut doc,
        json!({"cmd": "retarget_comment", "id": "C1", "target": {"parameter": "width"}}),
    );
    assert_eq!(retargeted["changed"], true);
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["place"], Value::Null, "{c1}");
    assert_eq!(c1["label"], "width");
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        file["comments"][0].get("place").is_none(),
        "{}",
        file["comments"][0]
    );
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

/// Done when (mitcad#1305): a comment on a parameter follows
/// `rename_parameter` in the same undo step, is not orphaned, and undo and
/// redo take its target back and forth; a rename by a value slot's
/// `name = expression` takes it along too.
#[test]
fn a_comment_on_a_parameter_follows_its_rename() {
    let mut doc = block();
    add(&mut doc, "make this 25 mm", json!({"parameter": "width"}));
    let depth = document(&doc)["undo_depth"].as_u64().unwrap();
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "width", "new_name": "span"}),
    );
    // One undo step: the rename's.
    assert_eq!(document(&doc)["undo_depth"], depth + 1);
    assert_eq!(undo_label(&doc), "Rename width to span");
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["orphaned"], false, "{c1}");
    assert_eq!(c1["target"], json!({"parameter": "span"}));
    assert_eq!(c1["label"], "span");
    assert!(c1.get("error").is_none(), "{c1}");
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["comments"][0]["target"], json!({"parameter": "span"}));
    assert!(file["comments"][0].get("orphaned").is_none());

    command(&mut doc, json!({"cmd": "undo"}));
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["orphaned"], false, "{c1}");
    assert_eq!(c1["target"], json!({"parameter": "width"}));
    command(&mut doc, json!({"cmd": "redo"}));
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["orphaned"], false, "{c1}");
    assert_eq!(c1["target"], json!({"parameter": "span"}));

    // Extrude1's own distance parameter, renamed by its slot.
    let parameters = query(&doc, json!({"query": "parameters"}));
    let own = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["owner"] == "F2")
        .unwrap_or_else(|| panic!("Extrude1 owns no parameter: {parameters}"));
    let own = own["name"].as_str().unwrap().to_owned();
    let id = add(&mut doc, "deeper", json!({"parameter": own}));
    let mut def = query(&doc, json!({"query": "feature", "uid": "F2"}))["def"].clone();
    def["extent"]["distance"] = json!("depth = 20 mm");
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": def}),
    );
    let on_own = comment(&doc, &id);
    assert_eq!(on_own["orphaned"], false, "{on_own}");
    assert_eq!(on_own["target"], json!({"parameter": "depth"}));
    // The other comment stays on its own parameter.
    assert_eq!(comment(&doc, "C1")["target"], json!({"parameter": "span"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(comment(&doc, &id)["target"], json!({"parameter": own}));
    assert_eq!(comment(&doc, &id)["orphaned"], false);
}

/// A 4 mm box beside the block at `x`, a new body.
fn box_at(x: f64) -> Value {
    json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [x, 0],
           "length": 4, "width": 4, "height": 4, "operation": "new_body"}})
}

/// mitcad#1320: comments on a deleted last feature and on its body stay
/// orphaned after a save, a load and a new feature: the new one takes a
/// new uid, not the deleted one's, so the comments are not found on it.
#[test]
fn a_comment_on_a_deleted_last_feature_stays_orphaned_after_a_reload() {
    let mut doc = block();
    let added = command(&mut doc, box_at(40.0));
    assert_eq!(added["uid"], "F3", "{added}");
    let on_feature = add(&mut doc, "make this box taller", json!({"feature": "F3"}));
    let on_body = add(&mut doc, "round this box", json!({"body": "F3.b0"}));
    for id in [&on_feature, &on_body] {
        assert_eq!(comment(&doc, id)["orphaned"], false);
    }
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F3"}));
    for id in [&on_feature, &on_body] {
        assert_eq!(comment(&doc, id)["orphaned"], true);
    }

    // The file keeps the number the next feature takes.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["next_feature"], json!(4), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    loaded.recompute();
    assert_eq!(loaded.to_json(), saved);

    // A new box is F4: both comments stay orphaned on their targets.
    let added = command(&mut loaded, box_at(60.0));
    assert_eq!(added["uid"], "F4", "{added}");
    let c = comment(&loaded, &on_feature);
    assert_eq!(c["orphaned"], true, "{c}");
    assert_eq!(c["target"], json!({"feature": "F3"}), "{c}");
    let c = comment(&loaded, &on_body);
    assert_eq!(c["orphaned"], true, "{c}");
    assert_eq!(c["target"], json!({"body": "F3.b0"}), "{c}");
    // A comment can go on the new box.
    add(&mut loaded, "and this one", json!({"feature": "F4"}));
}

/// mitcad#1318: a comment on a body where an occurrence places it follows
/// the occurrence to another parent, and back with Undo.
#[test]
fn a_comment_follows_its_occurrence_to_another_parent() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    let arm = command(&mut doc, json!({"cmd": "create_component", "name": "Arm"}))["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "20 mm"}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    let cell = command(&mut doc, json!({"cmd": "create_component", "name": "Cell"}))["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let id = add(
        &mut doc,
        "round these edges",
        json!({"body": "F2.b0", "occurrence": arm}),
    );
    assert_eq!(comment(&doc, &id)["orphaned"], false);

    command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": arm, "parent": cell}),
    );
    let moved = comment(&doc, &id);
    assert_eq!(
        moved["target"]["occurrence"],
        format!("{cell}/{arm}"),
        "{moved}"
    );
    assert_eq!(moved["orphaned"], false, "{moved}");

    command(&mut doc, json!({"cmd": "undo"}));
    let back = comment(&doc, &id);
    assert_eq!(back["target"]["occurrence"], arm, "{back}");
    assert_eq!(back["orphaned"], false, "{back}");
}

/// mitcad#2555: a comment on a body whose features go into an existing
/// component (Move into Component with an explicit destination) takes the
/// destination's path and still finds its body; Undo gives it its path
/// back.
#[test]
fn a_comment_follows_its_body_into_an_existing_component() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    let created = command(&mut doc, json!({"cmd": "create_component", "name": "Arm"}));
    let arm = created["occurrence"].as_str().unwrap().to_owned();
    let arm_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "20 mm"}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    // Bin, an existing empty component.
    let created = command(&mut doc, json!({"cmd": "create_component", "name": "Bin"}));
    let bin = created["occurrence"].as_str().unwrap().to_owned();
    let bin_component = created["component"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let id = add(
        &mut doc,
        "round these edges",
        json!({"body": "F2.b0", "occurrence": arm}),
    );
    assert_eq!(comment(&doc, &id)["orphaned"], false);

    let result = command(
        &mut doc,
        json!({"cmd": "move_to_component", "component": arm_component,
               "features": ["F1", "F2"], "source_occurrence": arm,
               "destination": {"kind": "existing", "component": bin_component, "occurrence": bin}}),
    );
    assert_eq!(result["components"][0]["created"], false, "{result}");
    let moved = comment(&doc, &id);
    assert_eq!(moved["target"]["occurrence"], bin, "{moved}");
    assert_eq!(moved["orphaned"], false, "{moved}");

    command(&mut doc, json!({"cmd": "undo"}));
    let back = comment(&doc, &id);
    assert_eq!(back["target"]["occurrence"], arm, "{back}");
    assert_eq!(back["orphaned"], false, "{back}");
}

/// mitcad#2010, Done when: a `{"design": true}` comment in an empty design
/// is added, replied to, addressed, resolved, reopened and deleted, saved
/// and read back, never orphaned by a model change (a feature added and
/// deleted), labelled "Design" without a place; a target of another form
/// beside it is refused with the forms, which list the new one.
#[test]
fn a_comment_on_the_whole_design_works_in_an_empty_design() {
    let design = || json!({"design": true});
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    let id = add(&mut doc, "Plan a wall bracket for a 40 mm pipe", design());
    assert_eq!(id, "C1");
    assert_eq!(undo_label(&doc), "Add Comment C1");
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["target"], design(), "{c1}");
    assert_eq!(c1["label"], "Design", "{c1}");
    assert_eq!(c1["place"], Value::Null, "{c1}");
    assert_eq!(c1["orphaned"], false, "{c1}");
    assert_eq!(comments(&doc)["open"], 1);
    let on_design = query(&doc, json!({"query": "comments", "target": design()}));
    assert_eq!(ids(&on_design), ["C1"]);

    // The agent asks back, the user answers, the agent addresses it.
    command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "Which wall material?\nAnd which screws?",
               "author": "claude-code"}),
    );
    assert_eq!(undo_label(&doc), "Reply to C1");
    command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "Concrete, M6"}),
    );
    let replies = comment(&doc, "C1")["replies"].clone();
    assert_eq!(replies[0]["author"], "claude-code");
    assert_eq!(
        replies[0]["text"],
        "Which wall material?\nAnd which screws?"
    );
    assert_eq!(replies[1]["author"], "user");

    // Model changes: a block added and deleted, with undo; never orphaned.
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "20 mm"}, "operation": "new_body"}},
        ]),
    );
    assert_eq!(comment(&doc, "C1")["orphaned"], false);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F1"}));
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["orphaned"], false, "{c1}");
    assert_eq!(c1["place"], Value::Null, "{c1}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(comment(&doc, "C1")["orphaned"], false);

    command(
        &mut doc,
        json!({"cmd": "address_comment", "id": "C1", "resolution": "Bracket1 for a 40 mm pipe",
               "author": "claude-code"}),
    );
    assert_eq!(comment(&doc, "C1")["status"], "addressed");
    command(&mut doc, json!({"cmd": "resolve_comment", "id": "C1"}));
    assert_eq!(comment(&doc, "C1")["status"], "resolved");
    command(
        &mut doc,
        json!({"cmd": "reopen_comment", "id": "C1", "text": "Make it 3 mm thicker"}),
    );
    let c1 = comment(&doc, "C1");
    assert_eq!(c1["status"], "open");
    assert_eq!(c1["replies"].as_array().unwrap().len(), 3);

    // Saved and read back: the target as written, no place.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["comments"][0]["target"], design());
    assert!(file["comments"][0].get("place").is_none(), "{saved}");
    assert!(file["comments"][0].get("orphaned").is_none(), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    loaded.recompute();
    assert_eq!(loaded.to_json(), saved);
    let listed = |doc: &Document<MockKernel>| comments(doc)["comments"].clone();
    assert_eq!(listed(&loaded), listed(&doc));

    // A comment orphaned elsewhere is retargeted to the design (the undo
    // above brought Sketch1 back, without Extrude1).
    let on_sketch = add(&mut loaded, "this sketch", json!({"feature": "F1"}));
    command(&mut loaded, json!({"cmd": "delete_feature", "uid": "F1"}));
    assert_eq!(comment(&loaded, &on_sketch)["orphaned"], true);
    command(
        &mut loaded,
        json!({"cmd": "retarget_comment", "id": on_sketch, "target": design()}),
    );
    let retargeted = comment(&loaded, &on_sketch);
    assert_eq!(retargeted["orphaned"], false, "{retargeted}");
    assert_eq!(retargeted["place"], Value::Null, "{retargeted}");
    assert_eq!(retargeted["label"], "Design", "{retargeted}");

    // Another form beside it, or `false`, is no target; the error lists
    // the forms, the design's among them.
    for target in [
        json!({"design": true, "feature": "F1"}),
        json!({"design": true, "occurrence": "O1"}),
        json!({"design": false}),
    ] {
        let error = refused(
            &mut loaded,
            json!({"cmd": "add_comment", "text": "x", "target": target}),
        );
        assert!(error.contains("a target is"), "{target}: {error}");
        assert!(error.contains("{\"design\": true}"), "{target}: {error}");
    }

    command(&mut loaded, json!({"cmd": "delete_comment", "id": "C1"}));
    assert_eq!(undo_label(&loaded), "Delete Comment C1");
    assert_eq!(ids(&comments(&loaded)), [on_sketch.as_str()]);
}

// Images on comments and replies (mitcad#2011, GitHub mitcad/Mitcad#402).

/// The start of a PNG file of `width` × `height` pixels as its header says
/// (the signature, IHDR, IEND), with `tag` after it so that two of the same
/// size differ: what the model reads of an image.
fn png(width: u32, height: u32, tag: u8) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(13u32.to_be_bytes());
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes.extend([0, 0, 0, 0]);
    bytes.extend(0u32.to_be_bytes());
    bytes.extend(b"IEND");
    bytes.extend([0xae, 0x42, 0x60, 0x82, tag]);
    bytes
}

/// The start of a JPEG file of `width` × `height` pixels: its start of
/// frame.
fn jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08];
    bytes.extend(height.to_be_bytes());
    bytes.extend(width.to_be_bytes());
    bytes.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    bytes.extend([0xff, 0xd9]);
    bytes
}

/// A BMP file's header of 2 × 2 pixels.
fn bmp() -> Vec<u8> {
    let mut bytes = b"BM".to_vec();
    bytes.resize(14, 0);
    bytes.extend(40u32.to_le_bytes());
    bytes.extend(2i32.to_le_bytes());
    bytes.extend(2i32.to_le_bytes());
    bytes.resize(54, 0);
    bytes
}

fn base64(bytes: &[u8]) -> String {
    mitcad_model::internal::base64::encode(bytes)
}

fn attachment(bytes: &[u8], name: &str) -> Value {
    json!({"data": base64(bytes), "name": name})
}

fn attachment_ids(listed: &Value) -> Vec<String> {
    listed
        .as_array()
        .map(|images| {
            images
                .iter()
                .map(|image| image["id"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The bytes the `comment_attachment` query gives, decoded.
fn attachment_bytes(doc: &Document<MockKernel>, comment: &str, id: &str) -> Vec<u8> {
    let answer = query(
        doc,
        json!({"query": "comment_attachment", "comment": comment, "id": id}),
    );
    mitcad_model::internal::base64::decode(answer["data"].as_str().unwrap()).unwrap()
}

/// Done when: images attached to a comment and to a reply, listed with
/// their fields; removed, undone and redone, each change one undo step;
/// the query's bytes equal to the stored ones; ids never given again; a
/// comment on the whole design takes them too.
#[test]
fn images_on_a_comment_and_a_reply_are_listed_removed_undone_and_redone() {
    let mut doc = block();
    let revision = |doc: &Document<MockKernel>| comments(doc)["revision"].as_u64().unwrap();
    let capture = png(3, 2, 1);
    let added = command(
        &mut doc,
        json!({"cmd": "add_comment", "text": "make this face flat", "target": top(),
               "attachments": [attachment(&capture, "C:\\shots\\capture.png")]}),
    );
    assert_eq!(
        (&added["id"], &added["attachments"]),
        (&json!("C1"), &json!(["A1"]))
    );
    assert_eq!(undo_label(&doc), "Add Comment C1");
    let c1 = comment(&doc, "C1");
    assert_eq!(
        c1["attachments"],
        json!([{
            "id": "A1",
            "media_type": "image/png",
            "width": 3,
            "height": 2,
            "bytes": capture.len(),
            "sha256": mitcad_model::internal::sha256::Sha256::of(&capture).to_string(),
            "name": "capture.png",
        }]),
        "{c1}"
    );
    assert_eq!(attachment_bytes(&doc, "C1", "A1"), capture);
    let answer = query(
        &doc,
        json!({"query": "comment_attachment", "comment": "C1", "id": "A1"}),
    );
    assert_eq!(answer["comment"], "C1");
    assert_eq!(answer["media_type"], "image/png");
    assert_eq!(answer["name"], "capture.png");

    // A reply with two images: a PNG and a JPEG, one undo step.
    let r1 = revision(&doc);
    let photo = jpeg(640, 480);
    let replied = command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "Done, see the result",
               "author": "claude-code",
               "attachments": [attachment(&png(5, 5, 2), "result.png"), attachment(&photo, "")]}),
    );
    assert_eq!(
        (&replied["reply"], &replied["attachments"]),
        (&json!(1), &json!(["A2", "A3"]))
    );
    assert_eq!(undo_label(&doc), "Reply to C1");
    assert!(revision(&doc) > r1, "a new image grows the revision");
    let c1 = comment(&doc, "C1");
    assert_eq!(attachment_ids(&c1["attachments"]), ["A1"]);
    let images = &c1["replies"][0]["attachments"];
    assert_eq!(attachment_ids(images), ["A2", "A3"]);
    assert_eq!(images[1]["media_type"], "image/jpeg");
    assert_eq!(images[1]["width"], 640);
    assert_eq!(images[1]["height"], 480);
    // A JPEG without a name is named for its format.
    assert_eq!(images[1]["name"], "image.jpg");
    assert_eq!(attachment_bytes(&doc, "C1", "A3"), photo);
    // A reply without images lists none.
    command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "Thanks"}),
    );
    assert!(
        comment(&doc, "C1")["replies"][1]
            .get("attachments")
            .is_none()
    );

    // Removed: its own undo step, undone and redone; the others stay.
    let r2 = revision(&doc);
    command(
        &mut doc,
        json!({"cmd": "remove_comment_attachment", "comment": "C1", "id": "A2"}),
    );
    assert_eq!(undo_label(&doc), "Remove Image");
    assert!(revision(&doc) > r2);
    let images = |doc: &Document<MockKernel>| {
        let c1 = comment(doc, "C1");
        (
            attachment_ids(&c1["attachments"]),
            attachment_ids(&c1["replies"][0]["attachments"]),
        )
    };
    assert_eq!(images(&doc), (vec!["A1".to_owned()], vec!["A3".to_owned()]));
    let gone = doc
        .query(&json!({"query": "comment_attachment", "comment": "C1", "id": "A2"}).to_string())
        .err()
        .unwrap()
        .to_string();
    assert!(gone.contains("C1 has no image A2"), "{gone}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        images(&doc),
        (
            vec!["A1".to_owned()],
            vec!["A2".to_owned(), "A3".to_owned()]
        )
    );
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(images(&doc), (vec!["A1".to_owned()], vec!["A3".to_owned()]));
    command(
        &mut doc,
        json!({"cmd": "remove_comment_attachment", "comment": "C1", "id": "A1"}),
    );
    assert_eq!(undo_label(&doc), "Remove Image");
    assert!(comment(&doc, "C1").get("attachments").is_none());

    // Refused: an image the comment does not have, no image id, no comment.
    for (fields, expected) in [
        (json!({"comment": "C1", "id": "A9"}), "C1 has no image A9"),
        (json!({"comment": "C1", "id": "F2"}), "F2 is no image id"),
        (
            json!({"comment": "C7", "id": "A3"}),
            "there is no comment C7",
        ),
    ] {
        let mut remove = fields.clone();
        remove["cmd"] = json!("remove_comment_attachment");
        let error = refused(&mut doc, remove);
        assert!(error.contains(expected), "{fields}: {error}");
    }

    // The next image takes a new id, never one removed.
    let added = command(
        &mut doc,
        json!({"cmd": "add_comment", "text": "the whole design", "target": {"design": true},
               "attachments": [attachment(&png(8, 8, 3), "overview.png")]}),
    );
    assert_eq!(
        (&added["id"], &added["attachments"]),
        (&json!("C2"), &json!(["A4"]))
    );
    assert_eq!(attachment_ids(&comment(&doc, "C2")["attachments"]), ["A4"]);
    // Undoing the comment takes its image with it, redo brings both back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(ids(&comments(&doc)), ["C1"]);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(attachment_bytes(&doc, "C2", "A4"), png(8, 8, 3));
    // A change of images recomputes nothing.
    let before = doc.recompute_count();
    command(
        &mut doc,
        json!({"cmd": "remove_comment_attachment", "comment": "C2", "id": "A4"}),
    );
    assert_eq!(doc.recompute_count(), before);
}

/// Done when: an image over the limit, one that is no PNG or JPEG and data
/// that is no base64 are refused with the reason in words, and nothing of
/// the comment or the reply is added.
#[test]
fn an_image_over_the_limit_or_of_another_kind_is_refused() {
    let mut doc = block();
    add(&mut doc, "first", top());
    let label = undo_label(&doc);
    let mut large = png(4000, 3000, 0);
    large.resize(mitcad_model::MAX_COMMENT_IMAGE + 1, 0);
    let cases = [
        (
            vec![attachment(&large, "large.png")],
            "the image is 4.0 MB, more than the 4 MB a comment's image may be",
        ),
        (
            vec![attachment(&bmp(), "plan.bmp")],
            "only PNG and JPEG images can be attached, and this one is BMP",
        ),
        (
            vec![attachment(b"hello", "notes.txt")],
            "the file is no PNG, JPEG, BMP, WebP or HEIF image",
        ),
        (
            vec![json!({"data": "@@not base64@@"})],
            "the image's data is no base64",
        ),
        (
            vec![
                attachment(&png(2, 2, 0), "a.png"),
                attachment(&large, "b.png"),
            ],
            "image 2: the image is 4.0 MB, more than the 4 MB",
        ),
    ];
    for (attachments, expected) in cases {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_comment", "text": "with an image", "target": top(),
                   "attachments": attachments}),
        );
        assert!(error.contains(expected), "{expected}: {error}");
        let error = refused(
            &mut doc,
            json!({"cmd": "reply_comment", "id": "C1", "text": "with an image",
                   "attachments": attachments}),
        );
        assert!(error.contains(expected), "{expected}: {error}");
    }
    // Nothing added: no comment, no reply, no undo step, no id used.
    assert_eq!(ids(&comments(&doc)), ["C1"]);
    assert!(
        comment(&doc, "C1")["replies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(undo_label(&doc), label);
    let added = command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "now one that fits",
               "attachments": [attachment(&png(2, 2, 0), "a.png")]}),
    );
    assert_eq!(added["attachments"], json!(["A1"]));
    // Exactly the limit is taken.
    let mut limit = png(10, 10, 0);
    limit.resize(mitcad_model::MAX_COMMENT_IMAGE, 0);
    let added = command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C1", "text": "the largest",
               "attachments": [attachment(&limit, "limit.png")]}),
    );
    assert_eq!(added["attachments"], json!(["A2"]));
    assert_eq!(
        comment(&doc, "C1")["replies"][1]["attachments"][0]["bytes"],
        mitcad_model::MAX_COMMENT_IMAGE
    );
}

/// A change made to a saved design's JSON before it is read again.
type Change = dyn Fn(&mut Value);

/// Done when: the images saved apart from the comments and read back the
/// same, the query's bytes equal to the stored ones; ids go on after the
/// largest ever used; bytes that do not match their fields, an image on a
/// reply or a comment the file does not have, an id twice and an image
/// over the limit are refused on load; a design without images keeps no
/// field of them, and the comments themselves keep none, so that a build
/// without images reads them.
#[test]
fn images_are_saved_and_read_back_and_bytes_that_do_not_match_are_refused() {
    let mut doc = block();
    add(&mut doc, "no image", top());
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(file.get("comment_attachments").is_none());
    assert!(file.get("next_comment_attachment").is_none());

    let capture = png(3, 2, 1);
    let photo = jpeg(64, 48);
    command(
        &mut doc,
        json!({"cmd": "add_comment", "text": "flat", "target": top(),
               "attachments": [attachment(&capture, "capture.png")]}),
    );
    command(
        &mut doc,
        json!({"cmd": "reply_comment", "id": "C2", "text": "done", "author": "claude-code",
               "attachments": [attachment(&photo, "result.jpg"), attachment(&png(4, 4, 9), "last.png")]}),
    );
    // The largest removed: the next id is kept in the file.
    command(
        &mut doc,
        json!({"cmd": "remove_comment_attachment", "comment": "C2", "id": "A3"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    for c in file["comments"].as_array().unwrap() {
        assert!(c.get("attachments").is_none(), "{c}");
        for reply in c["replies"].as_array().into_iter().flatten() {
            assert!(reply.get("attachments").is_none(), "{reply}");
        }
    }
    let images = file["comment_attachments"].as_array().unwrap();
    assert_eq!(images.len(), 2, "{saved}");
    assert_eq!(images[0]["comment"], "C2");
    assert!(images[0].get("reply").is_none());
    assert_eq!(images[0]["id"], "A1");
    assert_eq!(images[0]["name"], "capture.png");
    assert_eq!(
        images[0]["image"],
        json!({"format": "png", "width": 3, "height": 2, "data": base64(&capture)})
    );
    assert_eq!(images[1]["comment"], "C2");
    assert_eq!(images[1]["reply"], 1);
    assert_eq!(images[1]["id"], "A2");
    assert_eq!(images[1]["image"]["format"], "jpeg");
    assert_eq!(file["next_comment_attachment"], 4);

    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    loaded.recompute();
    assert_eq!(loaded.to_json(), saved);
    let listed = |doc: &Document<MockKernel>| comments(doc)["comments"].clone();
    assert_eq!(listed(&loaded), listed(&doc));
    assert_eq!(attachment_bytes(&loaded, "C2", "A1"), capture);
    assert_eq!(attachment_bytes(&loaded, "C2", "A2"), photo);
    let added = command(
        &mut loaded,
        json!({"cmd": "reply_comment", "id": "C1", "text": "next",
               "attachments": [attachment(&png(1, 1, 0), "one.png")]}),
    );
    assert_eq!(added["attachments"], json!(["A4"]));

    // Refused on load, each with the reason.
    let broken = |change: &dyn Fn(&mut Value)| {
        let mut file: Value = serde_json::from_str(&saved).unwrap();
        change(&mut file);
        Document::from_json(&file.to_string(), MockKernel::default())
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "accepted".to_owned())
    };
    let cases: [(&Change, &str); 7] = [
        (
            &|f| f["comment_attachments"][0]["image"]["width"] = json!(4),
            "comment_attachments[0]: image A1: the image is a 3 × 2 png file, not the 4 × 2 png it says",
        ),
        (
            &|f| f["comment_attachments"][0]["image"]["data"] = json!(base64(&png(3, 3, 1))),
            "comment_attachments[0]: image A1: the image is a 3 × 3 png file, not the 3 × 2 png it says",
        ),
        (
            &|f| f["comment_attachments"][0]["image"]["format"] = json!("bmp"),
            "comment_attachments[0]: image A1: the image is bmp, not PNG or JPEG",
        ),
        (
            &|f| f["comment_attachments"][1]["reply"] = json!(3),
            "comment_attachments[1]: C2 has no reply 3",
        ),
        (
            &|f| f["comment_attachments"][1]["comment"] = json!("C9"),
            "comment_attachments[1]: there is no comment C9",
        ),
        (
            &|f| f["comment_attachments"][1]["id"] = json!("A1"),
            "comment_attachments[1]: image A1 is listed more than once",
        ),
        (
            &|f| {
                let mut large = png(3, 2, 1);
                large.resize(mitcad_model::MAX_COMMENT_IMAGE + 1, 0);
                f["comment_attachments"][0]["image"]["data"] = json!(base64(&large));
            },
            "comment_attachments[0]: image A1: the image is 4.0 MB, more than the 4 MB",
        ),
    ];
    for (change, expected) in cases {
        let error = broken(change);
        // The detail with the field and the ids, for agents and the log;
        // the text shown in words (mitcad#2011).
        assert!(error.contains(expected), "{expected}: {error}");
        let words = shown(&error);
        assert!(
            words.contains("an image of a comment is damaged: "),
            "{words}"
        );
        assert!(in_words(&words), "{words}");
    }
    let words = shown(&broken(&|f| {
        f["comment_attachments"][0]["image"]["width"] = json!(4)
    }));
    assert!(
        words.contains(
            "an image of a comment is damaged: the image is a 3 × 2 png file, not the 4 × 2 png it says"
        ),
        "{words}"
    );
    let words = shown(&broken(&|f| {
        f["comment_attachments"][1]["reply"] = json!(3)
    }));
    assert!(
        words.contains(
            "an image of a comment is damaged: it is on a reply the comment does not have"
        ),
        "{words}"
    );
    // A field it does not know is refused, as inside the comments.
    let error = broken(&|f| f["comment_attachments"][0]["pin"] = json!(1));
    assert!(error.contains("unknown field `pin`"), "{error}");
    assert!(in_words(&shown(&error)), "{error}");
}
