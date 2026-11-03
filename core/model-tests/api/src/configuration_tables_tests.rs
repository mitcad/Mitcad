// SPDX-License-Identifier: MIT
//! Tables in configurations (mitcad#806): option values from a table
//! column, a table edit that moves an option, the rule action `in_table`,
//! and rows generated from a table (`generate_configuration_rows`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn doc() -> Document<MockKernel> {
    Document::new(MockKernel::default())
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(error) => error.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn text(doc: &Document<MockKernel>, name: &str) -> String {
    parameter(doc, name)["text"].as_str().unwrap().to_owned()
}

fn reopened(doc: &Document<MockKernel>) -> Document<MockKernel> {
    Document::from_json(&doc.to_json(), MockKernel::default()).unwrap()
}

/// The texts of a list domain of the `configure` query.
fn texts(domain: &Value) -> Vec<String> {
    domain["values"]
        .as_array()
        .unwrap_or_else(|| panic!("not a list: {domain}"))
        .iter()
        .map(|v| v["text"].as_str().unwrap().to_owned())
        .collect()
}

/// Sets the table Screws with the given sizes and head diameters.
fn set_screws(doc: &mut Document<MockKernel>, rows: &[(&str, f64)]) -> Value {
    let rows: Vec<Value> = rows.iter().map(|(size, dk)| json!([size, dk])).collect();
    command(
        doc,
        json!({"cmd": "set_table", "name": "Screws", "table": {
            "columns": [{"name": "Size", "unit": "Text"}, {"name": "dk", "unit": "mm"}],
            "rows": rows}}),
    )
}

/// A design whose option Size takes its values from Screws.Size, and whose
/// head diameter follows by a lookup.
fn screws() -> Document<MockKernel> {
    let mut doc = doc();
    set_screws(&mut doc, &[("M3", 5.5), ("M4", 7.0), ("M5", 8.5)]);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Size", "expression": "'M4'",
               "values": "Screws.Size"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "HeadDia",
               "expression": "XLOOKUP(Size; Screws.Size; Screws.dk)"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {"options": ["Size"]}}),
    );
    doc
}

#[test]
fn an_options_values_come_from_a_table_column() {
    let mut doc = screws();
    let size = parameter(&doc, "Size");
    assert_eq!(size["limits"]["values"], "Screws.Size", "{size}");
    assert_eq!(size["limits"]["values_text"], json!(["M3", "M4", "M5"]));
    assert_eq!(text(&doc, "HeadDia"), "7 mm");
    // The chooser lists the column.
    let report = query(&doc, json!({"query": "configure"}));
    assert_eq!(texts(&report["options"][0]["domain"]), ["M3", "M4", "M5"]);
    assert_eq!(texts(&report["options"][0]["allowed"]), ["M3", "M4", "M5"]);
    // A value of the column is chosen; the lookup follows.
    command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Size": "M5"}}),
    );
    assert_eq!(text(&doc, "Size"), "M5");
    assert_eq!(text(&doc, "HeadDia"), "8.5 mm");
    // A text that is no cell is refused.
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Size", "expression": "'M7'"}),
    );
    assert!(e.contains("not one of its allowed values"), "{e}");
    assert_eq!(text(&doc, "Size"), "M5");
    // Saved and read back as the column.
    let again = reopened(&doc);
    assert_eq!(parameter(&again, "Size")["limits"]["values"], "Screws.Size");
    // The table cannot be deleted while the option's values use it.
    let e = refused(&mut doc, json!({"cmd": "delete_table", "name": "Screws"}));
    assert!(e.contains("Size"), "{e}");
    // A column of the wrong kind is refused.
    let e = refused(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Head", "expression": "7 mm",
               "values": "Screws.Size", "strict": true}),
    );
    assert!(!e.is_empty());
}

#[test]
fn a_table_edit_that_removes_the_current_value_moves_the_option() {
    let mut doc = screws();
    command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Size": "M5"}}),
    );
    assert_eq!(text(&doc, "HeadDia"), "8.5 mm");
    // M5 leaves the table: Size moves to the column's first value.
    let answer = set_screws(&mut doc, &[("M3", 5.5), ("M4", 7.0), ("M6", 10.0)]);
    assert_eq!(text(&doc, "Size"), "M3", "{answer}");
    assert_eq!(text(&doc, "HeadDia"), "5.5 mm");
    let moves = answer["moves"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"));
    assert_eq!(moves.len(), 1, "{answer}");
    assert_eq!(moves[0]["option"], "Size");
    assert_eq!(moves[0]["from"], "M5");
    assert_eq!(moves[0]["to"], "M3");
    assert_eq!(moves[0]["to_expression"], "'M3'");
    assert_eq!(moves[0]["rule"], "its possible values Screws.Size");
    let report = query(&doc, json!({"query": "configure"}));
    assert_eq!(texts(&report["options"][0]["domain"]), ["M3", "M4", "M6"]);
    // The edit and the move are one undo step.
    doc.undo().unwrap();
    assert_eq!(text(&doc, "Size"), "M5");
    assert_eq!(text(&doc, "HeadDia"), "8.5 mm");
    // An edit that keeps the value moves nothing.
    let answer = set_screws(&mut doc, &[("M3", 5.5), ("M5", 8.7)]);
    assert_eq!(text(&doc, "Size"), "M5");
    assert_eq!(text(&doc, "HeadDia"), "8.7 mm");
    assert!(
        answer
            .get("moves")
            .is_none_or(|m| m.as_array().is_some_and(|m| m.is_empty())),
        "{answer}"
    );
}

#[test]
fn a_number_option_moves_to_the_nearest_value_of_its_column() {
    let mut doc = doc();
    let lengths = |values: &[f64]| -> Value {
        let rows: Vec<Value> = values.iter().map(|v| json!([v])).collect();
        json!({"cmd": "set_table", "name": "Lengths", "table": {
            "columns": [{"name": "L", "unit": "mm"}], "rows": rows}})
    };
    command(&mut doc, lengths(&[300.0, 500.0, 800.0]));
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "500 mm",
               "values": "Lengths.L"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {"options": ["Length"]}}),
    );
    let answer = command(&mut doc, lengths(&[300.0, 450.0, 800.0]));
    assert_eq!(text(&doc, "Length"), "450 mm", "{answer}");
    assert_eq!(answer["moves"][0]["from"], "500 mm", "{answer}");
    assert_eq!(answer["moves"][0]["to"], "450 mm");
}

/// Color and Length, made only in the combinations of the table Made.
fn made() -> Document<MockKernel> {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "set_table", "name": "Made", "table": {
            "columns": [{"name": "Color", "unit": "Text"}, {"name": "Length", "unit": "mm"}],
            "rows": [["Black", 500], ["Black", 800], ["White", 800], ["White", 1200]]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "expression": "'Black'",
               "values": ["'Black'", "'White'"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "800 mm",
               "min": "300 mm", "max": "2000 mm", "step": "100 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color", "Length"],
            "rules": [{"name": "Made sizes", "then": [
                {"in_table": "Made", "columns": {"Color": "Color", "Length": "Length"}}]}]}}),
    );
    doc
}

#[test]
fn in_table_allows_only_the_tables_combinations() {
    let mut doc = made();
    // The chooser of Length lists the lengths made in Black.
    let report = query(&doc, json!({"query": "configure"}));
    assert_eq!(
        texts(&report["options"][1]["allowed"]),
        ["500 mm", "800 mm"]
    );
    assert_eq!(report["options"][1]["restricted_by"], json!(["Made sizes"]));
    // Black in 1200 mm is no row: refused, nothing changes.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Length": "1200 mm"}}),
    );
    assert_eq!(answer["refused"]["rule"], "Made sizes", "{answer}");
    assert!(
        answer["refused"]["text"]
            .as_str()
            .unwrap()
            .contains("Length = 1200 mm"),
        "{answer}"
    );
    assert_eq!(text(&doc, "Length"), "800 mm");
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "1200 mm"}),
    );
    assert!(e.contains("Made sizes"), "{e}");
    // The combination is checked whole: the query lists what is broken.
    let report = query(
        &doc,
        json!({"query": "configure", "values": {"Length": "1200 mm"}}),
    );
    assert_eq!(report["refused"]["rule"], "Made sizes", "{report}");
    // White in 1200 mm is a row.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Color": "White", "Length": "1200 mm"}}),
    );
    assert_eq!(answer["refused"], Value::Null, "{answer}");
    assert_eq!(text(&doc, "Length"), "1200 mm");
    // Back to Black: Length moves to the nearest length made in Black.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Color": "Black"}}),
    );
    assert_eq!(answer["refused"], Value::Null, "{answer}");
    assert_eq!(text(&doc, "Length"), "800 mm");
    assert_eq!(answer["moves"][0]["option"], "Length");
    assert_eq!(answer["moves"][0]["rule"], "Made sizes");
    // Saved, and renames follow.
    let again = reopened(&doc);
    let table = query(&again, json!({"query": "configurations"}));
    assert_eq!(
        table["configurations"]["rules"][0]["then"][0],
        json!({"in_table": "Made", "columns": {"Color": "Color", "Length": "Length"}})
    );
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Color", "new_name": "Finish"}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_table", "name": "Made", "new_name": "Production"}),
    );
    let table = query(&doc, json!({"query": "configurations"}));
    assert_eq!(
        table["configurations"]["rules"][0]["then"][0],
        json!({"in_table": "Production", "columns": {"Finish": "Color", "Length": "Length"}})
    );
    let e = refused(
        &mut doc,
        json!({"cmd": "delete_table", "name": "Production"}),
    );
    assert!(e.contains("Made sizes"), "{e}");
}

#[test]
fn an_in_table_action_with_a_wrong_column_is_a_problem() {
    let mut doc = made();
    let e = refused(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color", "Length"],
            "rules": [{"name": "Made sizes", "then": [
                {"in_table": "Made", "columns": {"Color": "Length"}}]}]}}),
    );
    assert!(e.contains("is a number column"), "{e}");
    let e = refused(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color", "Length"],
            "rules": [{"name": "Made sizes", "then": [
                {"in_table": "Nowhere", "columns": {"Color": "Color"}}]}]}}),
    );
    assert!(e.contains("Nowhere"), "{e}");
}

#[test]
fn generating_rows_twice_updates_the_rows_and_does_not_duplicate_them() {
    let mut doc = screws();
    let answer = command(
        &mut doc,
        json!({"cmd": "generate_configuration_rows", "table": "Screws", "column": "Size",
               "option": "Size"}),
    );
    assert_eq!(answer["rows"], json!(["M3", "M4", "M5"]), "{answer}");
    assert_eq!(answer["added"], json!(["M3", "M4", "M5"]));
    assert_eq!(answer["updated"], json!([]));
    assert_eq!(doc.undo_label(), Some("Generate Rows from Screws"));
    let table = query(&doc, json!({"query": "configurations"}));
    let rows = table["configurations"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2]["name"], "M5");
    assert_eq!(rows[2]["values"], json!({"Size": "'M5'"}));
    // A designation written by hand stays; a new key adds a row.
    let mut configurations = table["configurations"].clone();
    configurations["rows"][1]["designation"] = json!("ISO 4762 M4");
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": configurations}),
    );
    set_screws(
        &mut doc,
        &[("M3", 5.5), ("M4", 7.0), ("M5", 8.5), ("M6", 10.0)],
    );
    let answer = command(
        &mut doc,
        json!({"cmd": "generate_configuration_rows", "table": "Screws", "column": "Size",
               "option": "Size"}),
    );
    assert_eq!(answer["added"], json!(["M6"]), "{answer}");
    assert_eq!(answer["updated"], json!([]));
    let table = query(&doc, json!({"query": "configurations"}));
    let rows = table["configurations"]["rows"].as_array().unwrap();
    let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["M3", "M4", "M5", "M6"]);
    assert_eq!(rows[1]["designation"], "ISO 4762 M4");
    // Applying a generated row sets the key, and the lookup follows.
    command(
        &mut doc,
        json!({"cmd": "apply_configuration", "name": "M6"}),
    );
    assert_eq!(text(&doc, "HeadDia"), "10 mm");
    // Undo takes back one generation at a time.
    doc.undo().unwrap();
    doc.undo().unwrap();
    let table = query(&doc, json!({"query": "configurations"}));
    assert_eq!(table["configurations"]["rows"].as_array().unwrap().len(), 3);
    // Refusals: a column of another kind, a parameter that does not exist.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Dia", "expression": "7 mm"}),
    );
    let e = refused(
        &mut doc,
        json!({"cmd": "generate_configuration_rows", "table": "Screws", "column": "Size",
               "option": "Dia"}),
    );
    assert!(e.contains("is a text column"), "{e}");
    let e = refused(
        &mut doc,
        json!({"cmd": "generate_configuration_rows", "table": "Screws", "column": "Size",
               "option": "Nope"}),
    );
    assert!(e.contains("Nope"), "{e}");
}

#[test]
fn a_csv_import_that_removes_the_current_value_lists_its_moves() {
    let mut doc = screws();
    command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Size": "M5"}}),
    );
    // M5 leaves the table: Size moves to the column's first value, and the
    // import's answer lists the move as set_table's does (mitcad#849).
    let csv = "Size [Text],dk [mm]\r\nM3,5.5\r\nM4,7\r\nM6,10\r\n";
    let answer = command(
        &mut doc,
        json!({"cmd": "import_table_csv", "name": "Screws", "csv": csv}),
    );
    assert_eq!(text(&doc, "Size"), "M3", "{answer}");
    assert_eq!(text(&doc, "HeadDia"), "5.5 mm");
    let moves = answer["moves"]
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"));
    assert_eq!(moves.len(), 1, "{answer}");
    assert_eq!(moves[0]["option"], "Size");
    assert_eq!(moves[0]["from"], "M5");
    assert_eq!(moves[0]["to"], "M3");
    assert_eq!(moves[0]["rule"], "its possible values Screws.Size");
    assert_eq!(doc.undo_label(), Some("Import Table Screws"));
    // The same file again changes nothing and moves nothing: the earlier
    // import's moves are not its own.
    let again = command(
        &mut doc,
        json!({"cmd": "import_table_csv", "name": "Screws", "csv": csv}),
    );
    assert_eq!(again["unchanged"], true, "{again}");
    assert_eq!(again["moves"], json!([]), "{again}");
    // An import that keeps the value moves nothing.
    let kept = "Size [Text],dk [mm]\r\nM3,5.6\r\nM4,7\r\n";
    let answer = command(
        &mut doc,
        json!({"cmd": "import_table_csv", "name": "Screws", "csv": kept}),
    );
    assert_eq!(text(&doc, "Size"), "M3");
    assert_eq!(text(&doc, "HeadDia"), "5.6 mm");
    assert_eq!(answer["moves"], json!([]), "{answer}");
    // The import and its move are one undo step.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(text(&doc, "Size"), "M5");
    assert_eq!(text(&doc, "HeadDia"), "8.5 mm");
}
