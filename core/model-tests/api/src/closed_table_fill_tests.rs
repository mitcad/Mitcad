// SPDX-License-Identifier: MIT
//! Filling a closed table (mitcad#1044, decision 12 of
//! `docs/configurations.md`): rows generated from a table with its other
//! columns as parameter columns, the rows as CSV (export, preview, import)
//! and an option added to a closed table with rows.

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

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(result) => panic!("{query} was answered: {result}"),
        Err(error) => error.to_string(),
    }
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Option<Value> {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
}

fn expression(doc: &Document<MockKernel>, name: &str) -> String {
    parameter(doc, name).unwrap_or_else(|| panic!("no parameter {name}"))["expression"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn table(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "configurations"}))["configurations"].clone()
}

fn row<'a>(table: &'a Value, name: &str) -> &'a Value {
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no row {name} in {table}"))
}

/// A table of screw sizes: size, d and k (M5 without k).
fn screws(doc: &mut Document<MockKernel>, m3_d: f64) {
    command(
        doc,
        json!({"cmd": "set_table", "name": "Screws", "table": {
            "columns": [{"name": "size", "unit": "Text"}, {"name": "d", "unit": "mm"},
                        {"name": "k", "unit": "mm"}],
            "rows": [["M3", m3_d, 1.86], ["M4", 4, 2.48], ["M5", 5, null]]}}),
    );
}

#[test]
fn generated_rows_take_the_tables_other_columns_as_parameter_columns() {
    let mut doc = doc();
    screws(&mut doc, 3.0);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "size", "unit": "Text", "expression": "'M4'"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "d", "expression": "6 mm"}),
    );
    let generate = json!({"cmd": "generate_configuration_rows", "table": "Screws",
                          "column": "size", "option": "size", "columns": ["d", "k"]});
    let answer = command(&mut doc, generate.clone());
    assert_eq!(answer["rows"], json!(["M3", "M4", "M5"]), "{answer}");
    assert_eq!(answer["added"], json!(["M3", "M4", "M5"]));
    assert_eq!(answer["created"], json!(["k"]));
    assert_eq!(doc.undo_label(), Some("Generate Rows from Screws"));
    // k is made in the column's unit, at the value of the current row (M4).
    assert_eq!(expression(&doc, "k"), "2.48 mm");
    let t = table(&doc);
    assert_eq!(t["closed"], true, "{t}");
    assert_eq!(t["options"], json!(["size"]));
    assert_eq!(t["parameters"], json!(["d", "k"]));
    assert_eq!(
        row(&t, "M4")["values"],
        json!({"size": "'M4'", "d": "4 mm", "k": "2.48 mm"})
    );
    assert_eq!(
        row(&t, "M5")["values"],
        json!({"size": "'M5'", "d": "5 mm"})
    );

    // Choosing a size sets the row's parameter columns.
    command(
        &mut doc,
        json!({"cmd": "configure", "values": {"size": "M3"}}),
    );
    assert_eq!(expression(&doc, "d"), "3 mm");
    assert_eq!(expression(&doc, "k"), "1.86 mm");

    // The table changes: generating again updates the row of the same name.
    screws(&mut doc, 3.1);
    let answer = command(&mut doc, generate.clone());
    assert_eq!(answer["added"], json!([]), "{answer}");
    assert_eq!(answer["updated"], json!(["M3"]));
    assert_eq!(answer["created"], json!([]));
    assert_eq!(row(&table(&doc), "M3")["values"]["d"], "3.1 mm");

    // Undo takes a generation back with the parameters it made.
    let mut fresh = self::doc();
    screws(&mut fresh, 3.0);
    command(
        &mut fresh,
        json!({"cmd": "add_parameter", "name": "size", "unit": "Text", "expression": "'M4'"}),
    );
    command(&mut fresh, generate);
    assert!(parameter(&fresh, "k").is_some());
    assert!(parameter(&fresh, "d").is_some());
    fresh.undo().unwrap();
    assert!(parameter(&fresh, "k").is_none());
    assert!(parameter(&fresh, "d").is_none());
    assert_eq!(table(&fresh), Value::Null);

    // A column of the wrong kind for its parameter is refused.
    command(
        &mut fresh,
        json!({"cmd": "add_parameter", "name": "k", "unit": "Text", "expression": "'x'"}),
    );
    let e = refused(
        &mut fresh,
        json!({"cmd": "generate_configuration_rows", "table": "Screws",
               "column": "size", "option": "size", "columns": ["k"]}),
    );
    assert!(
        e.contains("Screws.k is a number column, and parameter k is a text"),
        "{e}"
    );
}

#[test]
fn a_parameter_column_chosen_as_the_option_moves_to_the_options() {
    let mut doc = doc();
    screws(&mut doc, 3.0);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "size", "unit": "Text", "expression": "'M8'"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'Black'"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color"], "closed": true, "parameters": ["size"],
            "rows": [{"name": "Big", "values": {"Color": "'Black'", "size": "'M8'"}}]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "generate_configuration_rows", "table": "Screws",
               "column": "size", "option": "size"}),
    );
    let t = table(&doc);
    assert_eq!(t["options"], json!(["Color", "size"]), "{t}");
    assert_eq!(t["parameters"], json!([]));
    assert_eq!(t["closed"], true);
    // The kept row keeps its value, an option's now; the new rows take the
    // other option's current value.
    assert_eq!(
        row(&t, "Big")["values"],
        json!({"Color": "'Black'", "size": "'M8'"})
    );
    assert_eq!(
        row(&t, "M3")["values"],
        json!({"Color": "'Black'", "size": "'M3'"})
    );
    let problems = query(&doc, json!({"query": "configurations"}))["problems"].clone();
    assert_eq!(problems, json!([]));
}

#[test]
fn rows_go_through_csv_with_a_preview_and_refusals() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "k", "expression": "2 mm"}),
    );
    let csv = "size,d [mm],k [mm],Designation\r\n\
               M3,3,\"1,86\",ISO 7380 M3\r\n\
               M4,4,2.48,\r\n\
               M5,5,3.1,\r\n";
    let undo_before = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    let preview = query(
        &doc,
        json!({"query": "configuration_rows_csv_preview", "csv": csv}),
    );
    assert_eq!(preview["key"], "size", "{preview}");
    let roles: Vec<&str> = preview["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["key", "parameter", "parameter", "designation"]);
    assert_eq!(preview["columns"][0]["unit"], "Text");
    assert_eq!(preview["columns"][0]["inferred"], true);
    assert_eq!(preview["rows"][1], json!(["M4", 4.0, 2.48, null]));
    assert_eq!(preview["added"], json!(["M3", "M4", "M5"]));
    assert_eq!(preview["created"], json!(["size", "d"]));
    assert_eq!(preview["closed"], true);
    assert_eq!(preview["unchanged"], false);
    assert!(preview.get("problem").is_none(), "{preview}");
    assert_eq!(
        preview["summary"],
        "3 rows from the file: added M3, M4, M5; made the parameters size, d"
    );
    // The preview changed nothing.
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo_depth"],
        undo_before
    );
    assert!(parameter(&doc, "size").is_none());

    let done = command(
        &mut doc,
        json!({"cmd": "import_configuration_rows_csv", "csv": csv}),
    );
    assert_eq!(done["summary"], preview["summary"]);
    assert_eq!(done["moves"], json!([]));
    assert_eq!(doc.undo_label(), Some("Import Configuration Rows"));
    let t = table(&doc);
    assert_eq!(t["options"], json!(["size"]), "{t}");
    assert_eq!(t["parameters"], json!(["d", "k"]));
    assert_eq!(t["closed"], true);
    assert_eq!(
        row(&t, "M3")["values"],
        json!({"size": "'M3'", "d": "3 mm", "k": "1.86 mm"})
    );
    assert_eq!(row(&t, "M3")["designation"], "ISO 7380 M3");
    assert!(row(&t, "M4").get("designation").is_none());
    assert_eq!(expression(&doc, "size"), "'M3'");
    assert_eq!(expression(&doc, "d"), "3 mm");
    // The closed table's combination is the row M3 now: the cascade after
    // the import sets its parameter columns.
    assert_eq!(expression(&doc, "k"), "1.86 mm");
    // A size chosen sets its row's columns.
    command(
        &mut doc,
        json!({"cmd": "configure", "values": {"size": "M5"}}),
    );
    assert_eq!(expression(&doc, "k"), "3.1 mm");

    // The export, and the same file again changes nothing.
    let exported = query(&doc, json!({"query": "configuration_rows_csv"}));
    assert_eq!(exported["rows"], 3);
    assert_eq!(
        exported["csv"],
        "size [Text],d [mm],k [mm],Name [Text],Designation [Text]\r\n\
         M3,3,1.86,M3,ISO 7380 M3\r\n\
         M4,4,2.48,M4,\r\n\
         M5,5,3.1,M5,\r\n"
    );
    let undo_label = doc.undo_label().map(str::to_owned);
    let again = command(
        &mut doc,
        json!({"cmd": "import_configuration_rows_csv", "csv": exported["csv"]}),
    );
    assert_eq!(again["unchanged"], true, "{again}");
    assert_eq!(again["moves"], json!([]));
    assert_eq!(doc.undo_label().map(str::to_owned), undo_label);

    // Rows of the same name are updated, others added, the rest kept; a
    // formula of a parameter column is kept as it is; Name names a row.
    let update = "Name,size [Text],k [mm]\r\nM4,M4,2.5\r\nLong M6,M6,d * 0.6\r\n";
    let done = command(
        &mut doc,
        json!({"cmd": "import_configuration_rows_csv", "csv": update}),
    );
    assert_eq!(done["updated"], json!(["M4"]), "{done}");
    assert_eq!(done["added"], json!(["Long M6"]));
    assert_eq!(done["kept"], json!(["M3", "M5"]));
    assert_eq!(done["columns"][0]["role"], "name");
    assert_eq!(done["rows"][1][2], "d * 0.6");
    let t = table(&doc);
    assert_eq!(row(&t, "M4")["values"]["k"], "2.5 mm");
    assert_eq!(row(&t, "M4")["values"]["d"], "4 mm");
    assert_eq!(
        row(&t, "Long M6")["values"],
        json!({"size": "'M6'", "k": "d * 0.6"})
    );
    // One undo step.
    doc.undo().unwrap();
    assert_eq!(row(&table(&doc), "M4")["values"]["k"], "2.48 mm");

    // Refusals: a cell of the wrong kind (the preview lists it), a row
    // twice, no column of values, a column of the wrong kind.
    let wrong = "size [Text],k [mm]\r\nM3,5 deg\r\n";
    let preview = query(
        &doc,
        json!({"query": "configuration_rows_csv_preview", "csv": wrong}),
    );
    assert_eq!(preview["refused"][0]["line"], 2, "{preview}");
    assert_eq!(preview["refused"][0]["column"], "k");
    let e = refused(
        &mut doc,
        json!({"cmd": "import_configuration_rows_csv", "csv": wrong}),
    );
    assert!(e.contains("nothing is imported"), "{e}");
    let e = query_error(
        &doc,
        json!({"query": "configuration_rows_csv_preview",
               "csv": "size [Text]\r\nM3\r\nM3\r\n"}),
    );
    assert!(e.contains("the row M3 is in the file twice"), "{e}");
    let e = query_error(
        &doc,
        json!({"query": "configuration_rows_csv_preview", "csv": "Name\r\nA\r\n"}),
    );
    assert!(e.contains("no column of values"), "{e}");
    let e = refused(
        &mut doc,
        json!({"cmd": "import_configuration_rows_csv",
               "csv": "size [Text],k [Text]\r\nM3,big\r\n"}),
    );
    assert!(
        e.contains("CSV.k is a text column, and parameter k is a number"),
        "{e}"
    );
}

#[test]
fn an_option_added_to_a_closed_table_with_rows_needs_a_value_in_every_row() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "import_configuration_rows_csv",
               "csv": "size [Text],d [mm],k [mm]\r\nM3,3,1.86\r\nM4,4,2.48\r\n"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'Black'"}),
    );
    let mut t = table(&doc);
    // Without the rows' values the closed table refuses the option.
    let mut bare = t.clone();
    bare["options"] = json!(["size", "Color"]);
    let e = refused(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": bare}),
    );
    assert!(e.contains("M3: no value for Color"), "{e}");
    // Each row with the parameter's current value (as Add Option… sends
    // it), and the parameter column d made an option with its rows'
    // values: one set_configurations.
    t["options"] = json!(["size", "Color", "d"]);
    t["parameters"] = json!(["k"]);
    for r in t["rows"].as_array_mut().unwrap() {
        r["values"]["Color"] = json!("'Black'");
    }
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": t}),
    );
    assert_eq!(doc.undo_label(), Some("Edit Configurations"));
    let t = table(&doc);
    assert_eq!(t["options"], json!(["size", "Color", "d"]), "{t}");
    assert_eq!(
        row(&t, "M4")["values"],
        json!({"size": "'M4'", "Color": "'Black'", "d": "4 mm", "k": "2.48 mm"})
    );
    let problems = query(&doc, json!({"query": "configurations"}))["problems"].clone();
    assert_eq!(problems, json!([]));
}
