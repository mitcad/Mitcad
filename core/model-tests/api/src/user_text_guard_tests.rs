// SPDX-License-Identifier: MIT
//! What a person reads never names faces, edges, ids or points
//! (mitcad#1612, `docs/ui-conventions.md` "What a message says"): failing
//! cases (a chamfer and a fillet too large, a thread, conflicting sketch
//! constraints, an export) run through the api, and every message the
//! application shows (notices, a command's `error_notice`, failed
//! features' `message`, a refusal's text before its tail) is checked with
//! `notice::user_text_leaks`; agents keep the names, ids and points in the
//! `error`, the detail and the places.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::{FeatureError, NoticePlace, user_text_leaks};
use mitcad_model::test_support::{block, corner};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// That `text`, shown to a person, names nothing it may not.
fn assert_plain(text: &str) {
    assert_eq!(user_text_leaks(text), None, "a shown text leaks: {text:?}");
}

/// Every `message` in `value` (notices, error notices, failed features).
fn assert_messages_plain(value: &Value) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key == "message"
                    && let Some(text) = inner.as_str()
                {
                    assert_plain(text);
                }
                assert_messages_plain(inner);
            }
        }
        Value::Array(items) => items.iter().for_each(assert_messages_plain),
        _ => {}
    }
}

/// What the application shows of a refusal: the text before its tail and
/// before a sketch refusal's conflicts line.
fn shown_refusal(error: &str) -> String {
    let head = error
        .rfind("\n[conflicts: ")
        .map_or(error, |at| &error[..at]);
    FeatureError::from_text(head).message
}

/// The geometry kernel's chamfer refusal of the owner's case (a chamfer
/// beside a thread's end chamfer), as `placed_error` writes it.
fn chamfer_touch(edge: &str) -> String {
    let error = FeatureError::new(
        "the chamfer reaches exactly to the far edge of the face beside it, so that face would \
         end in a point; make the chamfer a little smaller or larger",
    )
    .at(NoticePlace::edge(edge))
    .at(NoticePlace::face("F2:end(r{c1})"))
    .at(NoticePlace::point([28.28, -28.28, 202.0]))
    .with_detail(format!(
        "the chamfer of edge {edge} reaches exactly to the far edge of face F2:end(r{{c1}}) \
         where face F2:side(c1) lies beyond it, at (28.28, -28.28, 202): its contact touches \
         that edge there, so that the face would end in a point; it cannot be built at this size"
    ));
    format!("chamfer: {}", error.text())
}

#[test]
fn a_chamfer_that_touches_a_far_edge_says_it_in_words_with_its_places() {
    let mut b = block();
    let edge = corner(b.extrude, 1, 0).to_string();
    b.doc
        .kernel()
        .fail_text
        .borrow_mut()
        .insert("chamfer", chamfer_touch(&edge));
    let result = command(
        &mut b.doc,
        json!({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
               "edges": [edge], "size": {"type": "equal_distance", "distance": 2}}}),
    );
    // Agents: the names and the point.
    let error = result["error"].as_str().unwrap();
    assert!(error.contains(&edge), "{result}");
    assert!(error.contains("(28.28, -28.28, 202)"), "{result}");
    // The person: words, the edge and the point painted.
    let notice = &result["error_notice"];
    assert_plain(notice["message"].as_str().unwrap());
    assert!(
        notice["message"]
            .as_str()
            .unwrap()
            .contains("reaches exactly to the far edge"),
        "{result}"
    );
    let places = notice["places"].as_array().unwrap();
    assert!(places.iter().any(|p| p["kind"] == "edge"), "{result}");
    assert!(places.iter().any(|p| p["kind"] == "point"), "{result}");
    assert_messages_plain(&result);
    // The timeline and the feature query: the same.
    let timeline = query(&b.doc, json!({"query": "timeline"}));
    assert_messages_plain(&timeline);
    let item = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["status"] == "error")
        .unwrap_or_else(|| panic!("no failed feature in {timeline}"))
        .clone();
    assert!(item["error"].as_str().unwrap().contains(&edge), "{item}");
    let feature = query(&b.doc, json!({"query": "feature", "uid": item["uid"]}));
    assert_messages_plain(&feature);
}

#[test]
fn older_kernel_texts_with_names_and_points_are_cleaned_for_people() {
    // A fillet's and a thread's text in the older form, names and a point
    // in the sentence: the fallback cleaner takes them out into places.
    for (op, def, text) in [
        (
            "fillet",
            json!({"type": "fillet", "body": "F2.b0", "radius": 30}),
            "fillet: the fillet of edge E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])} overflows face \
             F2:end(r{c1}) at (60, 0, 20); face F2:side(c2[c1,c3]) is narrower than the radius",
        ),
        (
            "fillet",
            json!({"type": "fillet", "body": "F2.b0", "radius": 30}),
            "fillet failed on F2:top and F2.b0 (k5, p15 and c14)",
        ),
    ] {
        let mut b = block();
        let mut def = def;
        def["edges"] = json!([corner(b.extrude, 1, 0).to_string()]);
        b.doc
            .kernel()
            .fail_text
            .borrow_mut()
            .insert(op, text.to_owned());
        let result = command(&mut b.doc, json!({"cmd": "add_feature", "def": def}));
        assert!(result["error"].as_str().unwrap().contains(text), "{result}");
        assert_messages_plain(&result);
        assert_messages_plain(&query(&b.doc, json!({"query": "timeline"})));
        let failed = &result["failed_features"][0];
        assert_plain(failed["message"].as_str().unwrap_or_default());
        assert!(failed["error"].as_str().unwrap().contains(text), "{result}");
    }
}

#[test]
fn a_thread_s_refusal_names_no_face() {
    let mut b = block();
    let face = format!("F{}:side(c1[c4,c2])", b.extrude.0);
    let text = FeatureError::new("the face is too thin for the thread")
        .at(NoticePlace::face(face.as_str()))
        .with_detail(format!("face {face} is too thin for the thread"))
        .text();
    b.doc
        .kernel()
        .fail_text
        .borrow_mut()
        .insert("chamfer", text);
    let result = command(
        &mut b.doc,
        json!({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
               "edges": [corner(b.extrude, 1, 0).to_string()],
               "size": {"type": "equal_distance", "distance": 1}}}),
    );
    assert_messages_plain(&result);
    assert_eq!(
        result["error_notice"]["places"][0]["name"],
        json!(face),
        "{result}"
    );
    assert!(
        result["error"].as_str().unwrap().contains(&face),
        "{result}"
    );
}

#[test]
fn a_conflicting_sketch_constraint_is_refused_in_words() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 5]}),
    );
    let id = line["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|e| e.starts_with('c'))
        .unwrap_or_else(|| panic!("no line in {line}"))
        .to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": id}}),
    );
    let refused = d
        .command(
            &json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                    "constraint": {"type": "vertical", "line": id}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert_plain(&shown_refusal(&refused));
    // The sketch query's notices.
    assert_messages_plain(&query(&d, json!({"query": "sketch", "uid": "F1"})));
}

#[test]
fn a_refusal_that_names_ids_is_shown_in_words() {
    // A sketch edit about an entity that is not there: agents read its id,
    // a person words.
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let refused = d
        .command(
            &json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": ["c1"], "axis": "c9"})
                .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert_plain(&shown_refusal(&refused));
    assert!(
        FeatureError::from_text(&refused)
            .agent_text()
            .contains("c9"),
        "{refused}"
    );
}

#[test]
fn an_export_that_cannot_mesh_a_face_says_it_in_words() {
    let mut b = block();
    let detail = "a face could not be triangulated: F2:side(c1[c4,c2]) (cylinder)";
    let text = FeatureError::new("a face could not be triangulated (its surface: cylinder)")
        .at(NoticePlace::face("F2:side(c1[c4,c2])"))
        .with_detail(detail)
        .text();
    b.doc
        .kernel()
        .fail_text
        .borrow_mut()
        .insert("triangle_mesh", text);
    let dir = mitcad_model::testing::unique_temp_dir("user-text-export");
    let stl = dir.join("export.stl").to_str().unwrap().to_owned();
    let refused = b
        .doc
        .command(&json!({"cmd": "export", "path": stl}).to_string())
        .unwrap_err()
        .to_string();
    mitcad_model::testing::remove_temp(&dir);
    let shown = shown_refusal(&refused);
    assert_plain(&shown);
    assert!(
        shown.contains("a face could not be triangulated (its surface: cylinder)"),
        "{refused}"
    );
    let read = FeatureError::from_text(&refused);
    assert!(read.agent_text().contains(detail), "{refused}");
    assert!(
        read.places
            .iter()
            .any(|p| p.name() == Some("F2:side(c1[c4,c2])")),
        "{refused}"
    );
}
