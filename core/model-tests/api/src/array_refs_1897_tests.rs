// SPDX-License-Identifier: MIT
//! References between the entries of a `model_command` array (mitcad#1897),
//! with the mock kernel: a parametric closed outline with a clockwise arc,
//! its constraints and dimensions and the extrusion of it, built in one
//! array without predicting an id; unknown, forward and wrongly typed
//! references; a refused entry undoing the entries before it, and a failed
//! feature stopping the array with the earlier work kept.
//!
//! [`run`] runs an array as both MCP servers do (`core/ffi/src/mcp.rs`
//! `run_commands`, `app/MainWindowMcp.cpp`), with the shared rules of
//! `mitcad_mcp` (`check_refs`, `resolve_entry`, `entry_failed`); the
//! servers' own runs are `tools/cli/mcp-array-refs-test.cmake`.

use serde_json::{Value, json};

use mitcad_mcp::{check_refs, entry_failed, resolve_entry};
use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn undo_depth(doc: &Document<MockKernel>) -> u64 {
    query(doc, json!({"query": "document"}))["undo_depth"]
        .as_u64()
        .unwrap()
}

/// A profile picked by a point inside it becomes its region, on the
/// document as the entries before left it (as `mcp_resolve_command`).
fn profiles_by_point(doc: &Document<MockKernel>, mut command: Value) -> Result<Value, String> {
    let Some(profiles) = command
        .pointer("/def/profiles")
        .and_then(Value::as_array)
        .cloned()
    else {
        return Ok(command);
    };
    let mut picked = Vec::with_capacity(profiles.len());
    for profile in &profiles {
        let request = match (profile.get("sketch"), profile.get("point")) {
            (Some(sketch), Some(point)) => {
                json!({"query": "profiles", "sketch": sketch, "point": point})
            }
            _ => {
                picked.push(profile.clone());
                continue;
            }
        };
        let found: Value =
            serde_json::from_str(&doc.query(&request.to_string()).map_err(|e| e.to_string())?)
                .unwrap();
        let [one] = found.as_array().map(Vec::as_slice).unwrap_or_default() else {
            return Err(format!("no single profile region: {request} found {found}"));
        };
        picked.push(json!({"sketch": one["sketch"], "region": one["region"]}));
    }
    command["def"]["profiles"] = Value::Array(picked);
    Ok(command)
}

/// Runs `entries` as one `model_command` array: names and references
/// checked before anything runs; each entry's references replaced by the
/// results before it right before it runs; a refused entry undoes the
/// entries before it (`applied` false, `entry`, `undone`); an entry whose
/// added or edited feature fails stops the array, applied (`stopped_at`).
fn run(doc: &mut Document<MockKernel>, entries: &[Value]) -> Value {
    let list: Vec<&Value> = entries.iter().collect();
    if let Err((entry, error)) = check_refs(&list) {
        return json!({"applied": false, "entry": entry, "error": error, "undone": 0});
    }
    let depth = undo_depth(doc);
    let mut results: Vec<Value> = Vec::new();
    let mut stopped = Value::Null;
    for i in 0..list.len() {
        let ran = resolve_entry(&list, i, &results)
            .and_then(|command| profiles_by_point(doc, command))
            .and_then(|command| doc.command(&command.to_string()).map_err(|e| e.to_string()));
        match ran {
            Ok(text) => {
                let result: Value = serde_json::from_str(&text).unwrap();
                let failed = entry_failed(&result);
                results.push(result);
                if failed && i + 1 < list.len() {
                    stopped = json!(i);
                    break;
                }
            }
            Err(error) => {
                let mut undone = 0;
                while undo_depth(doc) > depth {
                    command(doc, json!({"cmd": "undo"}));
                    undone += 1;
                }
                return json!({"applied": false, "entry": i, "undone": undone,
                              "error": format!("commands[{i}]: {error}")});
            }
        }
    }
    json!({"applied": true, "ran": results.len(), "results": results, "stopped_at": stopped})
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

fn entity(s: &Value, id: &Value) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == *id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn assert_at(s: &Value, id: &Value, expected: [f64; 2]) {
    let at = &entity(s, id)["at"];
    let (x, y) = (at[0].as_f64().unwrap(), at[1].as_f64().unwrap());
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * b.abs().max(1.0);
    assert!(
        close(x, expected[0]) && close(y, expected[1]),
        "{id} is at ({x}, {y}), not {expected:?}"
    );
}

/// The parameters, then a closed outline on Sketch1 drawn by references:
/// a line along the bottom, one up the right side, a clockwise arc across
/// the top that sags inwards, and a line down the left side to the first
/// point; its constraints and the arc's radius; then its extrusion. Its
/// coordinates follow w and h, the arc's sag is 10 at w = 60 (radius 50).
fn outline() -> Vec<Value> {
    vec![
        json!({"cmd": "add_parameter", "name": "w", "value": 60}),
        json!({"cmd": "add_parameter", "name": "h", "value": 40}),
        json!({"cmd": "add_parameter", "name": "t", "value": 5}),
        json!({"cmd": "sketch.create", "as": "sk"}),
        json!({"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"}, "as": "l1",
               "start": ["0", "0"], "end": ["w", 0]}),
        json!({"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"}, "as": "l2",
               "start": {"ref": "l1.end"}, "end": ["w", "h"]}),
        json!({"cmd": "sketch.arc", "sketch": {"ref": "sk.uid"}, "as": "a1",
               "mode": "three_point", "start": {"ref": "l2.end"}, "through": [30, 30],
               "end": [0, "h"]}),
        json!({"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"}, "as": "l3",
               "start": {"ref": "a1.end"}, "end": {"ref": "l1.start"}}),
        json!({"cmd": "sketch.add_constraint", "sketch": {"ref": "sk.uid"},
               "constraint": {"type": "horizontal", "line": {"ref": "l1.made.0"}}}),
        json!({"cmd": "sketch.add_constraint", "sketch": {"ref": "sk.uid"},
               "constraint": {"type": "vertical", "line": {"ref": "l3.made.0"}}}),
        json!({"cmd": "sketch.add_dimension", "sketch": {"ref": "sk.uid"}, "as": "radius",
               "dimension": {"type": "radius", "curve": {"ref": "a1.made.0"}}}),
        json!({"cmd": "add_feature", "as": "body",
               "def": {"type": "extrude",
                       "profiles": [{"sketch": {"ref": "sk.uid"}, "point": [30, 10]}],
                       "extent": {"type": "distance", "distance": "t"},
                       "operation": "new_body"}}),
    ]
}

#[test]
fn a_parametric_outline_with_a_clockwise_arc_and_its_extrusion_in_one_array() {
    let mut d = Document::new(MockKernel::default());
    let entries = outline();
    let ran = run(&mut d, &entries);
    assert_eq!(ran["applied"], true, "{ran}");
    assert_eq!(ran["ran"], entries.len(), "{ran}");
    assert!(ran["stopped_at"].is_null(), "{ran}");
    let results = &ran["results"];
    let sk = results[3]["uid"].as_str().unwrap().to_owned();
    // Fully constrained and dimensioned once the arc's radius is in.
    assert_eq!(results[10]["status"]["dof"], 0, "{}", results[10]);
    // The extrusion of the outline made a body.
    assert!(
        results[11].get("failed_features").is_none(),
        "{}",
        results[11]
    );
    assert_eq!(
        query(&d, json!({"query": "bodies"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // One undo step for the whole array is the servers' merge_undo; here
    // each entry is one.
    let s = sketch(&d, &sk);
    let (first, corner) = (&results[4]["inputs"]["start"], &results[4]["inputs"]["end"]);
    let (top_right, top_left) = (&results[5]["inputs"]["end"], &results[6]["inputs"]["end"]);
    assert_at(&s, first, [0.0, 0.0]);
    assert_at(&s, corner, [60.0, 0.0]);
    assert_at(&s, top_right, [60.0, 40.0]);
    assert_at(&s, top_left, [0.0, 40.0]);
    // The references joined the curves: l2 starts at l1's end, the arc at
    // l2's end, l3 runs from the arc's end as written to the first point.
    assert_eq!(results[5]["inputs"]["start"], *corner);
    assert_eq!(results[6]["inputs"]["start"], *top_right);
    assert_eq!(results[7]["inputs"]["start"], *top_left);
    assert_eq!(results[7]["inputs"]["end"], *first);
    // The arc is clockwise as written: stored counter-clockwise from its
    // end to its start, while a1.end named the end as written.
    let arc = entity(&s, &results[6]["made"][0]);
    assert_eq!(arc["start"], *top_left, "{arc}");
    assert_eq!(arc["end"], *top_right, "{arc}");

    // Changing the parameters moves the geometry they drive.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "w", "value": 80}),
    );
    let s = sketch(&d, &sk);
    assert!(s["error"].is_null(), "{s}");
    assert_at(&s, corner, [80.0, 0.0]);
    assert_at(&s, top_right, [80.0, 40.0]);
    assert_at(&s, top_left, [0.0, 40.0]);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "h", "value": 45}),
    );
    let s = sketch(&d, &sk);
    assert_at(&s, top_right, [80.0, 45.0]);
    assert_at(&s, top_left, [0.0, 45.0]);
    assert_at(&s, first, [0.0, 0.0]);
}

#[test]
fn unknown_and_forward_references_refuse_the_array_before_it_runs() {
    let mut d = Document::new(MockKernel::default());
    let mut forward = outline();
    // l2 refers to the arc, which comes after it.
    forward[5]["start"] = json!({"ref": "a1.end"});
    let ran = run(&mut d, &forward);
    assert_eq!(ran["applied"], false, "{ran}");
    assert_eq!(ran["entry"], 5, "{ran}");
    assert_eq!(ran["undone"], 0, "{ran}");
    assert!(
        ran["error"]
            .as_str()
            .unwrap()
            .contains("runs after this one"),
        "{ran}"
    );
    let mut unknown = outline();
    unknown[7]["end"] = json!({"ref": "l0.start"});
    let ran = run(&mut d, &unknown);
    assert_eq!(ran["entry"], 7, "{ran}");
    assert!(
        ran["error"]
            .as_str()
            .unwrap()
            .contains("no entry of the array is named “l0”"),
        "{ran}"
    );
    // Nothing ran.
    assert_eq!(undo_depth(&d), 0);
    assert_eq!(query(&d, json!({"query": "parameters"})), json!([]));
}

#[test]
fn a_reference_that_names_nothing_or_the_wrong_kind_undoes_the_entries_before_it() {
    let mut d = Document::new(MockKernel::default());
    for (i, field, bad, says) in [
        // A curve where a line takes a point.
        (7, "end", json!({"ref": "l1.made.0"}), "not a point"),
        // A path the result does not have.
        (5, "start", json!({"ref": "l1.finish"}), "has no finish"),
        // The arc made no point at its through.
        (7, "start", json!({"ref": "a1.through"}), "is null"),
    ] {
        let mut entries = outline();
        entries[i][field] = bad;
        let ran = run(&mut d, &entries);
        assert_eq!(ran["applied"], false, "{ran}");
        assert_eq!(ran["entry"], i, "{ran}");
        assert!(ran["undone"].as_u64().unwrap() > 0, "{ran}");
        let error = ran["error"].as_str().unwrap();
        assert!(error.starts_with(&format!("commands[{i}]: ")), "{error}");
        assert!(error.contains(says), "{error}");
        // The entries before it are undone: no parameter, no sketch.
        assert_eq!(undo_depth(&d), 0, "{ran}");
        assert_eq!(query(&d, json!({"query": "parameters"})), json!([]));
        let timeline = query(&d, json!({"query": "timeline"}));
        assert_eq!(timeline["features"], json!([]), "{timeline}");
    }
}

#[test]
fn a_failed_feature_stops_the_array_and_keeps_what_ran() {
    let mut d = Document::new(MockKernel::default());
    d.kernel().fail.borrow_mut().insert("extrude");
    let mut entries = outline();
    // An entry after the extrusion, which does not run.
    entries.push(json!({"cmd": "rename_feature", "uid": {"ref": "body.uid"},
                        "name": "Plate"}));
    let ran = run(&mut d, &entries);
    assert_eq!(ran["applied"], true, "{ran}");
    assert_eq!(ran["stopped_at"], 11, "{ran}");
    assert_eq!(ran["ran"], 12, "{ran}");
    let failed = &ran["results"][11]["failed_features"];
    assert_eq!(failed[0]["edited"], true, "{ran}");
    // The sketch and the failed extrusion stay in the timeline.
    let timeline = query(&d, json!({"query": "timeline"}));
    let features = timeline["features"].as_array().unwrap();
    assert_eq!(features.len(), 2, "{timeline}");
    assert_eq!(features[1]["type"], "extrude", "{timeline}");
    assert_eq!(features[1]["status"], "error", "{timeline}");
}
