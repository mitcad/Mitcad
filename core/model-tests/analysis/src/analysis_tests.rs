// SPDX-License-Identifier: MIT
//! Analyses kept in the document (mitcad#41): section analyses added,
//! edited, shown and hidden one at a time, renamed and deleted as undo
//! steps, their planes following the model, and saved in the design file.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::{AnalysisApi, DocumentApi};
use mitcad_model_diff::diff_states;

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

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "analyses"}"#).unwrap()).unwrap();
    answer.as_array().unwrap().clone()
}

/// The analyses as `name` or `name*` when shown.
fn shown(doc: &Document<MockKernel>) -> Vec<String> {
    analyses(doc)
        .iter()
        .map(|a| {
            let name = a["name"].as_str().unwrap();
            if a["visible"].as_bool().unwrap() {
                format!("{name}*")
            } else {
                name.to_owned()
            }
        })
        .collect()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "document"}"#).unwrap()).unwrap();
    answer["undo"].clone()
}

fn section(plane: Value, offset: f64, flip: bool) -> Value {
    json!({"type": "section", "plane": plane, "offset": offset, "flip": flip})
}

/// A 20 x 20 x 20 block (F2.b0) on a rectangle on XY.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

#[test]
fn section_analyses_are_added_edited_shown_one_at_a_time_and_undone() {
    let mut doc = block();
    assert!(analyses(&doc).is_empty());
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("yz"), 10.0, false)}),
    );
    assert_eq!(added["name"], "Section1");
    assert_eq!(added["recomputed"], 0, "an analysis changes no geometry");
    assert_eq!(undo_label(&doc), "Add Section1");
    let first = &analyses(&doc)[0];
    assert_eq!(first["type"], "section");
    assert_eq!(first["plane"], "yz");
    assert_eq!(first["offset"], 10.0);
    assert_eq!(
        first["section"],
        json!({"origin": [10.0, 0.0, 0.0], "normal": [1.0, 0.0, 0.0]})
    );

    // A second one is shown instead of the first.
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xy"), 0.0, true)}),
    );
    assert_eq!(added["name"], "Section2");
    assert_eq!(shown(&doc), ["Section1", "Section2*"]);
    // Flipped: the normal reversed, without -0.
    assert_eq!(
        analyses(&doc)[1]["section"],
        json!({"origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, -1.0]})
    );
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Section1", "visible": true}),
    );
    assert_eq!(shown(&doc), ["Section1*", "Section2"]);
    assert_eq!(undo_label(&doc), "Show Section1");
    // Nothing to change: no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Section1", "visible": true}),
    );
    assert_eq!(undo_label(&doc), "Show Section1");
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Section1", "visible": false}),
    );
    assert_eq!(shown(&doc), ["Section1", "Section2"]);
    assert_eq!(undo_label(&doc), "Hide Section1");

    // Editing keeps it hidden unless asked to show it.
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": section(json!("yz"), 5.0, true)}),
    );
    assert_eq!(shown(&doc), ["Section1", "Section2"]);
    assert_eq!(undo_label(&doc), "Edit Section1");
    assert_eq!(
        analyses(&doc)[0]["section"],
        json!({"origin": [5.0, 0.0, 0.0], "normal": [-1.0, 0.0, 0.0]})
    );
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section2", "def": section(json!("xy"), 0.0, true),
               "visible": true}),
    );
    assert_eq!(shown(&doc), ["Section1", "Section2*"]);
    assert_eq!(undo_label(&doc), "Edit Section2");

    command(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Section1", "new_name": "Middle"}),
    );
    assert_eq!(shown(&doc), ["Middle", "Section2*"]);
    // The next default name is still free.
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xz"), 0.0, false), "name": "  "}),
    );
    assert_eq!(shown(&doc), ["Middle", "Section2", "Section1*"]);
    command(
        &mut doc,
        json!({"cmd": "delete_analysis", "name": "Section2"}),
    );
    assert_eq!(shown(&doc), ["Middle", "Section1*"]);
    assert_eq!(undo_label(&doc), "Delete Section2");

    // Undo and redo take them back as other document commands.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(shown(&doc), ["Middle", "Section2", "Section1*"]);
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(shown(&doc), ["Section1", "Section2*"]);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(shown(&doc), ["Middle", "Section2*"]);
}

#[test]
fn analyses_refuse_what_they_cannot_keep() {
    let mut doc = block();
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("x"), 0.0, false)}),
    );
    assert!(error.contains("the section's plane"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!({"body": "F2.b0", "face": "F2:end(r{c9})"}), 0.0, false)}),
    );
    assert!(error.contains("F2:end(r{c9})"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy", "depth": 3}}),
    );
    assert!(error.contains("depth"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xy"), 0.0, false), "name": "Cut"}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xy"), 0.0, false), "name": "Cut"}),
    );
    assert_eq!(error, "an analysis 'Cut' exists");
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("yz"), 0.0, false)}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Section1", "new_name": "Cut"}),
    );
    assert_eq!(error, "an analysis 'Cut' exists");
    let error = refused(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Section1", "new_name": " "}),
    );
    assert_eq!(error, "the analysis needs a name");
    let error = refused(&mut doc, json!({"cmd": "delete_analysis", "name": "Nope"}));
    assert_eq!(error, "there is no analysis 'Nope'");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Nope", "visible": true}),
    );
    assert_eq!(error, "there is no analysis 'Nope'");
}

#[test]
fn a_section_on_a_face_follows_the_model() {
    let mut doc = block();
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(top.clone(), -5.0, false)}),
    );
    assert_eq!(
        analyses(&doc)[0]["section"],
        json!({"origin": [0.0, 0.0, 15.0], "normal": [0.0, 0.0, 1.0]})
    );
    // The block made taller: the face is found again by its name.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}),
    );
    assert_eq!(
        analyses(&doc)[0]["section"],
        json!({"origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]})
    );
    assert_eq!(analyses(&doc)[0]["plane"], top);
    // Rolled back before the block, the face is not there: the analysis
    // says why, and stays.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let rolled = &analyses(&doc)[0];
    assert!(rolled.get("section").is_none(), "{rolled}");
    assert!(
        rolled["error"].as_str().unwrap().contains("F2.b0"),
        "{rolled}"
    );
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    assert!(analyses(&doc)[0].get("section").is_some());
}

#[test]
fn analyses_are_saved_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    assert!(!plain.contains("\"analyses\""), "none: left out");
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("yz"), 10.0, true)}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!({"body": "F2.b0", "face": format!("F2:end({R})")}), 0.0, false)}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "Section1", "visible": false, "type": "section", "plane": "yz", "offset": 10.0,
             "flip": true},
            {"name": "Section2", "type": "section",
             "plane": {"body": "F2.b0", "face": format!("F2:end({R})")}}
        ])
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    assert_eq!(shown(&loaded), ["Section1", "Section2*"]);

    // Older files without analyses load as before.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(analyses(&old).is_empty());

    // The comparison of versions lists them.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.analyses.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["Section1: added", "Section2: added"]);
    assert!(diff.summary.contains("+2 analyses"), "{}", diff.summary);

    // One shown at a time, also from a file: the first shown one.
    let both = saved.replace("\"visible\": false,\n", "");
    let loaded = Document::from_json(&both, MockKernel::default()).unwrap();
    assert_eq!(shown(&loaded), ["Section1*", "Section2"]);

    // Broken entries are refused with where they are.
    let twice = saved.replace("\"Section2\"", "\"Section1\"");
    let Err(error) = Document::from_json(&twice, MockKernel::default()) else {
        panic!("a name listed twice was read");
    };
    assert!(error.to_string().contains("analyses[1]"), "{error}");
    let unknown = saved.replace("\"offset\": 10.0", "\"depth\": 10.0");
    let Err(error) = Document::from_json(&unknown, MockKernel::default()) else {
        panic!("an unknown field was read");
    };
    assert!(error.to_string().contains("analyses[0]"), "{error}");
}

fn resolved_section(
    doc: &Document<MockKernel>,
    plane: Value,
    occurrence: Option<&str>,
    offset: f64,
    flip: bool,
) -> Value {
    let mut query =
        json!({"query": "section_plane", "plane": plane, "offset": offset, "flip": flip});
    if let Some(path) = occurrence {
        query["occurrence"] = json!(path);
    }
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn near_vector(actual: &Value, expected: [f64; 3]) {
    for (i, expected) in expected.into_iter().enumerate() {
        assert!(
            (actual[i].as_f64().unwrap() - expected).abs() < 1e-9,
            "{actual}"
        );
    }
}

#[test]
fn section_planes_follow_nested_occurrences_and_flip_only_the_normal() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Assembly", "transform": {
                "translation": [100, 20, 30], "rotation": {"axis": [1, 0, 0], "angle": std::f64::consts::FRAC_PI_2}}},
            {"cmd": "create_component", "name": "Block", "transform": {"translation": [5, 7, 11]}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
                "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
                "type": "offset", "plane": "xy", "distance": 20}}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    let face = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    for offset in [0.0, -5.0, 5.0] {
        let plane = resolved_section(&doc, face.clone(), Some("O1/O2"), offset, false);
        near_vector(&plane["origin"], [105.0, -11.0 - offset, 37.0]);
        near_vector(&plane["normal"], [0.0, -1.0, 0.0]);
        let datum = resolved_section(&doc, json!("F3"), Some("O1/O2"), offset, false);
        assert_eq!(
            plane, datum,
            "the face and equivalent construction plane agree"
        );
        let flipped = resolved_section(&doc, face.clone(), Some("O1/O2"), offset, true);
        assert_eq!(
            plane["origin"], flipped["origin"],
            "Flip keeps the plane in place"
        );
        near_vector(&flipped["normal"], [0.0, 1.0, 0.0]);
        assert_eq!(
            plane,
            resolved_section(&doc, face.clone(), Some("O1/O2"), offset, false)
        );
        // Old analyses had no path: a uniquely placed component is inferable.
        assert_eq!(
            plane,
            resolved_section(&doc, face.clone(), None, offset, false)
        );
    }
    // The opposite face has the opposite outward normal. An equivalent
    // signed offset reaches the same interior plane; Flip exchanges sides.
    let bottom = json!({"body": "F2.b0", "face": format!("F2:start({R})")});
    let opposite = resolved_section(&doc, bottom, Some("O1/O2"), -15.0, false);
    near_vector(&opposite["origin"], [105.0, -6.0, 37.0]);
    near_vector(&opposite["normal"], [0.0, 1.0, 0.0]);
    let fixed = resolved_section(
        &doc,
        json!({"origin": [105, -6, 37], "normal": [0, -1, 0]}),
        None,
        0.0,
        false,
    );
    near_vector(&fixed["origin"], [105.0, -6.0, 37.0]);
    near_vector(&fixed["normal"], [0.0, -1.0, 0.0]);

    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Assembly:1", "transform": {
        "translation": [200, 0, 0], "rotation": {"axis": [0, 1, 0], "angle": std::f64::consts::PI}}}),
    );
    let repeated = resolved_section(&doc, face.clone(), Some("O3/O2"), -5.0, false);
    near_vector(&repeated["origin"], [195.0, 7.0, -26.0]);
    near_vector(&repeated["normal"], [0.0, 0.0, -1.0]);
    assert!(
        doc.query(&json!({"query": "section_plane", "plane": face}).to_string())
            .unwrap_err()
            .to_string()
            .contains("select an occurrence")
    );
    for wrong in ["", "O1", "O99", "O2/O1"] {
        assert!(
            doc.query(
                &json!({"query": "section_plane", "plane": face, "occurrence": wrong}).to_string()
            )
            .is_err()
        );
    }

    let definition = json!({"type": "section", "plane": face, "occurrence": "O3/O2", "offset": -5});
    command(&mut doc, json!({"cmd": "add_analysis", "def": definition}));
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    assert_eq!(analyses(&loaded)[0]["occurrence"], "O3/O2");
    command(
        &mut loaded,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": {
        "type": "section", "plane": face, "occurrence": "O3/O2", "offset": -5, "flip": true}}),
    );
    assert_eq!(
        analyses(&loaded)[0]["section"]["origin"],
        repeated["origin"]
    );
    near_vector(&analyses(&loaded)[0]["section"]["normal"], [0.0, 0.0, 1.0]);
    command(&mut loaded, json!({"cmd": "undo"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    let before = loaded.to_json();
    refused(
        &mut loaded,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": {
        "type": "section", "plane": face, "occurrence": "O1"}}),
    );
    assert_eq!(
        loaded.to_json(),
        before,
        "invalid occurrence edits change nothing"
    );
}

/// Section Analysis prepared to be worked out without the document, on
/// another thread (mitcad#438): the clips and the `section` query answer as
/// the document does, each body with its own error, and only `section` is
/// prepared. (The mock kernel cuts nothing: its answers are its errors.)
#[test]
fn prepared_clips_and_sections_answer_as_the_document() {
    let doc = block();
    let plane = json!({"origin": [10, 0, 0], "normal": [1, 0, 0]});
    let request =
        json!({"plane": plane, "bodies": [{"body": "F2.b0"}, {"body": "F9.b0"}]}).to_string();
    let prepared = doc.prepare_clips(&request).unwrap();
    assert_eq!(prepared.len(), 2);
    assert!(!prepared.is_empty());
    let direct: Vec<Option<String>> = doc
        .analysis_clips(&request)
        .unwrap()
        .into_iter()
        .map(|clip| clip.err().map(|e| e.to_string()))
        .collect();
    let later: Vec<Option<String>> = prepared
        .run(doc.kernel())
        .into_iter()
        .map(|clip| clip.err().map(|e| e.to_string()))
        .collect();
    assert_eq!(later, direct);
    // The mock kernel's detach gives each body as it is, as detached.
    assert!(prepared.input(0).is_some_and(|(_, detached)| detached));
    assert!(prepared.input(1).is_none());
    assert!(
        direct[1].as_deref().is_some_and(|e| e.contains("F9.b0")),
        "{direct:?}"
    );

    let query = json!({"query": "section", "world": true, "plane": plane}).to_string();
    let now = doc.query(&query).map_err(|e| e.to_string());
    let later = doc
        .prepare_query(&query)
        .and_then(|prepared| prepared.run(doc.kernel()))
        .map(|answer| answer.to_string())
        .map_err(|e| e.to_string());
    match (&now, &later) {
        (Ok(now), Ok(later)) => {
            let now: Value = serde_json::from_str(now).unwrap();
            let later: Value = serde_json::from_str(later).unwrap();
            assert_eq!(later, now);
        }
        (Err(now), Err(later)) => assert!(now.contains(later.as_str()), "{now} / {later}"),
        _ => panic!("now {now:?}, prepared {later:?}"),
    }
    let refused = doc
        .prepare_query(r#"{"query": "properties"}"#)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(refused.contains("prepared query"), "{refused}");
}

/// A feature preview shown cut by Section Analysis (mitcad#943): the clips
/// take the previewed command's bodies with `preview`, also one the
/// preview makes, which is no body at the marker, placed by `transform`;
/// without a preview there is nothing to clip. (The mock kernel cuts
/// nothing: its answers are its errors.)
#[test]
fn clips_take_the_previewed_bodies() {
    let mut doc = block();
    let plane = json!({"origin": [10, 0, 0], "normal": [1, 0, 0]});
    let errors = |doc: &Document<MockKernel>, bodies: Value| -> Vec<Option<String>> {
        let request = json!({"plane": plane, "bodies": bodies}).to_string();
        doc.analysis_clips(&request)
            .unwrap()
            .into_iter()
            .map(|clip| clip.err().map(|e| e.to_string()))
            .collect()
    };
    let before = errors(&doc, json!([{"body": "F2.b0", "preview": true}]));
    assert!(
        before[0]
            .as_deref()
            .is_some_and(|e| e.contains("not in the preview")),
        "{before:?}"
    );

    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [40, 0], "width": 10, "height": 10},
        ]),
    );
    let add = json!({"cmd": "add_feature", "def": {"type": "extrude",
        "profiles": [{"sketch": "F3", "region": R}],
        "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}});
    let report: Value = serde_json::from_str(&doc.preview(&add.to_string()).unwrap()).unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    let made = report["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|body| body["uid"].as_str())
        .find(|uid| *uid != "F2.b0")
        .unwrap_or_else(|| panic!("no new body in {report}"))
        .to_owned();
    let moved = json!({"linear": [[1, 0, 0], [0, 1, 0], [0, 0, 1]], "translation": [5, 0, 0]});
    let bodies = json!([
        {"body": "F2.b0", "preview": true},
        {"body": made, "preview": true, "transform": moved},
    ]);
    // Both reach the kernel's clip: the previewed bodies are there.
    let clips = errors(&doc, bodies.clone());
    assert_eq!(clips.len(), 2);
    for clip in &clips {
        assert!(
            clip.as_deref().is_some_and(|e| !e.contains("preview")),
            "{clips:?}"
        );
    }
    let request = json!({"plane": plane, "bodies": bodies}).to_string();
    let prepared = doc.prepare_clips(&request).unwrap();
    assert!(prepared.input(0).is_some() && prepared.input(1).is_some());
    // The new body is only in the preview.
    let committed = errors(&doc, json!([{"body": made}]));
    assert!(
        committed[0]
            .as_deref()
            .is_some_and(|e| e.contains(made.as_str())),
        "{committed:?}"
    );
}

/// The `section_plane` query of a plane turned about an axis (mitcad#823).
fn turned_section(doc: &Document<MockKernel>, mut query: Value) -> Result<Value, String> {
    query["query"] = json!("section_plane");
    doc.query(&query.to_string())
        .map(|answer| serde_json::from_str(&answer).unwrap())
        .map_err(|e| e.to_string())
}

/// A section's plane turned by an angle about an axis (mitcad#823): turned
/// rigidly about the axis, then moved by the offset along the turned
/// normal; Flip reverses only the normal.
#[test]
fn section_planes_turn_by_an_angle_about_an_axis() {
    let mut doc = block();
    let angle = 30f64.to_radians();
    let (sin, cos) = angle.sin_cos();
    let turned = turned_section(&doc, json!({"plane": "xy", "axis": "x", "angle": angle})).unwrap();
    near_vector(&turned["normal"], [0.0, -sin, cos]);
    near_vector(&turned["origin"], [0.0, 0.0, 0.0]);
    near_vector(&turned["axis"]["origin"], [0.0, 0.0, 0.0]);
    near_vector(&turned["axis"]["direction"], [1.0, 0.0, 0.0]);
    assert_eq!(turned["axis_occurrence"], "");
    let moved = turned_section(
        &doc,
        json!({"plane": "xy", "axis": "x", "angle": angle, "offset": 10}),
    )
    .unwrap();
    near_vector(&moved["normal"], [0.0, -sin, cos]);
    near_vector(&moved["origin"], [0.0, -10.0 * sin, 10.0 * cos]);
    let flipped = turned_section(
        &doc,
        json!({"plane": "xy", "axis": "x", "angle": angle, "offset": 10, "flip": true}),
    )
    .unwrap();
    assert_eq!(flipped["origin"], moved["origin"], "Flip keeps the plane");
    near_vector(&flipped["normal"], [0.0, sin, -cos]);
    // An axis with no angle leaves the plane as it is.
    let still = turned_section(&doc, json!({"plane": "xy", "axis": "x", "offset": 4})).unwrap();
    near_vector(&still["origin"], [0.0, 0.0, 4.0]);
    near_vector(&still["normal"], [0.0, 0.0, 1.0]);

    // The turn is about the axis' own line: a construction axis along Z
    // through (10, 10) turns YZ (x = 0) a quarter round onto y = 0.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_axis",
               "definition": {"type": "fixed", "origin": [10, 10, 0], "direction": [0, 0, 1]}}}),
    );
    let quarter = std::f64::consts::FRAC_PI_2;
    let about_f3 =
        turned_section(&doc, json!({"plane": "yz", "axis": "F3", "angle": quarter})).unwrap();
    near_vector(&about_f3["origin"], [20.0, 0.0, 0.0]);
    near_vector(&about_f3["normal"], [0.0, 1.0, 0.0]);

    // Refused: an angle without an axis, an axis along the plane's normal,
    // something that is no axis.
    let error = turned_section(&doc, json!({"plane": "xy", "angle": angle})).unwrap_err();
    assert!(error.contains("an angle needs an axis"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy", "angle": angle}}),
    );
    assert!(error.contains("an angle needs an axis"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy", "axis": "z",
               "angle": angle}}),
    );
    assert!(error.contains("along the plane's normal"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy", "axis": "xz",
               "angle": angle}}),
    );
    assert!(error.contains("the axis"), "{error}");
    assert!(analyses(&doc).is_empty(), "nothing refused was added");

    // Kept, edited and refused edits; the analyses query gives the turned
    // plane, or why the axis is not there at the marker.
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "yz", "axis": "F3",
               "angle": quarter, "offset": 5}}),
    );
    let kept = &analyses(&doc)[0];
    near_vector(&kept["section"]["origin"], [20.0, 5.0, 0.0]);
    near_vector(&kept["section"]["normal"], [0.0, 1.0, 0.0]);
    assert_eq!(kept["axis"], "F3");
    let before = doc.to_json();
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": {"type": "section",
               "plane": "yz", "angle": quarter}}),
    );
    assert!(error.contains("an angle needs an axis"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": {"type": "section",
               "plane": "xy", "axis": "F3", "angle": quarter}}),
    );
    assert!(error.contains("along the plane's normal"), "{error}");
    assert_eq!(doc.to_json(), before, "refused edits change nothing");
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    let rolled = &analyses(&doc)[0];
    assert!(rolled.get("section").is_none(), "{rolled}");
    assert!(
        rolled["error"].as_str().unwrap().contains("the axis"),
        "{rolled}"
    );
    command(&mut doc, json!({"cmd": "set_marker", "position": 3}));
    assert!(analyses(&doc)[0].get("section").is_some());
}

/// An axis in a nested occurrence is moved by its path from the design
/// root, as the plane is by its own (mitcad#823).
#[test]
fn a_section_axis_follows_its_nested_occurrence() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Assembly", "transform": {
                "translation": [100, 20, 30], "rotation": {"axis": [1, 0, 0], "angle": std::f64::consts::FRAC_PI_2}}},
            {"cmd": "create_component", "name": "Block", "transform": {"translation": [5, 7, 11]}},
            {"cmd": "add_feature", "def": {"type": "construction_axis",
                "definition": {"type": "fixed", "origin": [0, 0, 0], "direction": [0, 0, 1]}}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    // The axis in Block, at the origin along Z, is in the design at
    // (105, 9, 37) along -Y. XY (z = 0, 37 below it) turned a quarter round
    // about it is x = 142, the normal along -X.
    let quarter = std::f64::consts::FRAC_PI_2;
    let query = json!({"plane": "xy", "axis": "F1", "axis_occurrence": "O1/O2", "angle": quarter});
    let turned = turned_section(&doc, query).unwrap();
    near_vector(&turned["axis"]["origin"], [105.0, 9.0, 37.0]);
    near_vector(&turned["axis"]["direction"], [0.0, -1.0, 0.0]);
    assert_eq!(turned["axis_occurrence"], "O1/O2");
    near_vector(&turned["origin"], [142.0, 0.0, -68.0]);
    near_vector(&turned["normal"], [-1.0, 0.0, 0.0]);
    // Without a path, the component's only placement.
    let inferred =
        turned_section(&doc, json!({"plane": "xy", "axis": "F1", "angle": quarter})).unwrap();
    assert_eq!(inferred, turned);
    // A path that does not own the axis is refused.
    for wrong in ["O1", "O99", "O2/O1"] {
        let error = turned_section(
            &doc,
            json!({"plane": "xy", "axis": "F1", "axis_occurrence": wrong, "angle": quarter}),
        )
        .unwrap_err();
        assert!(error.contains("the axis"), "{wrong}: {error}");
    }
    // A repeated component needs the path.
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Assembly:1", "transform": {
        "translation": [200, 0, 0], "rotation": {"axis": [0, 1, 0], "angle": std::f64::consts::PI}}}),
    );
    let error =
        turned_section(&doc, json!({"plane": "xy", "axis": "F1", "angle": quarter})).unwrap_err();
    assert!(error.contains("select an occurrence"), "{error}");
    let query = json!({"plane": "xy", "axis": "F1", "axis_occurrence": "O1/O2", "angle": quarter});
    assert_eq!(turned_section(&doc, query).unwrap(), turned);
}

/// A turned analysis is saved and read back with the same plane; one
/// without an angle is written as before (mitcad#823).
#[test]
fn turned_sections_are_saved_and_read_back() {
    let mut doc = block();
    let angle = 45f64.to_radians();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "yz", "axis": "z",
               "axis_occurrence": "", "angle": angle, "offset": 2, "flip": true}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xz", "axis": "x"}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let first = &file["analyses"][0];
    assert_eq!(first["axis"], "z");
    assert_eq!(first["axis_occurrence"], "");
    assert_eq!(first["angle"], json!(angle));
    let second = &file["analyses"][1];
    assert!(second.get("angle").is_none(), "no angle written: {second}");
    assert_eq!(second["axis"], "x");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    let section = &analyses(&loaded)[0]["section"];
    let (sin, cos) = angle.sin_cos();
    near_vector(&section["normal"], [-cos, -sin, 0.0]);
    near_vector(&section["origin"], [2.0 * cos, 2.0 * sin, 0.0]);
    // Older MitCad refuses a turned analysis by its unknown fields; a file
    // with an angle and no axis is refused here.
    let no_axis = saved.replace("\"axis\": \"z\",", "");
    assert!(Document::from_json(&no_axis, MockKernel::default()).is_err());
}

/// A plane past every body (mitcad#824): the `section` query counts the
/// bodies the clip cuts, keeps whole and removes whole (each body's `clip`
/// too), here and prepared for another thread, so that the panel can say
/// why the section is empty. The mock kernel's sections cut the bounds.
#[test]
fn section_counts_the_bodies_a_plane_past_them_keeps_or_removes() {
    let doc = block();
    doc.kernel().sections.set(true);
    let answer = |x: f64, normal: f64, world: bool| -> Value {
        let query = json!({"query": "section", "world": world,
            "plane": {"origin": [x, 0, 0], "normal": [normal, 0, 0]}})
        .to_string();
        let now: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
        let later = doc
            .prepare_query(&query)
            .and_then(|prepared| prepared.run(doc.kernel()))
            .unwrap();
        assert_eq!(later, now, "{query}");
        now
    };
    let counts = |answer: &Value| {
        [
            answer["cut"].as_u64().unwrap(),
            answer["kept"].as_u64().unwrap(),
            answer["removed"].as_u64().unwrap(),
        ]
    };
    for world in [true, false] {
        // Across the block: cut, with the section's area and outline.
        let across = answer(10.0, 1.0, world);
        assert_eq!(counts(&across), [1, 0, 0], "{across}");
        assert_eq!(across["bodies"][0]["clip"], "cut");
        assert!(
            (across["area"].as_f64().unwrap() - 400.0).abs() < 1e-6,
            "{across}"
        );
        assert!(
            (across["length"].as_f64().unwrap() - 80.0).abs() < 1e-6,
            "{across}"
        );
        assert_eq!(across["bodies"][0]["edges"], 4);

        // Past the block on the side the normal points away from: the
        // block stays whole and the plane cuts nothing.
        let behind = answer(30.0, 1.0, world);
        assert_eq!(counts(&behind), [0, 1, 0], "{behind}");
        assert_eq!(behind["bodies"][0]["clip"], "kept");
        assert_eq!(behind["area"], 0.0);
        assert_eq!(behind["length"], 0.0);

        // Past it on the normal's side: the clip removes all of it.
        let beyond = answer(-30.0, 1.0, world);
        assert_eq!(counts(&beyond), [0, 0, 1], "{beyond}");
        assert_eq!(beyond["bodies"][0]["clip"], "removed");
        assert_eq!(beyond["area"], 0.0);

        // Flip at the same place keeps it whole instead.
        let flipped = answer(-30.0, -1.0, world);
        assert_eq!(counts(&flipped), [0, 1, 0], "{flipped}");

        // A plane on the block's face, the block behind it: kept whole,
        // touching the plane at most, while the face is its section.
        let touching = answer(20.0, 1.0, world);
        assert_eq!(counts(&touching), [0, 1, 0], "{touching}");
        assert!(touching["area"].as_f64().unwrap() > 0.0, "{touching}");
    }
}
