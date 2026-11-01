// SPDX-License-Identifier: MIT
//! Tables in the design (mitcad#667) through the api: `set_table`,
//! `rename_table`, `rename_table_column`, `delete_table` and the `tables`
//! query, and the table columns in `parameters`, `evaluate` and
//! `formula_help`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let result = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn screws() -> Value {
    json!({
        "comment": "ISO 4762 socket head cap screws",
        "columns": [
            {"name": "Size", "unit": "Text"},
            {"name": "d", "unit": "mm"},
            {"name": "dk", "unit": "mm"}],
        "rows": [["M3", 3, 5.5], ["M4", 4, 7], ["M5", 5, 8.5]]
    })
}

/// Sketch1 with a 60 x 40 rectangle (d1, d2) extruded by d3, which looks
/// up the head diameter of Size = 'M5' in Screws.
fn block_with_screws() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
            {"cmd": "add_feature", "def": {"type": "extrude",
             "profiles": [{"sketch": "F1", "region": REGION}],
             "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "set_table", "name": "Screws", "table": screws()},
            {"cmd": "add_parameter", "name": "Size", "expression": "'M5'"},
            {"cmd": "set_parameter", "name": "d3",
             "expression": "XLOOKUP(Size; Screws.Size; Screws.dk)"},
        ]),
    );
    doc
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap()
}

#[test]
fn tables_are_set_and_queried() {
    let mut doc = block_with_screws();
    assert_eq!(parameter(&doc, "d3")["value"], 8.5);
    let tables = query(&doc, json!({"query": "tables"}));
    let listed = &tables["tables"][0];
    assert_eq!(listed["name"], "Screws");
    assert_eq!(listed["comment"], "ISO 4762 socket head cap screws");
    assert_eq!(
        listed["columns"][0],
        json!({"name": "Size", "unit": "Text"})
    );
    assert_eq!(listed["rows"][2], json!(["M5", 5.0, 8.5]));
    assert_eq!(listed["used_by"], json!(["d3"]));
    assert_eq!(listed["problems"], json!([]));
    assert_eq!(
        query(&doc, json!({"query": "tables", "name": "Screws"}))["tables"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        doc.query(&json!({"query": "tables", "name": "Nuts"}).to_string())
            .is_err()
    );

    // An edit: the parameters whose values changed, one undo step.
    let mut table = screws();
    table["rows"][2][2] = json!(9);
    let result = command(
        &mut doc,
        json!({"cmd": "set_table", "name": "Screws", "table": table}),
    );
    assert_eq!(result["changed"], json!(["d3"]));
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Edit Table Screws"
    );
    assert_eq!(parameter(&doc, "d3")["value"], 9.0);
    // A refused one names the parameter and changes nothing.
    let mut table = screws();
    table["rows"].as_array_mut().unwrap().pop();
    let error = rejected(
        &mut doc,
        json!({"cmd": "set_table", "name": "Screws", "table": table}),
    );
    assert!(error.contains("parameter d3"), "{error}");
    assert_eq!(parameter(&doc, "d3")["value"], 9.0);
    // Unknown fields and cells of the wrong kind are refused.
    rejected(
        &mut doc,
        json!({"cmd": "set_table", "name": "T", "table": {"columns": [], "colour": 1}}),
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "set_table", "name": "T",
               "table": {"columns": [{"name": "n", "unit": "mm"}], "rows": [["x"]]}}),
    );
    assert!(error.contains("row 1, column n"), "{error}");
}

#[test]
fn renames_and_deletes_through_the_api() {
    let mut doc = block_with_screws();
    command(
        &mut doc,
        json!({"cmd": "rename_table", "name": "Screws", "new_name": "Bolts"}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_table_column", "table": "Bolts", "name": "dk", "new_name": "head"}),
    );
    assert_eq!(
        parameter(&doc, "d3")["expression"],
        "XLOOKUP(Size; Bolts.Size; Bolts.head)"
    );
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Rename Column Bolts.dk to head"
    );
    let error = rejected(&mut doc, json!({"cmd": "delete_table", "name": "Bolts"}));
    assert_eq!(error, "table Bolts is used by d3");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "expression": "20 mm"}),
    );
    command(&mut doc, json!({"cmd": "delete_table", "name": "Bolts"}));
    assert_eq!(query(&doc, json!({"query": "tables"}))["tables"], json!([]));
}

#[test]
fn queries_name_the_table_columns_expressions_use() {
    let doc = block_with_screws();
    assert_eq!(
        parameter(&doc, "d3")["table_dependencies"],
        json!(["Screws.Size", "Screws.dk"])
    );
    assert_eq!(parameter(&doc, "d1")["table_dependencies"], json!([]));
    let evaluated = query(
        &doc,
        json!({"query": "evaluate", "expression": "INDEX(Screws.d; 2) * 2"}),
    );
    assert_eq!(evaluated["value"], 8.0);
    assert_eq!(evaluated["table_references"], json!(["Screws.d"]));
    assert_eq!(evaluated["references"], json!([]));
    let help = query(&doc, json!({"query": "formula_help"}));
    assert_eq!(
        help["tables"],
        json!([{"name": "Screws", "columns": [
            {"name": "Size", "unit": "Text"},
            {"name": "d", "unit": "mm"},
            {"name": "dk", "unit": "mm"}]}])
    );
}
