// SPDX-License-Identifier: MIT
//! Minimum Distance (mitcad#2519) on the mock kernel
//! (`MockKernel::measures_selections`: the closest points of the picks'
//! boxes): the `minimum_distance` query's distance and closest points for
//! two boxes apart and touching, a clearance that marks the pair or not, a
//! component standing for its bodies with the far pairs left unmeasured,
//! the analysis kept with `MinDistance<n>` names and measured again when the
//! model changes, refused definitions, a body that is gone said in words
//! until an undo brings it back, and saved and read back.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const TOL: f64 = 1e-6;

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
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "analyses"}))
        .as_array()
        .unwrap()
        .clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn near(value: &Value, expected: f64) {
    let actual = value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} is no number"));
    assert!(
        (actual - expected).abs() <= TOL,
        "{actual} instead of {expected}"
    );
}

fn near_point(value: &Value, expected: [f64; 3]) {
    for (i, e) in expected.iter().enumerate() {
        near(&value[i], *e);
    }
}

/// A 10 mm square on `sketch` at `corner`, extruded 10 mm (or by the
/// parameter `height`) into a new body.
fn cube(sketch: &str, corner: [f64; 2], height: Value) -> Value {
    json!([
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": corner, "width": 10, "height": 10},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
            "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
    ])
}

/// Body1 (F2.b0), a 10 mm square at the origin as high as the parameter
/// `height` (10 mm), and the component Block placed once (O1) at
/// (0, 0, 15) with two 10 mm cubes: F4.b0 at its origin (in the design
/// x 0 to 10, z 15 to 25) and F6.b0 at x 40 in it (x 40 to 50).
fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "height", "value": "10 mm"}),
    );
    command(&mut doc, cube("F1", [0.0, 0.0], json!("height")));
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Block", "transform": {"translation": [0, 0, 15]}}),
    );
    command(&mut doc, cube("F3", [0.0, 0.0], json!(10)));
    command(&mut doc, cube("F5", [40.0, 0.0], json!(10)));
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    doc
}

fn body1() -> Value {
    json!({"body": "F2.b0"})
}

fn block_cube() -> Value {
    json!({"body": "F4.b0", "occurrence": "O1"})
}

fn distance(doc: &Document<MockKernel>, first: Value, second: Value) -> Value {
    query(
        doc,
        json!({"query": "minimum_distance", "first": [first], "second": [second]}),
    )
}

#[test]
fn two_boxes_five_mm_apart_and_touching() {
    let mut doc = design();
    // Body1's top at z 10, the Block's cube's bottom at z 15.
    let apart = distance(&doc, body1(), block_cube());
    near(&apart["distance"], 5.0);
    near_point(&apart["points"][0], [5.0, 5.0, 10.0]);
    near_point(&apart["points"][1], [5.0, 5.0, 15.0]);
    assert_eq!(apart["touching"], false);
    assert_eq!(apart["first"]["body"], "F2.b0");
    assert_eq!(apart["first"]["pick"], 0);
    assert_eq!(apart["second"]["body"], "F4.b0");
    assert_eq!(apart["second"]["occurrence"], "O1");
    // No clearance asked: none answered.
    assert!(apart.get("within_clearance").is_none(), "{apart}");
    // The other way round: the points swap.
    let back = distance(&doc, block_cube(), body1());
    near(&back["distance"], 5.0);
    near_point(&back["points"][0], [5.0, 5.0, 15.0]);
    near_point(&back["points"][1], [5.0, 5.0, 10.0]);
    // A face and a body: the top face is as far.
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    near(&distance(&doc, top, block_cube())["distance"], 5.0);

    // Body1 as high as 15 mm touches the Block's cube: 0.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 15}),
    );
    let touching = distance(&doc, body1(), block_cube());
    near(&touching["distance"], 0.0);
    assert_eq!(touching["touching"], true);
    near_point(&touching["points"][0], [5.0, 5.0, 15.0]);
    near_point(&touching["points"][1], [5.0, 5.0, 15.0]);
}

#[test]
fn a_clearance_marks_the_pair_closer_than_it() {
    let doc = design();
    let ask = |clearance: f64| {
        query(
            &doc,
            json!({"query": "minimum_distance", "first": [body1()], "second": [block_cube()],
                   "clearance": clearance}),
        )
    };
    let marked = ask(6.0);
    near(&marked["distance"], 5.0);
    near(&marked["clearance"], 6.0);
    assert_eq!(marked["within_clearance"], true);
    let clear = ask(4.0);
    near(&clear["clearance"], 4.0);
    assert_eq!(clear["within_clearance"], false);
    // As far as the clearance is not closer than it.
    assert_eq!(ask(5.0)["within_clearance"], false);
    for bad in [0.0, -1.0] {
        let error = query_error(
            &doc,
            json!({"query": "minimum_distance", "first": [body1()], "second": [block_cube()],
                   "clearance": bad}),
        );
        assert!(
            error.contains("the clearance must be greater than zero"),
            "{error}"
        );
    }
}

#[test]
fn a_component_stands_for_its_bodies_and_far_pairs_are_not_measured() {
    let doc = design();
    // The Block's two cubes: the nearer one is measured, the other's box is
    // farther than 5 mm and is not.
    let found = distance(&doc, body1(), json!({"occurrence": "O1"}));
    near(&found["distance"], 5.0);
    assert_eq!(found["second"]["body"], "F4.b0");
    assert_eq!(found["second"]["occurrence"], "O1");
    assert_eq!(found["second"]["pick"], 0);
    assert_eq!(found["pairs"], 2);
    assert_eq!(found["measured"], 1);
    // The far cube alone: from Body1's corner edge (10, y, 10) to its
    // (40, y, 15).
    let far = distance(&doc, body1(), json!({"body": "F6.b0", "occurrence": "O1"}));
    near(&far["distance"], (30.0f64 * 30.0 + 5.0 * 5.0).sqrt());
    near_point(&far["points"][0], [10.0, 5.0, 10.0]);
    near_point(&far["points"][1], [40.0, 5.0, 15.0]);
    // Several picks on a side: the closest pair, by the pick it came from.
    let several = query(
        &doc,
        json!({"query": "minimum_distance",
               "first": [{"body": "F6.b0", "occurrence": "O1"}, body1()],
               "second": [{"occurrence": "O1"}]}),
    );
    // F6.b0 against the Block's own F6.b0 is the same pick, left out; F6.b0
    // and F4.b0 are 30 mm apart; Body1 and F4.b0 5 mm.
    near(&several["distance"], 5.0);
    assert_eq!(several["first"]["pick"], 1);
    assert_eq!(several["first"]["body"], "F2.b0");
    // A body against itself: its only pair is the pick with itself.
    let same = query(
        &doc,
        json!({"query": "minimum_distance", "first": [body1()], "second": [body1()]}),
    );
    assert_eq!(same["error"], "the two selections are the same");
}

#[test]
fn kept_and_measured_again_when_the_model_changes() {
    let mut doc = design();
    let def = json!({"type": "minimum_distance", "first": [body1()], "second": [{"occurrence": "O1"}],
                     "clearance": 6});
    let added = command(&mut doc, json!({"cmd": "add_analysis", "def": def}));
    assert_eq!(added["name"], "MinDistance1");
    assert_eq!(undo_label(&doc), "Add MinDistance1");
    let listed = analyses(&doc);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["type"], "minimum_distance");
    assert_eq!(listed[0]["visible"], true);
    assert_eq!(listed[0]["first"], json!([{"body": "F2.b0"}]));
    assert_eq!(listed[0]["second"], json!([{"occurrence": "O1"}]));
    near(&listed[0]["clearance"], 6.0);
    assert!(listed[0].get("error").is_none(), "{}", listed[0]);

    let kept = |doc: &Document<MockKernel>| {
        query(
            doc,
            json!({"query": "minimum_distance", "name": "MinDistance1"}),
        )
    };
    let first = kept(&doc);
    near(&first["distance"], 5.0);
    assert_eq!(first["within_clearance"], true);
    // A parameter change moves Body1's top: measured again, 3 mm.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 12}),
    );
    let closer = kept(&doc);
    near(&closer["distance"], 3.0);
    near_point(&closer["points"][0], [5.0, 5.0, 12.0]);
    // The Block moved up: 13 mm, clear of the clearance.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "O1",
               "transform": {"translation": [0, 0, 25]}}),
    );
    let moved = kept(&doc);
    near(&moved["distance"], 13.0);
    assert_eq!(moved["within_clearance"], false);
    // Edited: a larger clearance marks it again.
    let wider = json!({"type": "minimum_distance", "first": [body1()], "second": [{"occurrence": "O1"}],
                       "clearance": 20});
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "MinDistance1", "def": wider}),
    );
    assert_eq!(undo_label(&doc), "Edit MinDistance1");
    assert_eq!(kept(&doc)["within_clearance"], true);
    // One analysis shown at a time: a section hides it.
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy"}}),
    );
    let shown: Vec<bool> = analyses(&doc)
        .iter()
        .map(|a| a["visible"].as_bool().unwrap())
        .collect();
    assert_eq!(shown, [false, true]);
    // A second one is MinDistance2.
    let second = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "minimum_distance",
               "first": [body1()], "second": [block_cube()]}}),
    );
    assert_eq!(second["name"], "MinDistance2");
    // Without a clearance: no clearance answered.
    let plain = query(
        &doc,
        json!({"query": "minimum_distance", "name": "MinDistance2"}),
    );
    assert!(plain.get("within_clearance").is_none(), "{plain}");
    // A section is not one.
    let error = query_error(
        &doc,
        json!({"query": "minimum_distance", "name": "Section1"}),
    );
    assert!(error.contains("not a minimum distance analysis"), "{error}");
}

#[test]
fn definitions_are_refused_with_the_reason() {
    let mut doc = design();
    let add = |first: Value, second: Value, clearance: Option<f64>| {
        let mut def = json!({"type": "minimum_distance", "first": first, "second": second});
        if let Some(clearance) = clearance {
            def["clearance"] = json!(clearance);
        }
        json!({"cmd": "add_analysis", "def": def})
    };
    for (cmd, expected) in [
        (
            add(json!([]), json!([body1()]), None),
            "needs a first selection",
        ),
        (
            add(json!([body1()]), json!([]), None),
            "needs a second selection",
        ),
        (
            add(json!([{"datum": "xy"}]), json!([body1()]), None),
            "not datums",
        ),
        (
            add(
                json!([{"face": format!("F2:end({R})")}]),
                json!([body1()]),
                None,
            ),
            "needs its body",
        ),
        (
            add(json!([{}]), json!([body1()]), None),
            "needs a body, or a component's occurrence",
        ),
        (
            add(json!([body1()]), json!([block_cube()]), Some(0.0)),
            "the clearance must be greater than zero",
        ),
        (
            add(json!([{"body": "F9.b0"}]), json!([body1()]), None),
            "does not exist at the timeline marker",
        ),
        (
            add(json!([body1()]), json!([{"occurrence": "O7"}]), None),
            "O7",
        ),
    ] {
        let error = refused(&mut doc, cmd.clone());
        assert!(error.contains(expected), "{cmd}: {error}");
    }
    let unknown = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "minimum_distance", "first": [body1()],
               "second": [block_cube()], "gap": 1}}),
    );
    assert!(unknown.contains("gap"), "{unknown}");
    assert!(analyses(&doc).is_empty());
    // The query wants a name or the picks, not both, not neither.
    let both = query_error(
        &doc,
        json!({"query": "minimum_distance", "name": "MinDistance1", "first": [body1()]}),
    );
    assert!(both.contains("not both"), "{both}");
    let neither = query_error(&doc, json!({"query": "minimum_distance"}));
    assert!(
        neither.contains("a first and a second selection"),
        "{neither}"
    );
}

#[test]
fn a_body_that_is_gone_is_said_until_an_undo_brings_it_back() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "minimum_distance",
               "first": [body1()], "second": [{"body": "F6.b0", "occurrence": "O1"}]}}),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F6"}));
    let listed = analyses(&doc);
    let error = listed[0]["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("body F6.b0 does not exist at the timeline marker"),
        "{}",
        listed[0]
    );
    let answer = query(
        &doc,
        json!({"query": "minimum_distance", "name": "MinDistance1"}),
    );
    assert_eq!(answer["error"], error);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(analyses(&doc)[0].get("error").is_none());
    let back = query(
        &doc,
        json!({"query": "minimum_distance", "name": "MinDistance1"}),
    );
    near(&back["distance"], (30.0f64 * 30.0 + 5.0 * 5.0).sqrt());
}

#[test]
fn minimum_distances_are_saved_and_read_back() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "minimum_distance",
               "first": [body1(), {"body": "F2.b0", "face": format!("F2:end({R})")}],
               "second": [{"occurrence": "O1"}], "clearance": 6}}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "MinDistance1", "type": "minimum_distance",
             "first": [{"body": "F2.b0"}, {"body": "F2.b0", "face": format!("F2:end({R})")}],
             "second": [{"occurrence": "O1"}], "clearance": 6.0}
        ])
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().measures_selections.set(true);
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    let answer = query(
        &loaded,
        json!({"query": "minimum_distance", "name": "MinDistance1"}),
    );
    near(&answer["distance"], 5.0);
    assert_eq!(answer["within_clearance"], true);
}
