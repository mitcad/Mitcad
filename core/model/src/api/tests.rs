// SPDX-License-Identifier: MIT
//! The JSON command, query, preview and script interface.

use serde_json::{Value, json};

use crate::Document;
use crate::testing::MockKernel;

fn doc() -> Document<MockKernel> {
    Document::new(MockKernel::default())
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// Sketch1 with a 60 x 40 rectangle, extruded 20 mm.
fn block() -> Document<MockKernel> {
    let mut doc = doc();
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}));
    assert_eq!(sketch["uid"], "F1");
    assert_eq!(sketch["name"], "Sketch1");
    let rectangle = command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    assert_eq!(
        rectangle,
        json!({"curves": ["c1", "c2", "c3", "c4"], "region": REGION,
               "entities": ["c1", "c2", "c3", "c4", "p5", "p6", "p7", "p8"],
               "constraints": ["k1", "k2", "k3", "k4"], "dimensions": ["k5", "k6"],
               "parameters": ["d1", "d2"],
               "status": {"dof": 0, "fully_constrained": ["c1", "c2", "c3", "c4", "p5", "p6", "p7", "p8"],
                          "redundant": [], "conflicts": []},
               "recomputed": 1, "error": null})
    );
    let extrude = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    assert_eq!(
        extrude,
        json!({"uid": "F2", "name": "Extrude1", "parameters": ["d3"], "recomputed": 1, "error": null})
    );
    doc
}

#[test]
fn commands_build_a_part_and_queries_describe_it() {
    let mut doc = block();
    assert_eq!(
        query(&doc, json!({"query": "timeline"})),
        json!({"marker": 2, "features": [
            {"uid": "F1", "name": "Sketch1", "type": "sketch", "suppressed": false, "status": "ok", "error": null,
             "dof": 0, "visible": false, "visible_set": false},
            {"uid": "F2", "name": "Extrude1", "type": "extrude", "suppressed": false, "status": "ok", "error": null}
        ]})
    );
    assert_eq!(
        query(&doc, json!({"query": "parameters"}))[2],
        json!({"name": "d3", "expression": "20 mm", "unit": "mm", "value": 20.0, "text": "20 mm",
               "comment": "Extrude1 distance", "kind": "model", "owner": "F2", "dependencies": [],
               "favorite": false})
    );
    assert_eq!(
        query(&doc, json!({"query": "bodies"})),
        json!([{"uid": "F2.b0", "name": "Body1"}])
    );
    assert_eq!(
        query(&doc, json!({"query": "profiles"})),
        json!([{"sketch": "F1", "sketch_name": "Sketch1", "region": REGION, "consumed": true}])
    );
    assert_eq!(
        query(&doc, json!({"query": "feature", "uid": "F2"}))["def"],
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": "d3"}, "operation": "new_body"})
    );
    assert_eq!(
        query(&doc, json!({"query": "document"})),
        json!({"features": 2, "marker": 2, "bodies": 1, "undo": "Add Extrude1", "redo": null,
               "units": {"length": "mm", "angle": "deg"}, "warnings": [], "undo_depth": 3,
               "display": {"origin": false, "isolated": []}, "revision": 3, "modified": true})
    );

    // The definition from the feature query edits back unchanged.
    let def = query(&doc, json!({"query": "feature", "uid": "F2"}))["def"].clone();
    let edit = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": def}),
    );
    assert_eq!(
        edit,
        json!({"parameters": [], "recomputed": 0, "error": null})
    );

    let fillet = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 50}}),
    );
    assert_eq!(fillet["uid"], "F3");
    doc.kernel().fail.borrow_mut().insert("fillet");
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d4", "value": 60}),
    );
    assert_eq!(
        changed,
        json!({"changed": ["d4"], "recomputed": 1, "error": "Fillet1: fillet failed"})
    );
    assert_eq!(
        query(&doc, json!({"query": "timeline"}))["features"][2]["status"],
        "error"
    );
    assert_eq!(
        command(&mut doc, json!({"cmd": "undo"}))["label"],
        "Change d4"
    );
    assert_eq!(
        command(&mut doc, json!({"cmd": "redo"}))["label"],
        "Change d4"
    );
    for cmd in [
        json!({"cmd": "suppress_feature", "uid": "F3"}),
        json!({"cmd": "suppress_feature", "uid": "F3", "suppressed": false}),
        json!({"cmd": "set_marker", "position": 2}),
        json!({"cmd": "set_marker", "position": 3}),
        json!({"cmd": "rename_feature", "uid": "F3", "name": "Round"}),
        json!({"cmd": "rename_body", "uid": "F2.b0", "name": "Base"}),
        json!({"cmd": "add_parameter", "name": "wall", "value": 2, "comment": "user"}),
        json!({"cmd": "rename_parameter", "name": "wall", "new_name": "shell"}),
        json!({"cmd": "delete_parameter", "name": "shell"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "reorder_feature", "uid": "F4", "index": 0}),
        json!({"cmd": "sketch.add_circle", "sketch": "F4", "center": [5, 5], "diameter": "d1"}),
        json!({"cmd": "recompute"}),
    ] {
        command(&mut doc, cmd);
    }
    assert_eq!(
        command(
            &mut doc,
            json!({"cmd": "delete_feature", "uid": "F2", "dependents": true})
        ),
        json!({"deleted": ["F2", "F3"], "recomputed": 0, "error": null})
    );
    assert_eq!(query(&doc, json!({"query": "bodies"})), json!([]));
}

#[test]
fn parameters_take_expressions_and_units() {
    let mut doc = block();
    let result = command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "wall", "value": "d1 / 20", "comment": "rim"}),
    );
    assert_eq!(result["recomputed"], 0);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "tilt", "expression": "15", "unit": "deg"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "gap", "value": 1.5}),
    );
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": "wall * 4 + gap"}),
    );
    assert_eq!(changed["changed"], json!(["d3"]));
    assert_eq!(changed["recomputed"], 1);
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_eq!(changed["changed"], json!(["d1", "d3", "wall"]));
    let params = query(&doc, json!({"query": "parameters"}));
    assert_eq!(
        params[3],
        json!({"name": "wall", "expression": "d1 / 20", "unit": "mm", "value": 4.0, "text": "4 mm",
               "comment": "rim", "kind": "user", "owner": null, "dependencies": ["d1"],
               "favorite": false})
    );
    assert_eq!(params[2]["value"], 17.5);
    assert_eq!(params[4]["text"], "15 deg");
    // A new unit keeps the expression; an angle unit does not fit a length.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "gap", "unit": "cm", "comment": "air"}),
    );
    let params = query(&doc, json!({"query": "parameters"}));
    assert_eq!(
        (
            &params[5]["expression"],
            &params[5]["unit"],
            &params[5]["value"]
        ),
        (&json!("1.5 mm"), &json!("cm"), &json!(1.5))
    );
    assert_eq!(params[5]["comment"], "air");
    let error = doc
        .command(r#"{"cmd": "set_parameter", "name": "d3", "unit": "deg"}"#)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "parameter 'd3' cannot change to 'deg': Extrude1 uses it as a length"
    );
    let error = doc
        .command(r#"{"cmd": "add_parameter", "name": "x", "value": 1, "expression": "2"}"#)
        .unwrap_err();
    assert!(error.to_string().contains("not both"), "{error}");
    // Document units: bare numbers of new dimensions are inches.
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    assert_eq!(
        query(&doc, json!({"query": "document"}))["units"],
        json!({"length": "in", "angle": "deg"})
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "pin", "value": 25.4}),
    );
    let params = query(&doc, json!({"query": "parameters"}));
    assert_eq!(params[6]["expression"], "1 in");
}

#[test]
fn rejected_commands_and_queries_say_why() {
    let mut doc = block();
    let error =
        |doc: &mut Document<MockKernel>, json: &str| doc.command(json).unwrap_err().to_string();
    assert!(error(&mut doc, "{").starts_with("not valid JSON"));
    assert!(
        error(&mut doc, r#"{"cmd": "fly"}"#).contains("invalid command: unknown variant `fly`")
    );
    assert!(error(&mut doc, r#"{"cmd": "set_marker", "at": 2}"#).contains("unknown field `at`"),);
    assert_eq!(
        error(&mut doc, r#"{"cmd": "delete_feature", "uid": "F1"}"#),
        "Sketch1 is used by Extrude1; delete them too, or change them first"
    );
    assert_eq!(
        error(
            &mut doc,
            r#"{"cmd": "set_parameter", "name": "x", "value": 1}"#
        ),
        "unknown parameter 'x'"
    );
    // A batch stops at the first failure; earlier commands stay applied.
    let batch = r#"[{"cmd": "sketch.create"}, {"cmd": "set_marker", "position": 9}]"#;
    assert_eq!(
        error(&mut doc, batch),
        "commands[1]: marker position 9 is past the end of the timeline (3)"
    );
    assert_eq!(query(&doc, json!({"query": "document"}))["features"], 3);
    let query_error = doc
        .query(r#"{"query": "faces", "body": "F9.b0"}"#)
        .unwrap_err();
    assert_eq!(
        query_error.to_string(),
        "body F9.b0 does not exist at the timeline marker"
    );
    // The mock kernel measures nothing.
    let unsupported = doc
        .query(r#"{"query": "bodies", "properties": true}"#)
        .unwrap_err();
    assert_eq!(
        unsupported.to_string(),
        "Body1: the geometry kernel does not support mass properties"
    );
}

#[test]
fn a_batch_returns_every_result() {
    let mut doc = doc();
    let results = doc
        .command(
            &json!([
                {"cmd": "sketch.create"},
                {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
                {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
                 "extent": {"type": "symmetric", "distance": 5}, "operation": "new_body"}}
            ])
            .to_string(),
        )
        .unwrap();
    let results: Value = serde_json::from_str(&results).unwrap();
    assert_eq!(results.as_array().unwrap().len(), 3);
    assert_eq!(results[2]["uid"], "F2");
    assert_eq!(
        doc.body_shape("F2.b0".parse().unwrap()).unwrap().history,
        "prism(F2:-5..5)"
    );
}

#[test]
fn previews_report_without_committing() {
    let mut doc = block();
    let cut = json!({"cmd": "add_feature", "def": {"type": "extrude",
                     "profiles": [{"sketch": "F1", "region": REGION}],
                     "extent": {"type": "distance", "distance": 5}, "operation": "cut",
                     "participants": ["F2.b0"]}});
    let report: Value = serde_json::from_str(&doc.preview(&cut.to_string()).unwrap()).unwrap();
    assert_eq!(
        report,
        json!({"uid": "F3", "name": "Extrude2", "status": "ok", "error": null, "warnings": [],
               "bodies": [{"uid": "F2.b0", "name": "Body1", "changed": true}],
               "removed": [], "tool": true})
    );
    assert_eq!(query(&doc, json!({"query": "document"}))["features"], 2);
    let edit = json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
                      "profiles": [{"sketch": "F1", "region": REGION}],
                      "extent": {"type": "distance", "distance": 35}, "operation": "new_body"}});
    doc.preview(&edit.to_string()).unwrap();
    assert_eq!(
        doc.preview_body("F2.b0".parse().unwrap()).unwrap().history,
        "prism(F2:0..35)"
    );
    assert_eq!(
        doc.body_shape("F2.b0".parse().unwrap()).unwrap().history,
        "prism(F2:0..20)"
    );
    assert!(doc.preview(r#"{"cmd": "undo"}"#).is_err());
}

#[test]
fn scripts_check_expectations() {
    let mut doc = block();
    let script = json!([
        {"comment": "a hole through the block"},
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 20], "diameter": 10},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
         "extent": {"type": "through_all"}, "operation": "cut"}},
        {"expect": {"bodies": 1, "body": "Body1",
                    "has_face": "F4:side(c1)", "has_edge": "E{F4:side(c1)|F4:side(c1)}",
                    "no_edge": "E{F2:side(c1[c4,c2])|F2:side(c3[c2,c4])}"}},
        {"expect": {"feature": "Extrude2", "status": "ok"}},
        {"expect": {"parameter": "d3", "value": 20}},
    ]);
    let results: Value =
        serde_json::from_str(&doc.run_script(&script.to_string()).unwrap()).unwrap();
    assert_eq!(results.as_array().unwrap().len(), 3);

    let mut doc = block();
    let mut failing = |step: Value| {
        doc.run_script(&json!([step]).to_string())
            .unwrap_err()
            .to_string()
    };
    assert_eq!(
        failing(json!({"expect": {"bodies": 2}})),
        "steps[0]: expected 2 bodies, got 1: Body1"
    );
    assert_eq!(
        failing(json!({"expect": {"feature": "Extrude1", "status": "error"}})),
        "steps[0]: expected Extrude1 to be error, got ok "
    );
    assert_eq!(
        failing(json!({"expect": {"body": "Body1", "has_face": "F9:top"}})),
        "steps[0]: expected Body1 to have a face F9:top"
    );
    assert_eq!(
        failing(json!({"expect": {"body": "Body1", "volume": 1}})),
        "steps[0]: Body1: the geometry kernel does not support mass properties"
    );
    assert_eq!(
        failing(json!({"expect": {"parameter": "d3", "value": 21}})),
        "steps[0]: expected d3 = 21, got 20"
    );
    assert!(failing(json!({"expect": {"bodys": 1}})).contains("unknown field `bodys`"));
    assert!(failing(json!({"cmd": "jump"})).starts_with("steps[0]: invalid command"));
}

// Sketch mode (U2): the commands of one interactive operation undo at once.
#[test]
fn merged_commands_undo_as_one_step() {
    let mut doc = doc();
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    assert_eq!(depth, 1);
    command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": "c1"}}),
    );
    let merged = command(
        &mut doc,
        json!({"cmd": "merge_undo", "depth": depth, "label": "Add Line to Sketch1"}),
    );
    assert_eq!(merged["undo_depth"], 2);
    let document = query(&doc, json!({"query": "document"}));
    assert_eq!(document["undo"], "Add Line to Sketch1");
    assert_eq!(
        command(&mut doc, json!({"cmd": "undo"}))["label"],
        "Add Line to Sketch1"
    );
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert_eq!(sketch["entities"], json!([]));
    assert_eq!(sketch["constraints"], json!([]));
    let error = doc
        .command(&json!({"cmd": "merge_undo", "depth": 5, "label": "x"}).to_string())
        .unwrap_err();
    assert_eq!(error.to_string(), "undo depth 5 is past the last step (1)");
}

#[test]
fn a_dimension_text_moves() {
    let mut doc = block();
    let moved = command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5",
               "text": [30, -8]}),
    );
    assert_eq!(moved["dimensions"], json!([]));
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert_eq!(sketch["dimensions"][0]["text"], json!([30.0, -8.0]));
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Move Dimension in Sketch1"
    );
}

#[test]
fn profile_hashes_follow_the_geometry() {
    let mut doc = block();
    let hash = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "profiles", "hashes": true}))[0]["hash"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let first = hash(&doc);
    assert_eq!(first.len(), 16);
    // A new name, or a value set back, changes no geometry.
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F1", "name": "Plate"}),
    );
    assert_eq!(hash(&doc), first);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 70}),
    );
    assert_ne!(hash(&doc), first);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(hash(&doc), first);
    // Volumes alone come from the kernel's mass properties, which the mock
    // kernel does not measure.
    let unsupported = doc
        .query(r#"{"query": "bodies", "volumes": true}"#)
        .unwrap_err();
    assert_eq!(
        unsupported.to_string(),
        "Body1: the geometry kernel does not support mass properties"
    );
}

#[test]
fn recompute_times_list_the_features_the_last_recompute_evaluated() {
    let mut doc = block();
    // A new distance evaluates the extrusion again, not the sketch.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 25}),
    );
    let times = query(&doc, json!({"query": "recompute_times"}));
    let features = times["features"].as_array().unwrap();
    assert_eq!(features.len(), 1);
    assert_eq!(features[0]["uid"], "F2");
    assert_eq!(features[0]["name"], "Extrude1");
    assert_eq!(features[0]["type"], "extrude");
    assert!(features[0]["ms"].as_f64().unwrap() >= 0.0);
    assert_eq!(times["ms"], features[0]["ms"]);
    // As text (mitcad-cli info --timings): the slowest first.
    let text = doc
        .analysis(r#"{"query": "recompute_times"}"#, false)
        .unwrap();
    assert!(text.starts_with("Recompute: 1 features in "), "{text}");
    assert!(text.contains("\n  F2 Extrude1 (extrude): "), "{text}");
}

// The saved state (P8).
#[test]
fn mark_saved_and_the_document_query_tell_unsaved_changes() {
    let mut doc = block();
    let state = |doc: &Document<MockKernel>| {
        let document = query(doc, json!({"query": "document"}));
        (document["revision"].clone(), document["modified"].clone())
    };
    assert_eq!(state(&doc), (json!(3), json!(true)));
    // No undo step, nothing recomputed.
    assert_eq!(
        command(&mut doc, json!({"cmd": "mark_saved"})),
        json!({"revision": 3, "recomputed": 0, "error": null})
    );
    assert_eq!(state(&doc), (json!(3), json!(false)));
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Add Extrude1"
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 25}),
    );
    assert_eq!(state(&doc), (json!(4), json!(true)));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(state(&doc), (json!(3), json!(false)));
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(state(&doc), (json!(4), json!(true)));
    command(&mut doc, json!({"cmd": "undo"}));
    // A recompute and a preview change nothing.
    command(&mut doc, json!({"cmd": "recompute"}));
    let mut def = query(&doc, json!({"query": "feature", "uid": "F2"}))["def"].clone();
    def["extent"]["distance"] = json!(30);
    doc.preview(&json!({"cmd": "edit_feature", "uid": "F2", "def": def}).to_string())
        .unwrap();
    assert_eq!(state(&doc), (json!(3), json!(false)));

    // A project file read is modified until marked saved: the application
    // marks a file it opens, not an imported design.
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        query(&opened, json!({"query": "document"}))["modified"],
        true
    );
    command(&mut opened, json!({"cmd": "recompute"}));
    command(&mut opened, json!({"cmd": "mark_saved"}));
    assert_eq!(
        query(&opened, json!({"query": "document"}))["modified"],
        false
    );
}

#[test]
fn the_changes_since_saved_are_the_undo_steps_since() {
    let mut doc = block();
    let changes = |doc: &Document<MockKernel>| query(doc, json!({"query": "changes_since_saved"}));
    // A document read is saved nowhere: unknown.
    let opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(changes(&opened), json!({"known": false, "steps": []}));
    command(&mut doc, json!({"cmd": "mark_saved"}));
    assert_eq!(changes(&doc), json!({"known": true, "steps": []}));
    command(
        &mut doc,
        json!({"cmd": "set_origin_visible", "visible": true}),
    );
    assert_eq!(changes(&doc), json!({"known": true, "steps": []}));
    command(&mut doc, json!({"cmd": "undo"}));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 25}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F2", "name": "Plate"}),
    );
    assert_eq!(
        changes(&doc),
        json!({"known": true, "steps": ["Change d3", "Rename Extrude1 to Plate"]})
    );
    // The display state is not versioned: its steps are left out.
    command(
        &mut doc,
        json!({"cmd": "set_origin_visible", "visible": true}),
    );
    assert_eq!(
        changes(&doc),
        json!({"known": true, "steps": ["Change d3", "Rename Extrude1 to Plate"]})
    );
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        changes(&doc),
        json!({"known": true, "steps": ["Change d3"]})
    );
    // Saved here, then undone past the saved state.
    command(&mut doc, json!({"cmd": "mark_saved"}));
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        changes(&doc),
        json!({"known": true, "steps": ["Undo Change d3", "Undo Add Extrude1"]})
    );
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(
        changes(&doc),
        json!({"known": true, "steps": ["Undo Change d3"]})
    );
    // A new command after the undo drops the saved state's step.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 30}),
    );
    assert_eq!(changes(&doc), json!({"known": false, "steps": []}));
}
