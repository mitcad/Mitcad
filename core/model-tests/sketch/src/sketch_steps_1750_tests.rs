// SPDX-License-Identifier: MIT
//! The work steps of a sketch (mitcad#1750): which use of a tool made which
//! items, recorded by the `sketch.*` commands, merged with their undo
//! steps, kept through undo, redo and deletes, and listed by the `sketch`
//! query's `steps`; a sketch without them (an older design, an import)
//! lists each item as a step of its own.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    let query = json!({"query": "sketch", "uid": "F1"});
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The `sketch` query's steps as (id, kind, items).
fn steps(doc: &Document<MockKernel>) -> Vec<(Option<String>, Option<String>, Vec<String>)> {
    sketch(doc)["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| {
            (
                step["id"].as_str().map(str::to_owned),
                step["kind"].as_str().map(str::to_owned),
                strings(&step["items"]),
            )
        })
        .collect()
}

/// Everything a command's answer says it made.
fn made(answer: &Value) -> Vec<String> {
    let mut items = strings(&answer["entities"]);
    items.extend(strings(&answer["constraints"]));
    items.extend(strings(&answer["dimensions"]));
    items.sort();
    items
}

fn sorted(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items
}

fn rectangle(doc: &mut Document<MockKernel>) -> Value {
    command(
        doc,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [0, 0], "b": [20, 10]}),
    )
}

#[test]
fn each_command_records_one_step_of_what_it_made() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let rectangle = rectangle(&mut d);
    assert_eq!(rectangle["step"], "s1");
    let line = strings(&rectangle["made"])[0].clone();
    let dimension = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": line}, "value": 25}),
    );
    assert_eq!(dimension["step"], "s2");
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [40, 0], "radius": 5}),
    );
    assert_eq!(circle["step"], "s3");

    let listed = steps(&d);
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert_eq!(listed[0].0.as_deref(), Some("s1"));
    assert_eq!(listed[0].1.as_deref(), Some("rectangle"));
    // The rectangle's lines, corners and constraints in one step.
    assert_eq!(sorted(listed[0].2.clone()), made(&rectangle));
    assert!(listed[0].2.len() > 4);
    assert_eq!(listed[1].1.as_deref(), Some("add_dimension"));
    assert_eq!(listed[1].2, strings(&dimension["dimensions"]));
    assert_eq!(listed[2].1.as_deref(), Some("circle"));
    assert_eq!(sorted(listed[2].2.clone()), made(&circle));

    // Every item in exactly one step.
    let s = sketch(&d);
    let mut all = sorted(strings(&s["order"]));
    let mut stepped: Vec<String> = listed.iter().flat_map(|s| s.2.clone()).collect();
    stepped.sort();
    all.dedup();
    assert_eq!(stepped, all);

    // A command that makes nothing records no step.
    let drag = command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1",
               "dimension": strings(&dimension["dimensions"])[0], "value": 30}),
    );
    assert!(drag.get("step").is_none(), "{drag}");
    assert_eq!(steps(&d).len(), 3);
}

#[test]
fn undo_and_redo_keep_the_steps() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    rectangle(&mut d);
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [40, 0], "radius": 5}),
    );
    let both = steps(&d);
    assert_eq!(both.len(), 2);
    command(&mut d, json!({"cmd": "undo"}));
    let one = steps(&d);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].1.as_deref(), Some("rectangle"));
    command(&mut d, json!({"cmd": "redo"}));
    assert_eq!(steps(&d), both);
    assert_eq!(sorted(steps(&d)[1].2.clone()), made(&circle));
    // A step made after an undo takes the free number again.
    command(&mut d, json!({"cmd": "undo"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 30], "end": [10, 30]}),
    );
    assert_eq!(line["step"], "s2");
    assert_eq!(steps(&d)[1].1.as_deref(), Some("add_line"));
}

#[test]
fn commands_merged_into_one_undo_step_are_one_step() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    rectangle(&mut d);
    // A tool's line with the constraint its snap adds, and the axis an
    // input pick made first: three commands, then merged as sketch mode
    // merges a tool's commands.
    let depth = d.undo_depth();
    command(
        &mut d,
        json!({"cmd": "sketch.axis", "sketch": "F1", "axis": "x"}),
    );
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 30], "end": [10, 31]}),
    );
    let made_line = strings(&line["entities"])
        .into_iter()
        .find(|id| id.starts_with('c'))
        .unwrap();
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": made_line}}),
    );
    assert_eq!(steps(&d).len(), 4);
    command(
        &mut d,
        json!({"cmd": "merge_undo", "depth": depth, "label": "Line in Sketch1"}),
    );
    let listed = steps(&d);
    assert_eq!(listed.len(), 2, "{listed:?}");
    // The tool's own kind, not the axis made for its input.
    assert_eq!(listed[1].1.as_deref(), Some("add_line"));
    assert!(listed[1].2.contains(&made_line));
    assert!(listed[1].2.iter().any(|id| id.starts_with('k')));
    // One undo takes the whole step back, redo brings it back merged.
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(steps(&d).len(), 1);
    command(&mut d, json!({"cmd": "redo"}));
    assert_eq!(steps(&d), listed);
}

#[test]
fn deleted_items_leave_their_step_and_an_emptied_step_goes() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let rectangle = rectangle(&mut d);
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [40, 0], "radius": 5}),
    );
    // One of the rectangle's constraints: the rest stays its step.
    let constraint = strings(&rectangle["constraints"])[0].clone();
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [constraint]}),
    );
    let listed = steps(&d);
    assert_eq!(listed.len(), 2);
    assert!(!listed[0].2.contains(&constraint));
    assert!(!listed[0].2.is_empty());
    // The circle with its centre: its step goes.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": strings(&circle["entities"])}),
    );
    let listed = steps(&d);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].1.as_deref(), Some("rectangle"));
    // Undo brings the circle's step back.
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(steps(&d).len(), 2);
}

#[test]
fn a_sketch_without_steps_lists_each_item_alone() {
    // As a design saved before steps were recorded, or an import: the
    // definition has no steps.
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "entities": [
            {"id": "p1", "type": "point", "at": [0, 0]},
            {"id": "p2", "type": "point", "at": [10, 0]},
            {"id": "c3", "type": "line", "start": "p1", "end": "p2"}
        ]}}),
    );
    let listed = steps(&d);
    assert_eq!(
        listed,
        [
            (None, None, vec!["p1".to_owned()]),
            (None, None, vec!["p2".to_owned()]),
            (None, None, vec!["c3".to_owned()]),
        ]
    );
    // An edit now records what it makes; the older items stay alone.
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [40, 0], "radius": 5}),
    );
    let listed = steps(&d);
    assert_eq!(listed.len(), 4, "{listed:?}");
    assert_eq!(listed[3].1.as_deref(), Some("circle"));
    assert_eq!(sorted(listed[3].2.clone()), made(&circle));
    // The definition keeps them.
    let def = d
        .query(&json!({"query": "feature", "uid": "F1"}).to_string())
        .unwrap();
    assert!(def.contains("\"steps\""), "{def}");
}

#[test]
fn an_extension_is_a_step_of_its_own_naming_the_curve_it_changed() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [25, -5], "end": [25, 5]}),
    );
    let line = strings(&sketch(&d)["steps"][0]["items"])
        .into_iter()
        .find(|id| id.starts_with('c'))
        .unwrap();
    let extended = command(
        &mut d,
        json!({"cmd": "sketch.extend", "sketch": "F1", "curve": line, "at": [9, 0]}),
    );
    let step = extended["step"]
        .as_str()
        .expect("the extension's step")
        .to_owned();
    let s = sketch(&d);
    let listed = s["steps"].as_array().unwrap();
    assert_eq!(listed.len(), 3, "{listed:?}");
    // The line stays its own step's; the extension lists it as changed.
    assert!(strings(&listed[0]["items"]).contains(&line));
    assert_eq!(listed[2]["id"], step);
    assert_eq!(listed[2]["kind"], "extend");
    assert!(strings(&listed[2]["items"]).contains(&line));
    assert_eq!(strings(&listed[2]["changed"]), std::slice::from_ref(&line));
    // Undo takes the extension's step back.
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(steps(&d).len(), 2);
}

#[test]
fn a_text_is_listed_as_its_steps_item() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let text = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "Label", "at": [0, 0], "height": 5}),
    );
    let made = strings(&text["texts"]);
    assert_eq!(made.len(), 1);
    let listed = steps(&d);
    let step = listed
        .iter()
        .find(|s| s.1.as_deref() == Some("add_text"))
        .unwrap_or_else(|| panic!("no text step in {listed:?}"));
    assert!(step.2.contains(&made[0]), "{listed:?}");
}
