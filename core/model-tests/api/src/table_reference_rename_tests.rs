// SPDX-License-Identifier: MIT
//! Table renames preserve bindings and every configuration rule expression.

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const SIDE: &str = "F2:side(c1[c4,c2])";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(
        &doc.command(&command.to_string())
            .unwrap_or_else(|error| panic!("{command}: {error}")),
    )
    .unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn bound_body(doc: &Document<MockKernel>) -> Value {
    let bodies = query(doc, json!({"query": "bodies"}));
    bodies
        .as_array()
        .unwrap()
        .iter()
        .find(|body| body["uid"] == "F2.b0")
        .unwrap_or_else(|| panic!("no bound body F2.b0: {bodies}"))
        .clone()
}

fn table(name: &str, column: &str, value: i32) -> Value {
    json!({"cmd": "set_table", "name": name, "table": {
        "columns": [{"name": column, "unit": ""}], "rows": [[value]]}})
}

fn rename(column: bool) -> Value {
    if column {
        json!({"cmd": "rename_table_column", "table": "Flags", "name": "Show", "new_name": "Visible"})
    } else {
        json!({"cmd": "rename_table", "name": "Flags", "new_name": "Controls"})
    }
}

fn renamed_names(column: bool) -> (&'static str, &'static str) {
    if column {
        ("Flags", "Visible")
    } else {
        ("Controls", "Show")
    }
}

fn lookup(table: &str, column: &str) -> String {
    format!("INDEX({table}.{column}; 1)")
}

fn bound_component() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let value = lookup("Flags", "Show");
    command(
        &mut doc,
        json!([
            {"cmd": "create_component", "name": "Block", "activate": true},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude",
                "profiles": [{"sketch": "F1", "region": REGION}],
                "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
            table("Flags", "Show", 1),
            {"cmd": "add_parameter", "name": "Watch", "expression": value},
            {"cmd": "set_body_visible", "uid": "F2.b0", "expression": value},
            {"cmd": "set_body_appearance", "uid": "F2.b0",
                "expression": format!("if({value}; 'paint_black'; 'paint_red')")},
            {"cmd": "set_body_material", "uid": "F2.b0",
                "expression": format!("if({value}; 'steel'; 'pla')")},
            {"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
                "expression": format!("if({value}; 'paint_white'; 'paint_yellow')")},
            {"cmd": "set_occurrence_visible", "occurrence": "O1", "expression": value},
            {"cmd": "suppress_feature", "uid": "F2", "expression": format!("{value} < 0")},
        ]),
    );
    doc
}

fn assert_bindings(doc: &Document<MockKernel>, table: &str, column: &str) {
    let value = lookup(table, column);
    let body = bound_body(doc);
    assert_eq!(body["visible_when"], value);
    assert_eq!(
        body["appearance_expr"],
        format!("if({value}; 'paint_black'; 'paint_red')")
    );
    assert_eq!(
        body["material_expr"],
        format!("if({value}; 'steel'; 'pla')")
    );
    assert_eq!(
        body["face_appearance_exprs"][SIDE],
        format!("if({value}; 'paint_white'; 'paint_yellow')")
    );
    assert!(body.get("warnings").is_none(), "{body}");
    let occurrence = &query(doc, json!({"query": "components"}))["occurrences"][0];
    assert_eq!(occurrence["visible_when"], value);
    let feature = query(doc, json!({"query": "feature", "uid": "F2"}));
    assert_eq!(feature["suppressed_when"], format!("{value} < 0"));
    let parameters = query(doc, json!({"query": "parameters"}));
    let watch = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Watch")
        .unwrap();
    assert_eq!(watch["expression"], value);
    assert_eq!(watch["value"], 1.0);
}

fn bindings_follow_rename(column: bool) {
    let mut doc = bound_component();
    let before = doc.to_json();
    command(&mut doc, rename(column));
    let (name, column) = renamed_names(column);
    assert_bindings(&doc, name, column);
    let after = doc.to_json();
    doc.undo().unwrap();
    assert_eq!(doc.to_json(), before);
    assert_bindings(&doc, "Flags", "Show");
    doc.redo().unwrap();
    assert_eq!(doc.to_json(), after);

    let mut loaded = Document::from_json(&after, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_bindings(&loaded, name, column);
    command(&mut loaded, table(name, column, 0));
    let body = bound_body(&loaded);
    assert_eq!(body["visible"], false);
    assert_eq!(body["appearance"], "paint_red");
    assert_eq!(body["material"], "pla");
    assert_eq!(body["face_appearances"][0]["appearance"], "paint_yellow");
    assert!(body.get("warnings").is_none(), "{body}");
    assert_eq!(
        query(&loaded, json!({"query": "components"}))["occurrences"][0]["visible"],
        false
    );
    command(&mut loaded, table(name, column, -1));
    assert_eq!(
        query(&loaded, json!({"query": "feature", "uid": "F2"}))["suppressed"],
        true
    );
}

#[test]
fn table_rename_preserves_all_binding_consumers() {
    bindings_follow_rename(false);
}

#[test]
fn column_rename_preserves_all_binding_consumers() {
    bindings_follow_rename(true);
}

fn restriction(value: &str) -> Value {
    json!({"option": "Choice", "values": [value, "2"],
        "exclude": [format!("{value} + 3")], "min": value,
        "max": format!("{value} + 1"), "step": format!("{value} - {value} + 1")})
}

fn rules(value: &str) -> Value {
    json!([{"name": "Enabled", "when": format!("{value} = 1"),
        "then": [restriction(value), {"require": format!("{value} >= 1"), "message": "Flags.Show is enabled"}],
        "else": [restriction(value), {"require": format!("{value} >= 1"), "message": "Flags.Show is enabled"}]}])
}

fn rules_follow_rename(column: bool) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            table("Flags", "Show", 1),
            {"cmd": "add_parameter", "name": "Choice", "expression": "1", "unit": "", "values": ["1", "2", "3", "4"]},
            {"cmd": "set_configurations", "configurations": {"options": ["Choice"],
                "rules": rules(&lookup("Flags", "Show"))}},
        ]),
    );
    let before = doc.to_json();
    command(&mut doc, rename(column));
    let (name, column) = renamed_names(column);
    let expected = rules(&lookup(name, column));
    assert_eq!(
        query(&doc, json!({"query": "configurations"}))["configurations"]["rules"],
        expected
    );
    let after = doc.to_json();
    doc.undo().unwrap();
    assert_eq!(doc.to_json(), before);
    doc.redo().unwrap();
    assert_eq!(doc.to_json(), after);
    let mut loaded = Document::from_json(&after, MockKernel::default()).unwrap();
    assert_eq!(
        query(&loaded, json!({"query": "configurations"}))["configurations"]["rules"],
        expected
    );
    command(
        &mut loaded,
        json!({"cmd": "set_parameter", "name": "Choice", "expression": "2"}),
    );
    command(&mut loaded, table(name, column, 2));
    let error = loaded
        .command(&json!({"cmd": "set_parameter", "name": "Choice", "expression": "1"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("Enabled"), "{error}");
    assert!(
        !error.contains("unknown table") && !error.contains("has no column"),
        "{error}"
    );
}

#[test]
fn table_rename_preserves_rule_conditions_and_action_formulas() {
    rules_follow_rename(false);
}

#[test]
fn column_rename_preserves_rule_conditions_and_action_formulas() {
    rules_follow_rename(true);
}
