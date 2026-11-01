// SPDX-License-Identifier: MIT
//! Tables in the design (mitcad#667, `tables.rs`, `document/tables.rs`)
//! with the mock kernel: lookups that drive features, edits that
//! re-evaluate only what they reach, refused edits, renames, deletes, undo,
//! the design file, copies into another design and configurations.

use std::collections::BTreeMap;

use serde_json::json;

use crate::Document;
use crate::configurations::{ConfigurationRow, Configurations};
use crate::expr::ParamLimits;
use crate::ids::ComponentUid;
use crate::tables::{Table, TableCell};
use crate::test_support::{Block, block};
use crate::testing::MockKernel;
use crate::transform::Transform;

fn screws() -> Table {
    serde_json::from_value(json!({
        "name": "Screws",
        "comment": "ISO 4762 socket head cap screws",
        "columns": [
            {"name": "Size", "unit": "Text"},
            {"name": "d", "unit": "mm"},
            {"name": "dk", "unit": "mm"},
            {"name": "k", "unit": "mm"}],
        "rows": [
            ["M3", 3, 5.5, 3],
            ["M4", 4, 7, 4],
            ["M5", 5, 8.5, 5]]
    }))
    .unwrap()
}

/// The block with the table Screws, the text parameter Size = 'M5' and
/// the extrusion's distance d3 looked up: the head diameter of M5.
fn block_with_screws() -> Block {
    let mut b = block();
    assert!(b.doc.set_table(screws()).unwrap().is_empty());
    b.doc
        .add_parameter_expression("Size", "'M5'", None, "")
        .unwrap();
    b.doc
        .set_parameter_expression("d3", "XLOOKUP(Size; Screws.Size; Screws.dk)", None)
        .unwrap();
    assert_eq!(b.history(b.body), "prism(F2:0..8.5)");
    b
}

fn value(doc: &Document<MockKernel>, name: &str) -> f64 {
    let params = doc.parameters();
    params.value(params.find(name).unwrap()).unwrap()
}

fn expression(doc: &Document<MockKernel>, name: &str) -> String {
    let params = doc.parameters();
    params
        .get(params.find(name).unwrap())
        .unwrap()
        .expression()
        .to_owned()
}

/// Screws with the cell of `row` (from 0) and `column` set.
fn screws_with(row: usize, column: usize, cell: TableCell) -> Table {
    let mut table = screws();
    table.rows[row][column] = cell;
    table
}

#[test]
fn a_table_edit_recomputes_only_the_features_whose_values_change() {
    let mut b = block_with_screws();
    let other = b.circle_extrude("new_body", &[]);
    assert_eq!(b.doc.table_users("Screws"), ["d3"]);
    // M5's head diameter changes: d3 follows, only Extrude1 recomputes.
    let changed = b
        .doc
        .set_table(screws_with(2, 2, TableCell::Number(9.0)))
        .unwrap();
    assert_eq!(changed, ["d3"]);
    assert_eq!(b.doc.undo_label(), Some("Edit Table Screws"));
    assert_eq!(b.evaluated(), vec![b.extrude]);
    assert!(!b.evaluated().contains(&other));
    assert_eq!(b.history(b.body), "prism(F2:0..9)");
    // M3's changes no used value: an undo step that recomputes nothing.
    let count = b.doc.recompute_count();
    let mut table = screws_with(2, 2, TableCell::Number(9.0));
    table.rows[0][2] = TableCell::Number(5.6);
    assert!(b.doc.set_table(table.clone()).unwrap().is_empty());
    assert_eq!(b.doc.recompute_count(), count);
    assert_eq!(b.doc.table("Screws"), Some(&table));
    // The same table again is no command at all.
    let depth = b.doc.undo_depth();
    assert!(b.doc.set_table(table).unwrap().is_empty());
    assert_eq!(b.doc.undo_depth(), depth);
}

#[test]
fn an_edit_that_would_make_a_parameter_fail_is_refused() {
    let mut b = block_with_screws();
    let before = b.doc.state().clone();
    // M5 gone: d3 finds no row.
    let mut table = screws();
    table.rows.pop();
    let error = b.doc.set_table(table).unwrap_err().to_string();
    assert!(error.contains("parameter d3"), "{error}");
    // Its head diameter empty.
    let error = b
        .doc
        .set_table(screws_with(2, 2, TableCell::Empty))
        .unwrap_err()
        .to_string();
    assert!(error.contains("parameter d3"), "{error}");
    // A column in use left out.
    let mut table = screws();
    table.columns.remove(2);
    for row in &mut table.rows {
        row.remove(2);
    }
    assert_eq!(
        b.doc.set_table(table).unwrap_err().to_string(),
        "column Screws.dk is used by d3"
    );
    // A cell of the wrong kind.
    let error = b
        .doc
        .set_table(screws_with(1, 2, TableCell::Text("7".to_owned())))
        .unwrap_err()
        .to_string();
    assert!(error.contains("row 2, column dk"), "{error}");
    // Nothing changed.
    assert!(b.doc.state() == &before);
    assert_eq!(b.history(b.body), "prism(F2:0..8.5)");
    // The key changed to one the table lacks is refused as well.
    assert!(
        b.doc
            .set_parameter_expression("Size", "'M7'", None)
            .is_err()
    );
    assert_eq!(value(&b.doc, "d3"), 8.5);
}

#[test]
fn tables_and_parameters_share_one_namespace() {
    let mut b = block_with_screws();
    // A table cannot take a parameter's name, nor a parameter a table's.
    let mut table = screws();
    table.name = "Size".to_owned();
    assert_eq!(
        b.doc.set_table(table).unwrap_err().to_string(),
        "Size is the name of a parameter"
    );
    let error = b
        .doc
        .add_parameter_expression("Screws", "1 mm", None, "")
        .unwrap_err()
        .to_string();
    assert_eq!(error, "Screws is the name of a table");
    assert!(b.doc.rename_parameter("Size", "Screws").is_err());
    assert!(b.doc.rename_table("Screws", "d1").is_err());
    assert!(b.doc.rename_table("Screws", "mm").is_err());
}

#[test]
fn renames_rewrite_expressions_and_limits() {
    let mut b = block_with_screws();
    let limits = ParamLimits {
        max: Some("INDEX(Screws.k; 1) + 1 mm".to_owned()),
        ..ParamLimits::default()
    };
    b.doc
        .add_parameter_with_limits("Gap", "10 mm", None, "", limits)
        .unwrap();
    assert_eq!(value(&b.doc, "Gap"), 4.0);
    b.doc.rename_table("Screws", "Bolts").unwrap();
    assert_eq!(b.doc.undo_label(), Some("Rename Table Screws to Bolts"));
    assert_eq!(
        expression(&b.doc, "d3"),
        "XLOOKUP(Size; Bolts.Size; Bolts.dk)"
    );
    let gap = |doc: &Document<MockKernel>| {
        let params = doc.parameters();
        params
            .get(params.find("Gap").unwrap())
            .unwrap()
            .limits()
            .max
            .clone()
    };
    assert_eq!(gap(&b.doc).as_deref(), Some("INDEX(Bolts.k; 1) + 1 mm"));
    assert!(b.doc.table("Screws").is_none());
    assert_eq!(b.doc.table_users("Bolts"), ["d3", "Gap"]);
    b.doc.rename_table_column("Bolts", "k", "head").unwrap();
    assert_eq!(gap(&b.doc).as_deref(), Some("INDEX(Bolts.head; 1) + 1 mm"));
    b.doc.rename_table_column("Bolts", "dk", "dia").unwrap();
    assert_eq!(
        expression(&b.doc, "d3"),
        "XLOOKUP(Size; Bolts.Size; Bolts.dia)"
    );
    // Values did not change.
    assert_eq!(value(&b.doc, "d3"), 8.5);
    assert_eq!(value(&b.doc, "Gap"), 4.0);
    // A column name taken, or not a name, is refused.
    assert!(b.doc.rename_table_column("Bolts", "dia", "d").is_err());
    assert!(b.doc.rename_table_column("Bolts", "dia", "a b").is_err());
    assert!(b.doc.rename_table_column("Bolts", "nope", "x").is_err());
    // Undo brings the old names back.
    b.doc.undo();
    b.doc.undo();
    b.doc.undo();
    assert_eq!(
        expression(&b.doc, "d3"),
        "XLOOKUP(Size; Screws.Size; Screws.dk)"
    );
    assert!(b.doc.table("Screws").is_some());
}

#[test]
fn a_table_in_use_is_not_deleted() {
    let mut b = block_with_screws();
    assert_eq!(
        b.doc.delete_table("Screws").unwrap_err().to_string(),
        "table Screws is used by d3"
    );
    assert!(b.doc.delete_table("Nuts").is_err());
    b.doc.set_parameter_expression("d3", "20 mm", None).unwrap();
    b.doc.delete_table("Screws").unwrap();
    assert_eq!(b.doc.undo_label(), Some("Delete Table Screws"));
    assert!(b.doc.tables().is_empty());
    b.doc.undo();
    assert!(b.doc.table("Screws").is_some());
}

#[test]
fn undo_and_redo_restore_tables_with_their_values() {
    let mut b = block_with_screws();
    b.doc
        .set_table(screws_with(2, 2, TableCell::Number(10.0)))
        .unwrap();
    assert_eq!(value(&b.doc, "d3"), 10.0);
    assert_eq!(b.doc.undo().as_deref(), Some("Edit Table Screws"));
    assert_eq!(b.doc.table("Screws"), Some(&screws()));
    assert_eq!(value(&b.doc, "d3"), 8.5);
    assert_eq!(b.history(b.body), "prism(F2:0..8.5)");
    // The parameters evaluate with the table restored.
    b.doc
        .set_parameter_expression("Size", "'M4'", None)
        .unwrap();
    assert_eq!(value(&b.doc, "d3"), 7.0);
    b.doc.undo();
    assert_eq!(b.doc.redo().as_deref(), Some("Change Size"));
    assert_eq!(value(&b.doc, "d3"), 7.0);
}

#[test]
fn tables_are_saved_and_read_back() {
    let b = block_with_screws();
    let saved = b.doc.to_json();
    let file: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["tables"][0]["name"], "Screws");
    assert_eq!(file["tables"][0]["rows"][2], json!(["M5", 5.0, 8.5, 5.0]));
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.table("Screws"), Some(&screws()));
    assert_eq!(value(&loaded, "d3"), 8.5);
    // A design without tables has no field.
    let plain = block().doc.to_json();
    assert!(!plain.contains("\"tables\""));
    // A table with a parameter's name, or a wrong cell, is refused.
    let mut file2 = file.clone();
    file2["tables"][0]["name"] = json!("Size");
    assert!(Document::from_json(&file2.to_string(), MockKernel::default()).is_err());
    let mut file3 = file;
    file3["tables"][0]["rows"][0][1] = json!("3");
    let error = match Document::from_json(&file3.to_string(), MockKernel::default()) {
        Ok(_) => panic!("a text in a number column was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("tables[0]"), "{error}");
}

#[test]
fn a_copy_into_another_design_brings_its_tables() {
    let b = block_with_screws();
    // The other design has a different table of the same name.
    let mut doc = Document::new(MockKernel::default());
    doc.set_table(screws_with(2, 2, TableCell::Number(9.0)))
        .unwrap();
    doc.add_component_copy(
        &b.doc,
        Some("Part"),
        ComponentUid::ROOT,
        Transform::IDENTITY,
    )
    .unwrap();
    let names: Vec<&str> = doc.tables().iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Screws", "Screws_1"]);
    assert_eq!(doc.table("Screws_1").unwrap().rows, screws().rows);
    let copied = doc
        .parameters()
        .iter()
        .find(|p| p.expression().contains("XLOOKUP"))
        .unwrap();
    assert_eq!(
        copied.expression(),
        "XLOOKUP(Size; Screws_1.Size; Screws_1.dk)"
    );
    assert_eq!(copied.value(), 8.5);
    // Into a design with the same table, the copy uses it.
    let mut same = Document::new(MockKernel::default());
    same.set_table(screws()).unwrap();
    same.add_component_copy(
        &b.doc,
        Some("Part"),
        ComponentUid::ROOT,
        Transform::IDENTITY,
    )
    .unwrap();
    assert_eq!(same.tables().len(), 1);
}

#[test]
fn a_configuration_row_sets_only_the_key() {
    let mut b = block_with_screws();
    let row = |name: &str| ConfigurationRow {
        name: name.to_owned(),
        values: BTreeMap::from([("Size".to_owned(), format!("'{name}'"))]),
        ..ConfigurationRow::default()
    };
    b.doc
        .set_configurations(Configurations {
            parameters: vec!["Size".to_owned()],
            rows: vec![row("M3"), row("M4"), row("M5")],
            ..Configurations::default()
        })
        .unwrap();
    let changed = b.doc.apply_configuration("M3").unwrap();
    assert!(changed.contains(&"d3".to_owned()), "{changed:?}");
    assert_eq!(value(&b.doc, "d3"), 5.5);
    assert_eq!(b.history(b.body), "prism(F2:0..5.5)");
    // A table edit that drops a row a configuration sets is refused: the
    // rows must stay applicable.
    let mut table = screws();
    table.rows.remove(1);
    assert!(b.doc.set_table(table).is_err());
}
