// SPDX-License-Identifier: MIT
//! Component copies remap allowed values with the other parameter limits.

use serde_json::{Value, json};

use mitcad_model::testing::{MockKernel, unique_temp_dir};
use mitcad_model::{Document, InsertOptions};
use mitcad_model_api::DocumentApi;

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn block(component: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    if component {
        command(
            &mut doc,
            json!({"cmd": "create_component", "name": "Part", "activate": true}),
        );
    }
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
             "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude",
             "profiles": [{"sketch": "F1", "region": REGION}],
             "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    doc
}

#[test]
fn paste_new_allowed_values_follow_copied_model_parameters() {
    let mut doc = block(true);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "values": ["d1 / 2"],
               "min": "d1 / 5", "max": "d1", "step": "d1 / 10"}),
    );
    command(&mut doc, json!({"cmd": "paste_new", "occurrence": "O1"}));
    let copied = parameter(&doc, "d6");
    assert_eq!(copied["limits"]["values"], json!(["d4 / 2"]));
    assert_eq!(copied["limits"]["min"], "d4 / 5");
    assert_eq!(copied["limits"]["max"], "d4");
    assert_eq!(copied["limits"]["step"], "d4 / 10");
    assert_eq!(copied["dependencies"], json!(["d4"]));
    assert_eq!(
        query(&doc, json!({"query": "feature", "uid": "F4"}))["def"]["extent"]["distance"],
        "d6"
    );

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        query(&doc, json!({"query": "parameters"}))
            .as_array()
            .unwrap()
            .len(),
        3
    );
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(parameter(&doc, "d6"), copied);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "expression": "20 mm"}),
    );
    assert_eq!(parameter(&doc, "d3")["value"], 10.0);
    assert_eq!(parameter(&doc, "d6")["value"], 5.0);

    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(parameter(&reopened, "d6"), parameter(&doc, "d6"));
    command(
        &mut reopened,
        json!({"cmd": "set_parameter", "name": "d4", "expression": "30 mm"}),
    );
    assert_eq!(parameter(&reopened, "d6")["value"], 15.0);
    assert_eq!(parameter(&reopened, "d3")["value"], 10.0);
}

#[test]
fn copied_allowed_value_lists_keep_user_parameters_shared() {
    let mut doc = block(true);
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "Shared", "expression": "2", "unit": ""},
            {"cmd": "set_parameter", "name": "d3", "values": ["d1 / Shared", "d2 / Shared"]},
            {"cmd": "paste_new", "occurrence": "O1"}
        ]),
    );
    assert_eq!(
        parameter(&doc, "d6")["limits"]["values"],
        json!(["d4 / Shared", "d5 / Shared"])
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Shared", "expression": "4"}),
    );
    assert_eq!(parameter(&doc, "d3")["value"], 2.5);
    assert_eq!(parameter(&doc, "d6")["value"], 2.5);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "expression": "20 mm"}),
    );
    assert_eq!(parameter(&doc, "d6")["value"], 2.5);
}

fn sizes(doc: &mut Document<MockKernel>, name: &str, size: f64) {
    command(
        doc,
        json!({"cmd": "set_table", "name": name, "table": {
            "columns": [{"name": "Length", "unit": "mm"}], "rows": [[size]]}}),
    );
}

#[test]
fn editable_insert_allowed_values_follow_the_copied_table() {
    // A whole column and ordinary lookup formulas both follow a conflicting
    // table name; the destination's unrelated table keeps its values.
    for (written, expected) in [
        ("Sizes.Length", json!("Sizes_1.Length")),
        (
            "INDEX(Sizes.Length; 1)",
            json!(["INDEX(Sizes_1.Length; 1)"]),
        ),
    ] {
        let dir = unique_temp_dir("copy-allowed-values");
        let path = dir.join("block.mitcad");
        let mut source = block(false);
        sizes(&mut source, "Sizes", 5.0);
        command(
            &mut source,
            json!({"cmd": "set_parameter", "name": "d3", "values": [written]}),
        );
        std::fs::write(&path, source.to_json()).unwrap();
        let mut doc = Document::new(MockKernel::default());
        sizes(&mut doc, "Sizes", 100.0);
        doc.insert_component(
            "block.mitcad",
            &InsertOptions {
                link: false,
                base: Some(dir.clone()),
                ..InsertOptions::default()
            },
        )
        .unwrap();
        assert_eq!(parameter(&doc, "d3")["limits"]["values"], expected);
        assert_eq!(parameter(&doc, "d3")["value"], 5.0);
        command(&mut doc, json!({"cmd": "undo"}));
        assert_eq!(query(&doc, json!({"query": "parameters"})), json!([]));
        command(&mut doc, json!({"cmd": "redo"}));
        assert_eq!(parameter(&doc, "d3")["limits"]["values"], expected);
        let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
        assert_eq!(parameter(&reopened, "d3")["limits"]["values"], expected);
        sizes(&mut reopened, "Sizes", 200.0);
        assert_eq!(parameter(&reopened, "d3")["value"], 5.0);
        sizes(&mut reopened, "Sizes_1", 7.0);
        assert_eq!(parameter(&reopened, "d3")["value"], 7.0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
