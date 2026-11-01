// SPDX-License-Identifier: MIT
//! Configurations (mitcad#797): the `values` limit, options, open and
//! closed tables, the migration of old tables, `configure` and its query
//! (mitcad#798); rules, the cascade and its enforcement (mitcad#799).

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

fn add(doc: &mut Document<MockKernel>, name: &str, expression: &str) {
    command(
        doc,
        json!({"cmd": "add_parameter", "name": name, "expression": expression}),
    );
}

fn reopened(doc: &Document<MockKernel>) -> Document<MockKernel> {
    Document::from_json(&doc.to_json(), MockKernel::default()).unwrap()
}

// mitcad#798: allowed values.

#[test]
fn a_number_snaps_to_its_allowed_values_and_a_strict_one_refuses() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "520 mm",
               "values": ["300 mm", "500 mm", 800]}),
    );
    let length = parameter(&doc, "Length");
    assert_eq!(length["text"], "500 mm", "{length}");
    assert_eq!(length["limited"], "values");
    assert_eq!(
        length["limits"]["values"],
        json!(["300 mm", "500 mm", "800"])
    );
    assert_eq!(
        length["limits"]["values_text"],
        json!(["300 mm", "500 mm", "800 mm"])
    );
    assert_eq!(
        length["limits"]["values_value"],
        json!([300.0, 500.0, 800.0])
    );
    assert!(
        length["limit_message"]
            .as_str()
            .unwrap()
            .contains("allowed value 500 mm"),
        "{length}"
    );
    // A listed value is not limited.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "800 mm"}),
    );
    assert_eq!(text(&doc, "Length"), "800 mm");
    assert!(parameter(&doc, "Length")["limited"].is_null());
    // Strict: refused, nothing changes.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "strict": true}),
    );
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "520 mm"}),
    );
    assert!(
        e.contains("520 mm is not one of its allowed values 300 mm, 500 mm, 800 mm"),
        "{e}"
    );
    assert_eq!(text(&doc, "Length"), "800 mm");
    // Removed with null.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "values": null, "strict": false}),
    );
    assert!(parameter(&doc, "Length").get("limits").is_none());
}

#[test]
fn a_text_outside_its_allowed_values_is_refused() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "expression": "'Black'",
               "values": ["'Black'", "'White'"]}),
    );
    assert_eq!(text(&doc, "Color"), "Black");
    assert_eq!(
        parameter(&doc, "Color")["limits"]["values_value"],
        json!(["Black", "White"])
    );
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Color", "expression": "'Green'"}),
    );
    assert!(
        e.contains("'Green' is not one of its allowed values 'Black', 'White'"),
        "{e}"
    );
    assert_eq!(text(&doc, "Color"), "Black");
    // Not strict either: a text has no nearest value.
    let e = refused(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Finish", "expression": "'Matte'",
               "values": ["'Gloss'"]}),
    );
    assert!(e.contains("is not one of its allowed values"), "{e}");
    // A text still has no minimum.
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Color", "min": "1"}),
    );
    assert!(e.contains("a text parameter has no minimum"), "{e}");
}

#[test]
fn allowed_values_are_saved_undone_renamed_and_go_through_the_csv() {
    let mut doc = doc();
    add(&mut doc, "Short", "300 mm");
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "500 mm",
               "values": ["Short", "500 mm"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "expression": "'Black'",
               "values": ["'Black'", "'White'"]}),
    );
    // Save and load.
    let again = reopened(&doc);
    assert_eq!(
        parameter(&again, "Length")["limits"]["values"],
        json!(["Short", "500 mm"])
    );
    assert_eq!(
        parameter(&again, "Color")["limits"]["values"],
        json!(["'Black'", "'White'"])
    );
    // Undo.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "values": ["500 mm", "900 mm"]}),
    );
    assert_eq!(
        parameter(&doc, "Length")["limits"]["values"],
        json!(["500 mm", "900 mm"])
    );
    doc.undo().unwrap();
    assert_eq!(
        parameter(&doc, "Length")["limits"]["values"],
        json!(["Short", "500 mm"])
    );
    // A rename rewrites them; the parameter a value uses is a dependency.
    assert_eq!(parameter(&doc, "Length")["dependencies"], json!(["Short"]));
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Short", "new_name": "Shortest"}),
    );
    assert_eq!(
        parameter(&doc, "Length")["limits"]["values"],
        json!(["Shortest", "500 mm"])
    );
    // The CSV: exported and imported into a new design.
    let csv = query(&doc, json!({"query": "parameters_csv"}))["csv"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        csv.contains(",\"[\"\"Shortest\"\",\"\"500 mm\"\"]\"\r\n"),
        "{csv}"
    );
    let mut copy = Document::new(MockKernel::default());
    command(
        &mut copy,
        json!({"cmd": "import_parameters_csv", "csv": csv}),
    );
    assert_eq!(
        parameter(&copy, "Length")["limits"]["values"],
        json!(["Shortest", "500 mm"])
    );
    assert_eq!(
        parameter(&copy, "Color")["limits"]["values"],
        json!(["'Black'", "'White'"])
    );
}

// mitcad#798: options, open and closed tables.

/// A shelf: Color (text, three values), Tables and Power (0 or 1), Length
/// (300 mm to 2000 mm in steps of 10 mm) and Depth, driven by Length.
fn shelf() -> Document<MockKernel> {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "expression": "'White'",
               "values": ["'Black'", "'White'", "'Red'"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Tables", "expression": "1",
               "values": ["0", "1"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Power", "expression": "1",
               "values": ["0", "1"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "1500 mm",
               "min": "300 mm", "max": "2000 mm", "step": "10 mm"}),
    );
    add(&mut doc, "Depth", "Length / 4");
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color", "Tables", "Power", "Length", "Depth"],
            "default": "Standard",
            "rows": [{"name": "Standard", "values": {"Color": "'White'", "Tables": "1",
                      "Power": "1", "Length": "1200 mm"}, "designation": "Shelf standard"}]}}),
    );
    doc
}

#[test]
fn the_configure_query_gives_lists_ranges_and_driven_options() {
    let doc = shelf();
    let answer = query(&doc, json!({"query": "configure"}));
    let options = answer["options"].as_array().unwrap();
    let option = |name: &str| options.iter().find(|o| o["name"] == name).unwrap().clone();
    // A list.
    let color = option("Color");
    assert_eq!(color["kind"], "text");
    assert_eq!(color["value"], "White");
    assert_eq!(color["domain"]["kind"], "list");
    let listed: Vec<&str> = color["allowed"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["text"].as_str().unwrap())
        .collect();
    assert_eq!(listed, ["Black", "White", "Red"]);
    assert_eq!(color["allowed"]["values"][0]["expression"], "'Black'");
    assert_eq!(color["driven"], false);
    // A range.
    let length = option("Length");
    assert_eq!(length["kind"], "number");
    assert_eq!(length["allowed"]["kind"], "range");
    assert_eq!(length["allowed"]["min"], 300.0);
    assert_eq!(length["allowed"]["max"], 2000.0);
    assert_eq!(length["allowed"]["step"], 10.0);
    assert_eq!(length["allowed"]["max_text"], "2000 mm");
    assert_eq!(length["text"], "1500 mm");
    // A driven option.
    let depth = option("Depth");
    assert_eq!(depth["driven"], true);
    assert_eq!(depth["expression"], "Length / 4");
    assert_eq!(depth["allowed"]["kind"], "free");
    // 1500 mm is no row of the open table: no row.
    assert!(answer["row"].is_null());
    assert_eq!(answer["designation_values"]["Length"], "1500");
    assert_eq!(answer["designation_values"]["Color"], "White");
    assert_eq!(answer["problems"], json!([]));
    // Tried values change nothing.
    let tried = query(
        &doc,
        json!({"query": "configure", "values": {"Length": "1200 mm"}}),
    );
    assert_eq!(tried["row"], "Standard");
    assert_eq!(text(&doc, "Length"), "1500 mm");
}

#[test]
fn configure_writes_constants_in_one_undo_step() {
    let mut doc = shelf();
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Color": "Black", "Length": 800}}),
    );
    assert_eq!(answer["refused"], Value::Null);
    assert_eq!(parameter(&doc, "Color")["expression"], "'Black'");
    assert_eq!(text(&doc, "Length"), "800 mm");
    assert_eq!(text(&doc, "Depth"), "200 mm");
    assert_eq!(
        doc.undo_label(),
        Some("Configure Color = Black, Length = 800 mm")
    );
    doc.undo().unwrap();
    assert_eq!(text(&doc, "Color"), "White");
    assert_eq!(text(&doc, "Length"), "1500 mm");
    // The row's values make the row.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Length": "1200 mm"}}),
    );
    assert_eq!(answer["row"], "Standard");
    // A driven option, a parameter that is no option and a formula are
    // refused.
    let e = refused(
        &mut doc,
        json!({"cmd": "configure", "values": {"Depth": "100 mm"}}),
    );
    assert!(e.contains("option Depth is driven"), "{e}");
    add(&mut doc, "Wall", "4 mm");
    let e = refused(
        &mut doc,
        json!({"cmd": "configure", "values": {"Wall": "5 mm"}}),
    );
    assert!(e.contains("Wall is not an option"), "{e}");
    let e = refused(
        &mut doc,
        json!({"cmd": "configure", "values": {"Length": "Wall * 100"}}),
    );
    assert!(e.contains("is not a constant"), "{e}");
}

/// A screw of a closed table: Size and Length are text options; the rows
/// also set d and L.
fn screw(closed: bool) -> Document<MockKernel> {
    let mut doc = doc();
    add(&mut doc, "Size", "'M4'");
    add(&mut doc, "Length", "'10'");
    add(&mut doc, "d", "4 mm");
    add(&mut doc, "L", "10 mm");
    let rows = json!([
        {"name": "M4x10", "values": {"Size": "'M4'", "Length": "'10'", "d": "4 mm", "L": "10 mm"}},
        {"name": "M4x16", "values": {"Size": "'M4'", "Length": "'16'", "d": "4 mm", "L": "16 mm"},
         "part_number": "S-416"},
        {"name": "M6x16", "values": {"Size": "'M6'", "Length": "'16'", "d": "6 mm", "L": "16 mm"}}
    ]);
    let rows = if closed {
        rows
    } else {
        // An open table's rows set options only.
        let mut rows = rows;
        for row in rows.as_array_mut().unwrap() {
            let values = row["values"].as_object_mut().unwrap();
            values.remove("d");
            values.remove("L");
        }
        rows
    };
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Size", "Length"], "closed": closed,
            "parameters": if closed { json!(["d", "L"]) } else { json!([]) },
            "default": "M4x10", "rows": rows}}),
    );
    doc
}

#[test]
fn a_closed_table_refuses_a_combination_that_is_no_row() {
    let mut doc = screw(true);
    // M4 and 99: no row.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Size": "M4", "Length": "99"}}),
    );
    assert_eq!(
        answer["refused"]["rule"],
        "the configuration table is closed"
    );
    assert!(
        answer["refused"]["text"]
            .as_str()
            .unwrap()
            .contains("Size M4 and Length 99 is no row"),
        "{answer}"
    );
    assert_eq!(text(&doc, "Length"), "10");
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "'99'"}),
    );
    assert!(e.contains("is no row"), "{e}");
    // Size M6 alone: Length follows the rows of M6.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Size": "M6"}}),
    );
    assert_eq!(answer["refused"], Value::Null, "{answer}");
    assert_eq!(text(&doc, "Length"), "16");
    assert_eq!(answer["moves"][0]["option"], "Length");
    assert_eq!(
        answer["moves"][0]["rule"],
        "the configuration table is closed"
    );
    assert_eq!(answer["row"], "M6x16");
    // The query's choosers: Length of M6 is 16 only.
    let report = query(&doc, json!({"query": "configure"}));
    assert_eq!(
        report["options"][1]["allowed"]["values"],
        json!([{"value": "16", "text": "16", "expression": "'16'"}])
    );
}

#[test]
fn an_open_table_allows_combinations_that_are_no_row() {
    let mut doc = screw(false);
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Size": "M6", "Length": "10"}}),
    );
    assert_eq!(answer["refused"], Value::Null);
    assert!(answer["row"].is_null());
    let report = query(&doc, json!({"query": "configure"}));
    assert!(report["row"].is_null());
    assert_eq!(
        report["designation_values"],
        json!({"Size": "M6", "Length": "10"})
    );
    // Rows of an open table set options only.
    let e = refused(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Size"], "parameters": ["d"],
            "rows": [{"name": "A", "values": {"Size": "'M4'"}}], "closed": false}}),
    );
    assert!(
        e.contains("the rows of an open table set options only"),
        "{e}"
    );
}

/// A design file whose table has the form before mitcad#798, as text.
fn old_file() -> String {
    let mut doc = doc();
    add(&mut doc, "d", "4 mm");
    add(&mut doc, "L", "10 mm");
    // A parameter already named Length: the selector gets another name.
    add(&mut doc, "Length", "10 mm");
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    file["configurations"] = json!({
    "selectors": ["Size", "Length"], "parameters": ["d", "L"], "default": "M4x16",
    "rows": [
        {"name": "M4x10", "select": {"Size": "M4", "Length": "10"},
         "values": {"d": "4 mm", "L": "10 mm"}},
        {"name": "M4x16", "select": {"Size": "M4", "Length": "16"},
         "values": {"d": "4 mm", "L": "16 mm"}, "part_number": "S-416",
         "designation": "Screw M4x16"}
    ]});
    file.to_string()
}

#[test]
fn an_old_table_opens_closed_with_text_options_and_is_written_new() {
    let doc = Document::from_json(&old_file(), MockKernel::default()).unwrap();
    let table = doc.configurations();
    assert!(table.closed);
    assert_eq!(table.options, ["Size", "Length2"]);
    assert_eq!(table.parameters, ["d", "L"]);
    assert_eq!(table.default.as_deref(), Some("M4x16"));
    assert_eq!(table.rows.len(), 2);
    let row = table.row("M4x16").unwrap();
    assert_eq!(row.part_number.as_deref(), Some("S-416"));
    assert_eq!(row.designation.as_deref(), Some("Screw M4x16"));
    assert_eq!(row.values["Size"], "'M4'");
    assert_eq!(row.values["Length2"], "'16'");
    assert_eq!(row.values["L"], "16 mm");
    assert!(
        doc.load_warnings()
            .iter()
            .any(|w| w.contains("selector Length is the option Length2")),
        "{:?}",
        doc.load_warnings()
    );
    // The new options are text parameters with the values of the row the
    // parameters have (M4x10: d 4 mm, L 10 mm).
    assert_eq!(text(&doc, "Size"), "M4");
    assert_eq!(text(&doc, "Length2"), "10");
    assert_eq!(parameter(&doc, "Size")["unit"], "Text");
    // The query keeps the old names of the values.
    let answer = query(&doc, json!({"query": "configurations"}));
    assert_eq!(answer["current"], "M4x10");
    assert_eq!(
        answer["selector_values"],
        json!({"Size": ["M4"], "Length2": ["10", "16"]})
    );
    // Written in the new form.
    let text = doc.to_json();
    let written: Value = serde_json::from_str(&text).unwrap();
    let table = &written["configurations"];
    assert_eq!(table["options"], json!(["Size", "Length2"]));
    assert_eq!(table["closed"], true);
    assert!(table.get("selectors").is_none(), "{table}");
    assert!(table["rows"][0].get("select").is_none(), "{table}");
    // And opens the same again.
    let again = reopened(&doc);
    assert_eq!(again.configurations(), doc.configurations());
    assert!(again.load_warnings().is_empty());
}

// mitcad#799: rules, the cascade and its enforcement.

/// The shelf with the design note's rules: Black is short, Power needs
/// tables, and `extra` rules.
fn shelf_with(extra: Value) -> Document<MockKernel> {
    let mut doc = shelf();
    let mut rules = vec![
        json!({"name": "Black is short", "when": "Color = 'Black'",
               "then": [{"option": "Length", "max": "1000 mm"}]}),
        json!({"name": "Power needs tables", "when": "Tables == 0",
               "then": [{"option": "Power", "values": ["0"]}]}),
    ];
    rules.extend(extra.as_array().cloned().unwrap_or_default());
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color", "Tables", "Power", "Length", "Depth"],
            "default": "Standard",
            "rows": [{"name": "Standard", "values": {"Color": "'White'", "Tables": "1",
                      "Power": "1", "Length": "1200 mm"}}],
            "rules": rules}}),
    );
    doc
}

#[test]
fn the_design_notes_rules_move_the_later_options() {
    let mut doc = shelf_with(json!([]));
    assert_eq!(text(&doc, "Length"), "1500 mm");
    // Black is short: Length moves from 1500 mm to 1000 mm.
    let answer = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Color", "expression": "'Black'"}),
    );
    assert_eq!(text(&doc, "Length"), "1000 mm");
    assert_eq!(text(&doc, "Depth"), "250 mm");
    let moves = answer["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 1, "{answer}");
    assert_eq!(moves[0]["option"], "Length");
    assert_eq!(moves[0]["from"], "1500 mm");
    assert_eq!(moves[0]["to"], "1000 mm");
    assert_eq!(moves[0]["rule"], "Black is short");
    assert_eq!(moves[0]["text"], "Length 1500 mm → 1000 mm: Black is short");
    assert!(
        answer["changed"]
            .as_array()
            .unwrap()
            .contains(&json!("Length")),
        "{answer}"
    );
    // The change and the move are one undo step.
    assert_eq!(doc.undo_label(), Some("Change Color"));
    doc.undo().unwrap();
    assert_eq!(text(&doc, "Color"), "White");
    assert_eq!(text(&doc, "Length"), "1500 mm");
    doc.redo().unwrap();
    assert_eq!(text(&doc, "Length"), "1000 mm");
    // Power needs tables: Power is chosen by itself when Tables is 0.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Tables": 0}}),
    );
    assert_eq!(text(&doc, "Power"), "0");
    assert_eq!(answer["moves"][0]["option"], "Power");
    assert_eq!(answer["moves"][0]["rule"], "Power needs tables");
    // The chooser of Power offers 0 only, and says why.
    let report = query(&doc, json!({"query": "configure"}));
    let power = &report["options"][2];
    assert_eq!(power["name"], "Power");
    assert_eq!(
        power["allowed"]["values"],
        json!([{"value": 0.0, "text": "0", "expression": "0"}])
    );
    assert_eq!(power["restricted_by"], json!(["Power needs tables"]));
    // Length of a black shelf: at most 1000 mm.
    let length = &report["options"][3];
    assert_eq!(length["allowed"]["max"], 1000.0);
    assert_eq!(length["restricted_by"], json!(["Black is short"]));
    // apply_configuration answers its moves too: the row is White.
    let answer = command(
        &mut doc,
        json!({"cmd": "apply_configuration", "name": "Standard"}),
    );
    assert_eq!(answer["moves"], json!([]));
    assert_eq!(text(&doc, "Length"), "1200 mm");
}

#[test]
fn opposite_rules_make_no_cycle() {
    // Color restricts Length, and Length restricts Color.
    let mut doc = shelf_with(json!([
        {"name": "Long is light", "when": "Length > 1000 mm",
         "then": [{"option": "Color", "exclude": ["'Black'"]}]}
    ]));
    // Choosing Black moves Length, which then allows Black.
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Color": "Black"}}),
    );
    assert_eq!(answer["refused"], Value::Null, "{answer}");
    assert_eq!(text(&doc, "Color"), "Black");
    assert_eq!(text(&doc, "Length"), "1000 mm");
    // A long black shelf: Color comes before Length and does not move, so
    // the change is refused, and nothing changes.
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "1500 mm"}),
    );
    assert!(
        e.contains("the rule Long is light: does not allow Color = Black"),
        "{e}"
    );
    assert_eq!(text(&doc, "Length"), "1000 mm");
}

#[test]
fn an_empty_intersection_names_both_rules() {
    let mut doc = shelf_with(json!([
        {"name": "Long only", "when": "Color = 'Black'",
         "then": [{"option": "Length", "min": "1500 mm"}]}
    ]));
    let answer = command(
        &mut doc,
        json!({"cmd": "configure", "values": {"Color": "Black"}}),
    );
    let text_of = answer["refused"]["text"].as_str().unwrap();
    assert_eq!(
        text_of, "Black is short and Long only leave Length no value",
        "{answer}"
    );
    assert_eq!(text(&doc, "Color"), "White");
    // The query reports it for the values tried.
    let report = query(
        &doc,
        json!({"query": "configure", "values": {"Color": "Black"}}),
    );
    assert!(
        report["refused"]["text"]
            .as_str()
            .unwrap()
            .contains("Black is short and Long only"),
        "{report}"
    );
}

#[test]
fn broken_rules_refuse_set_parameter_driven_options_and_the_csv() {
    let mut doc = shelf_with(json!([
        {"name": "No red", "when": "Color = 'Red'", "then": [{"forbid": "Red is not made"}]},
        {"name": "Shallow", "then": [{"option": "Depth", "max": "300 mm"}]}
    ]));
    // Depth (Length / 4) is 375 mm already: broken before, not refused.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "1000 mm"}),
    );
    let label = doc.undo_label().map(str::to_owned);
    // A forbid refuses set_parameter, and nothing changes.
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Color", "expression": "'Red'"}),
    );
    assert!(e.contains("the rule No red: Red is not made"), "{e}");
    assert_eq!(text(&doc, "Color"), "White");
    assert_eq!(doc.undo_label().map(str::to_owned), label);
    // A driven option that breaks a rule is refused: Depth would be 350 mm.
    let e = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Length", "expression": "1400 mm"}),
    );
    assert!(
        e.contains("the rule Shallow: does not allow Depth = 350 mm"),
        "{e}"
    );
    assert_eq!(text(&doc, "Length"), "1000 mm");
    // A parameters CSV import that breaks a rule is refused.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Color", "expression": "'Black'"}),
    );
    let e = refused(
        &mut doc,
        json!({"cmd": "import_parameters_csv", "csv": "name,expression\nLength,1500 mm\n"}),
    );
    assert!(e.contains("Black is short"), "{e}");
    assert_eq!(text(&doc, "Length"), "1000 mm");
}

#[test]
fn renames_follow_rules_and_used_parameters_are_not_deleted() {
    let mut doc = shelf();
    add(&mut doc, "Wall", "4 mm");
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Color", "Length"],
            "rules": [{"name": "Thick walls", "when": "Wall > 5 mm",
                       "then": [{"option": "Length", "min": "500 mm"}]},
                      {"name": "Black is short", "when": "Color = 'Black'",
                       "then": [{"option": "Length", "max": "1000 mm"}]}]}}),
    );
    assert_eq!(parameter(&doc, "Wall")["rules"], json!(["Thick walls"]));
    assert_eq!(parameter(&doc, "Color")["option"], true);
    assert_eq!(
        parameter(&doc, "Length")["rules"],
        json!(["Thick walls", "Black is short"])
    );
    // A rename rewrites the conditions and the options.
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Wall", "new_name": "Side"}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Color", "new_name": "Finish"}),
    );
    let table = query(&doc, json!({"query": "configurations"}))["configurations"].clone();
    assert_eq!(table["options"], json!(["Finish", "Length"]));
    assert_eq!(table["rules"][0]["when"], "Side > 5 mm");
    assert_eq!(table["rules"][1]["when"], "Finish = 'Black'");
    // Neither a rule's parameter nor an option is deleted.
    let e = refused(&mut doc, json!({"cmd": "delete_parameter", "name": "Side"}));
    assert!(e.contains("is used by the rule Thick walls"), "{e}");
    let e = refused(
        &mut doc,
        json!({"cmd": "delete_parameter", "name": "Finish"}),
    );
    // An option a rule uses: the rule's refusal comes first.
    assert!(e.contains("is used by the rule Black is short"), "{e}");
    // The rules still work under the new names.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Finish", "expression": "'Black'"}),
    );
    assert_eq!(text(&doc, "Length"), "1000 mm");
}

#[test]
fn check_rules_reports_values_never_allowed_and_rules_that_never_apply() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Color", "expression": "'White'",
               "values": ["'Black'", "'White'", "'Red'"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Size", "expression": "'S'",
               "values": ["'S'", "'L'"]}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
        "options": ["Color", "Size"],
        "rules": [
            {"name": "No red", "then": [{"option": "Color", "exclude": ["'Red'"]}]},
            {"name": "Never", "when": "Size = 'XL'", "then": [{"forbid": "no XL"}]},
            {"name": "Black is small", "when": "Color = 'Black'",
             "then": [{"option": "Size", "values": ["'S'"]}]}
        ]}}),
    );
    let report = query(&doc, json!({"query": "check_rules"}));
    assert_eq!(report["combinations"], 6);
    assert_eq!(report["checked"], 6);
    assert_eq!(report["sampled"], false);
    assert_eq!(
        report["never_allowed"],
        json!([{"option": "Color", "text": "Red"}])
    );
    assert_eq!(report["never_applies"], json!(["Never"]));
    assert_eq!(report["unrepairable"], json!([]));
    assert!(
        report["lines"]
            .as_array()
            .unwrap()
            .contains(&json!("the rule Never never applies")),
        "{report}"
    );
    // A smaller limit samples.
    let report = query(&doc, json!({"query": "check_rules", "limit": 4}));
    assert_eq!(report["sampled"], true);
    assert_eq!(report["checked"], 4);
}

#[test]
fn rule_problems_name_their_rule_and_place() {
    // mitcad#1064: the rule editor marks the block that is wrong.
    let mut doc = shelf();
    let rules = json!([
        {"name": "Black is short", "when": "Color = 'Black'",
         "then": [{"option": "Length", "max": "1000 mm"}]},
        {"name": "Wrong", "when": "Colour = 'Black'",
         "then": [{"option": "Size", "values": ["'S'"]},
                  {"option": "Length", "values": ["300 mm", "Nope"]}],
         "else": [{"option": "Color", "values": ["'Black'", "'Green'"]},
                  {"option": "Length", "step": "-10 mm"},
                  {"option": "Length", "min": "2500 mm"}]},
        {"name": "", "then": [{"forbid": "never"}]}
    ]);
    // Rules not set yet are checked as set_configurations would check
    // them, and nothing changes.
    let answer = query(
        &doc,
        json!({"query": "configurations", "rules": rules.clone()}),
    );
    let problems = answer["rule_problems"].as_array().unwrap();
    let places: Vec<(u64, Value, &str)> = problems
        .iter()
        .map(|p| {
            (
                p["index"].as_u64().unwrap(),
                p["place"].clone(),
                p["level"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        places,
        [
            (1, json!("when"), "error"),
            (1, json!("then.0.option"), "error"),
            (1, json!("then.1.values.1"), "error"),
            (1, json!("else.0.values.1"), "warning"),
            (1, json!("else.1.step"), "error"),
            (1, json!("else.2.min"), "warning"),
            (2, Value::Null, "error"),
        ],
        "{answer}"
    );
    assert_eq!(problems[0]["rule"], "Wrong");
    assert_eq!(
        problems[1]["message"],
        "the rule Wrong: Size is not an option of the configurations"
    );
    assert_eq!(
        problems[3]["message"],
        "the rule Wrong: values: Green is not one of the values of Color"
    );
    assert_eq!(
        problems[4]["message"],
        "the rule Wrong: the step must be positive"
    );
    assert_eq!(
        problems[5]["message"],
        "the rule Wrong: min: 2500 mm is above the largest value of Length, 2000 mm"
    );
    assert_eq!(problems[6]["message"], "rule 3 has no name");
    // The errors are the table's problems, in their words; the warnings
    // are none of them.
    let errors: Vec<Value> = problems
        .iter()
        .filter(|p| p["level"] == "error")
        .map(|p| p["message"].clone())
        .collect();
    assert_eq!(answer["problems"], Value::Array(errors.clone()));
    let own = query(&doc, json!({"query": "configurations"}));
    assert!(
        own["configurations"]["rules"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "{own}"
    );
    assert_eq!(own["rule_problems"], json!([]));
    assert_eq!(own["problems"], json!([]));
    // set_configurations refuses them with the first.
    let mut table = own["configurations"].clone();
    table["rules"] = rules;
    let e = refused(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": table}),
    );
    assert!(e.contains(errors[0].as_str().unwrap()), "{e}");
    // A value outside its option's domain alone refuses nothing.
    table["rules"] = json!([{"name": "No green",
                             "then": [{"option": "Color", "exclude": ["'Green'"]}]}]);
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": table}),
    );
    let answer = query(&doc, json!({"query": "configurations"}));
    assert_eq!(answer["problems"], json!([]));
    assert_eq!(
        answer["rule_problems"],
        json!([{"index": 0, "rule": "No green", "place": "then.0.exclude.0",
                "level": "warning",
                "message": "the rule No green: exclude: Green is not one of the values of Color"}])
    );
}
