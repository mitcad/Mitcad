// SPDX-License-Identifier: MIT
//! What a tutorial's page reads of its design through the bridge's `query`
//! (mitcad#2371, `tutorial_bridge.rs`): a 30 × 30 mm square that is free
//! to move told from one that is fully constrained while its sketch is
//! open; an Extrude Cut through a cube told from a blind pocket and from a
//! new body; commands, other queries, other designs and unknown fields
//! refused in words, and nothing changed by any of it.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::{DocumentApi, TUTORIAL_QUERIES, tutorial_query};

/// The design's name, as the window's bridge gives it.
const DESIGN: &str = "Lesson";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The page's query, with `open` the sketch open in the window.
fn read(doc: &Document<MockKernel>, request: Value, open: &str) -> Value {
    let answer = tutorial_query(doc, &request.to_string(), open, DESIGN)
        .unwrap_or_else(|e| panic!("{request}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// The page's query refused: the reason.
fn refused(doc: &Document<MockKernel>, request: Value, open: &str) -> String {
    match tutorial_query(doc, &request.to_string(), open, DESIGN) {
        Ok(answer) => panic!("{request} was answered: {answer}"),
        Err(error) => error.to_string(),
    }
}

fn near(value: &Value, expected: f64) -> bool {
    value.as_f64().is_some_and(|v| (v - expected).abs() < 1e-6)
}

/// The dimensions of the answer whose value is 30 mm.
fn thirties(sketch: &Value) -> usize {
    sketch["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| near(&d["value"], 30.0) && d["driven"] == json!(false))
        .count()
}

#[test]
fn a_free_square_is_told_from_a_fully_constrained_one_while_its_sketch_is_open() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}));
    let uid = sketch["uid"].as_str().unwrap().to_owned();
    let name = sketch["name"].clone();
    // A 30 × 30 mm square of four lines, horizontal and vertical, with two
    // sides dimensioned: it keeps its size but can still move.
    let square = command(
        &mut doc,
        json!({"cmd": "sketch.rectangle", "sketch": uid, "mode": "two_point",
               "a": [0, 0], "b": [30, 30]}),
    );
    let lines: Vec<String> = square["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(lines.len(), 4, "{square}");
    for line in &lines[..2] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_dimension", "sketch": uid,
                   "dimension": {"type": "length", "line": line}, "value": 30}),
        );
    }
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();

    let free = read(&doc, json!({"query": "sketch"}), &uid);
    assert_eq!(free["uid"], json!(uid), "{free}");
    assert_eq!(free["open"], json!(true));
    assert_eq!(free["plane"], json!("xy"));
    assert!(near(&free["normal"][2], 1.0), "{free}");
    assert_eq!(free["solved"], json!(true));
    assert_eq!(free["dof"], json!(2), "{free}");
    assert_eq!(free["fully_constrained"], json!(false));
    assert_eq!(thirties(&free), 2, "{free}");
    assert_eq!(free["curves"], json!({"line": 4}));
    assert_eq!(free["construction_curves"], json!(0));
    let profiles = free["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 1, "{free}");
    assert!(near(&profiles[0]["area"], 900.0), "{free}");
    // Only the answer's own fields: not the sketch query's whole answer.
    assert!(free.get("entities").is_none(), "{free}");

    // A corner fixed: nothing can move.
    let definition = query(&doc, json!({"query": "sketch", "uid": uid}));
    let corner = definition["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == json!(lines[0]))
        .map(|line| line["start"].clone())
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "sketch.set_fixed", "sketch": uid, "entities": [corner]}),
    );
    let fixed = read(&doc, json!({"query": "sketch"}), &uid);
    assert_eq!(fixed["dof"], json!(0), "{fixed}");
    assert_eq!(fixed["fully_constrained"], json!(true));
    assert_eq!(thirties(&fixed), 2, "{fixed}");

    // By name, with the sketch closed: the same sketch, not open.
    let named = read(&doc, json!({"query": "sketch", "name": name}), "");
    assert_eq!(named["uid"], json!(uid));
    assert_eq!(named["open"], json!(false));
    assert_eq!(named["fully_constrained"], json!(true));
    // No sketch open and none named: refused in words.
    let none = refused(&doc, json!({"query": "sketch"}), "");
    assert!(none.contains("No sketch is open"), "{none}");
    let missing = refused(&doc, json!({"query": "sketch", "name": "Sketch9"}), "");
    assert!(missing.contains("no sketch Sketch9"), "{missing}");

    // Reading changed nothing: one undo step, the corner's fix.
    let after = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    assert_eq!(after, json!(depth.as_u64().unwrap() + 1));
}

/// A 30 mm cube (F1, the body F1.b0) and a 10 mm circle sketched in the
/// middle of its bottom face's plane (F2): its region.
fn cube_and_circle() -> (Document<MockKernel>, Value) {
    let mut doc = Document::new(MockKernel::default());
    // The bodies' volumes are measured.
    doc.kernel().measures.set(true);
    let cube = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 30, "width": 30, "height": 30, "operation": "new_body"}}),
    );
    assert_eq!(cube["uid"], "F1", "{cube}");
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}));
    assert_eq!(sketch["uid"], "F2", "{sketch}");
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F2", "center": [15, 15], "diameter": 10}),
    );
    let region = circle["region"].clone();
    assert!(region.is_string(), "{circle}");
    (doc, region)
}

/// The cube and the circle extruded with `extent` and `operation`.
fn extruded(extent: Value, operation: &str) -> Document<MockKernel> {
    let (mut doc, region) = cube_and_circle();
    let mut def = json!({"type": "extrude", "profiles": [{"sketch": "F2", "region": region}],
                         "extent": extent, "operation": operation});
    if operation == "cut" {
        def["participants"] = json!(["F1.b0"]);
    }
    let added = command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    assert_eq!(added["uid"], "F3", "{added}");
    doc
}

#[test]
fn a_cut_through_a_cube_is_told_from_a_blind_pocket_and_from_a_new_body() {
    // Through All, cut: one body.
    let through = extruded(json!({"type": "through_all"}), "cut");
    let cut = read(&through, json!({"query": "feature", "type": "extrude"}), "");
    assert_eq!(cut["uid"], "F3", "{cut}");
    assert_eq!(cut["name"], "Extrude1");
    assert_eq!(cut["type"], "extrude");
    assert_eq!(cut["operation"], "cut");
    assert_eq!(cut["extent"]["type"], "through_all", "{cut}");
    assert_eq!(cut["suppressed"], json!(false));
    assert_eq!(cut["rolled_back"], json!(false));
    // Built: no error (the mock kernel may warn).
    assert!(cut["status"].is_string(), "{cut}");
    assert_eq!(cut["error"], Value::Null, "{cut}");
    // The last feature, unnamed, and the feature by name are the same.
    assert_eq!(read(&through, json!({"query": "feature"}), "")["uid"], "F3");
    assert_eq!(
        read(
            &through,
            json!({"query": "feature", "name": "Extrude1"}),
            ""
        )["uid"],
        "F3"
    );
    let bodies = read(&through, json!({"query": "bodies"}), "");
    assert_eq!(bodies["count"], json!(1), "{bodies}");
    assert_eq!(bodies["solids"], json!(1), "{bodies}");
    assert_eq!(bodies["bodies"][0]["uid"], "F1.b0");
    assert_eq!(bodies["bodies"][0]["kind"], "solid");
    assert_eq!(bodies["bodies"][0]["solid"], json!(true));
    assert!(
        bodies["bodies"][0]["volume"].as_f64().unwrap() > 0.0,
        "{bodies}"
    );

    // A blind pocket, 10 mm deep: a cut, but not through; its depth as a
    // value in mm, not a parameter's name.
    let blind = extruded(json!({"type": "distance", "distance": 10}), "cut");
    let pocket = read(&blind, json!({"query": "feature", "type": "extrude"}), "");
    assert_eq!(pocket["operation"], "cut");
    assert_eq!(pocket["extent"]["type"], "distance", "{pocket}");
    assert!(near(&pocket["extent"]["distance"], 10.0), "{pocket}");
    assert!(near(&pocket["def"]["extent"]["distance"], 10.0), "{pocket}");
    assert_eq!(
        read(&blind, json!({"query": "bodies"}), "")["count"],
        json!(1)
    );

    // A new body: a cylinder beside the cube, two solids.
    let new_body = extruded(json!({"type": "distance", "distance": 40}), "new_body");
    let cylinder = read(
        &new_body,
        json!({"query": "feature", "type": "extrude"}),
        "",
    );
    assert_eq!(cylinder["operation"], "new_body", "{cylinder}");
    let bodies = read(&new_body, json!({"query": "bodies"}), "");
    assert_eq!(bodies["count"], json!(2), "{bodies}");
    assert_eq!(bodies["solids"], json!(2), "{bodies}");

    // A type the design has none of, and a name of another type.
    let no_hole = refused(&new_body, json!({"query": "feature", "type": "hole"}), "");
    assert!(no_hole.contains("no feature of the type hole"), "{no_hole}");
    let not_sketch = refused(
        &new_body,
        json!({"query": "sketch", "name": "Extrude1"}),
        "",
    );
    assert!(not_sketch.contains("not a sketch"), "{not_sketch}");
    let unknown = refused(
        &new_body,
        json!({"query": "feature", "name": "Fillet7"}),
        "",
    );
    assert!(unknown.contains("no feature Fillet7"), "{unknown}");
}

#[test]
fn commands_other_queries_and_other_designs_are_refused_in_words() {
    let (doc, _) = cube_and_circle();
    let before = query(&doc, json!({"query": "document"}));

    // A command: the page only reads.
    let write = refused(&doc, json!({"cmd": "delete_feature", "uid": "F1"}), "");
    assert!(write.contains("only reads the design"), "{write}");
    assert!(write.contains("delete_feature"), "{write}");
    // A read-only model query that is not on the list.
    let other = refused(&doc, json!({"query": "timeline"}), "");
    assert!(other.contains("cannot read the query timeline"), "{other}");
    for name in TUTORIAL_QUERIES {
        assert!(other.contains(name), "{other}");
    }
    // No query at all, and not an object.
    let nameless = refused(&doc, json!({"name": "Sketch1"}), "");
    assert!(nameless.contains("needs \"query\""), "{nameless}");
    let text = refused(&doc, json!("bodies"), "");
    assert!(text.contains("is an object"), "{text}");
    // Another design: refused; this window's own: read.
    let elsewhere = refused(
        &doc,
        json!({"query": "bodies", "design": "Other design"}),
        "",
    );
    assert!(
        elsewhere.contains("only the design of its own window (Lesson)"),
        "{elsewhere}"
    );
    assert!(elsewhere.contains("Other design"), "{elsewhere}");
    let own = read(&doc, json!({"query": "bodies", "design": DESIGN}), "");
    assert_eq!(own["count"], json!(1));
    // A field the query does not take (a file, another document).
    let path = refused(
        &doc,
        json!({"query": "bodies", "path": "/elsewhere/design.mitcad"}),
        "",
    );
    assert!(path.contains("has no field \"path\""), "{path}");
    let document = refused(&doc, json!({"query": "sketch", "document": "d2"}), "F2");
    assert!(document.contains("has no field \"document\""), "{document}");

    // Nothing changed.
    assert_eq!(query(&doc, json!({"query": "document"})), before);
    assert_eq!(
        query(&doc, json!({"query": "bodies"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
