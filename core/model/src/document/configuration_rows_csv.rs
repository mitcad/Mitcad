// SPDX-License-Identifier: MIT
//! A configuration table's rows as CSV (mitcad#1044; `docs/configurations.md`,
//! "Filling a closed table", decision 12): the Rows tab's Export CSV… and
//! Import CSV…, the classic design table of a row per size and a column
//! per parameter.
//!
//! The file is read by the tables CSV reader (`tables_csv.rs`: units in the
//! header, decimal commas, inferred columns, free names, sizes). Its
//! columns:
//!
//! - `Name`, `Designation`, `Part Number` and `Supplier URL` (in any case,
//!   with or without spaces or `_`) fill the rows' fields; an empty cell
//!   clears a field, and a row without a name is named by its key.
//! - The first other column is the **key option** (`Size`); every other
//!   column sets the parameter of its name: an option's value, or a
//!   parameter column of the closed table. A parameter the design lacks is
//!   made with the column's unit ([`Configurations::fill_rows`]). A cell of
//!   a parameter column that is a formula (`Length / 4`) is kept as it is.
//! - A row of the same name is updated, the others are added, and the
//!   table's rows the file lacks are kept.
//!
//! The export writes the options' and the parameter columns' values in
//! their parameters' units (a formula as its text), then `Name`, and the
//! fields some row has; so export and import give the same rows.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::configuration_tables::add_new_parameters;
use super::{DocState, Document, ModelError, invalid};
use crate::configurations::{
    Cell, ConfigurationRow, Configurations, NewParameter, RowFields, RowSource, TableColumn,
    TableColumns,
};
use crate::expr::{Expr, ParameterTable, Unit, Value, format_number_exact};
use crate::kernel::Kernel;
use crate::tables_csv::{self, CsvCell, RefusedCell, TableCsv};

/// The name the file's columns go by in messages (`CSV.d is a text
/// column, and parameter d is a number`).
const FILE: &str = "CSV";

/// The header of each field column, as the export writes it.
const NAME: &str = "Name";
const DESIGNATION: &str = "Designation";
const PART_NUMBER: &str = "Part Number";
const SUPPLIER_URL: &str = "Supplier URL";

/// What a column of the file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowsCsvRole {
    /// The key option: the first column of values.
    Key,
    /// Another option's values.
    Option,
    /// A parameter column of the closed table.
    Parameter,
    Name,
    Designation,
    PartNumber,
    SupplierUrl,
}

/// A field column's role by its header: the export's names in any case,
/// spaces and `_` left out.
fn field_role(header: &str) -> Option<RowsCsvRole> {
    let plain: String = header
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect();
    match plain.as_str() {
        "name" => Some(RowsCsvRole::Name),
        "designation" => Some(RowsCsvRole::Designation),
        "partnumber" => Some(RowsCsvRole::PartNumber),
        "supplierurl" => Some(RowsCsvRole::SupplierUrl),
        _ => None,
    }
}

/// A column of the file as the preview shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowsCsvColumn {
    /// The name in the file (without its unit).
    pub name: String,
    /// The parameter's name, when the file's cannot be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed: Option<String>,
    /// `mm`, `""`, `Text`.
    pub unit: String,
    /// The header had no unit: the unit comes from the cells.
    pub inferred: bool,
    pub role: RowsCsvRole,
}

/// What importing a rows CSV does (the preview) or did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowsCsvImport {
    /// The key option (the file's first column of values).
    pub key: String,
    /// The columns in the file's order.
    pub columns: Vec<RowsCsvColumn>,
    /// The rows as the file has them: numbers in their column's unit,
    /// texts (a formula as its text), null for an empty or refused cell.
    pub rows: Vec<Vec<CsvCell>>,
    pub refused: Vec<RefusedCell>,
    /// Row names: rows the import adds, rows it changes, and the table's
    /// rows the file does not have, which stay.
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub kept: Vec<String>,
    /// The parameters the import makes.
    pub created: Vec<String>,
    /// The table is closed after the import.
    pub closed: bool,
    /// The table is already as the file has it.
    pub unchanged: bool,
    /// What the table would have wrong (the first problem), which refuses
    /// the import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// `1 row`, `3 rows`.
fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl RowsCsvImport {
    /// `3 rows from the file: added M3, M4, M5; made the parameters Size, d;
    /// kept Old; 1 cell refused`, or `…: all up to date`.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.added.is_empty() {
            parts.push(format!("added {}", self.added.join(", ")));
        }
        if !self.updated.is_empty() {
            parts.push(format!("updated {}", self.updated.join(", ")));
        }
        if !self.created.is_empty() {
            parts.push(format!(
                "made the {} {}",
                if self.created.len() == 1 {
                    "parameter"
                } else {
                    "parameters"
                },
                self.created.join(", ")
            ));
        }
        if parts.is_empty() {
            parts.push("all up to date".to_owned());
        }
        if !self.kept.is_empty() {
            parts.push(format!("kept {}", self.kept.join(", ")));
        }
        if !self.refused.is_empty() {
            parts.push(count(self.refused.len(), "cell refused", "cells refused"));
        }
        format!(
            "{} from the file: {}",
            count(self.rows.len(), "row", "rows"),
            parts.join("; ")
        )
    }

    /// Why the import is refused, None when it can be done: the first
    /// refused cell, else the table's first problem.
    pub fn refusal(&self) -> Option<String> {
        if let Some(first) = self.refused.first() {
            let column = if first.column.is_empty() {
                "beyond the last column".to_owned()
            } else {
                format!("column {}", first.column)
            };
            return Some(format!(
                "{} of the file refused, the first on line {}, {column}: {}; nothing is imported",
                count(self.refused.len(), "cell", "cells"),
                first.line,
                first.reason
            ));
        }
        self.problem
            .as_ref()
            .map(|problem| format!("{problem}; nothing is imported"))
    }
}

/// The file's columns as configurations read a table's
/// ([`TableColumns`]): a column by its name in the table (`renamed`, else
/// the file's).
struct FileColumns<'a>(&'a TableCsv);

impl TableColumns for FileColumns<'_> {
    fn table_column(&self, _table: &str, column: &str) -> Result<TableColumn, String> {
        let read = self.0;
        let i = read
            .columns
            .iter()
            .position(|c| c.table_name() == column)
            .ok_or_else(|| format!("the file has no column {column}"))?;
        let unit = read.columns[i].unit;
        Ok(TableColumn {
            unit,
            cells: read
                .rows
                .iter()
                .map(|row| match row.get(i) {
                    Some(CsvCell::Number(n)) => Some(Cell::Number(unit.to_canonical(*n))),
                    Some(CsvCell::Text(text)) => Some(Cell::Text(text.clone())),
                    _ => None,
                })
                .collect(),
        })
    }
}

/// A row's expression as a cell of a column in `unit`: the number of a
/// constant in the unit, the text of a text constant, else the expression
/// as a text (a formula).
fn cell_of(expression: &str, unit: Unit, constants: &ParameterTable) -> CsvCell {
    if unit.is_text() {
        return match constants.evaluate_value(expression) {
            Ok(Value::Text(text)) => CsvCell::Text(text),
            _ => CsvCell::Text(expression.to_owned()),
        };
    }
    match constants.evaluate(expression, unit) {
        Ok(q) if unit.canonical_to_unit(q.value).is_finite() => {
            CsvCell::Number(unit.canonical_to_unit(q.value))
        }
        _ => CsvCell::Text(expression.to_owned()),
    }
}

/// A cell as a field's text.
fn cell_text(cell: Option<&CsvCell>) -> String {
    match cell {
        Some(CsvCell::Text(text)) => text.clone(),
        Some(CsvCell::Number(n)) => format_number_exact(*n),
        _ => String::new(),
    }
}

/// A field of a row the export writes.
type Field = fn(&ConfigurationRow) -> Option<&String>;

fn designation_of(row: &ConfigurationRow) -> Option<&String> {
    row.designation.as_ref()
}

fn part_number_of(row: &ConfigurationRow) -> Option<&String> {
    row.part_number.as_ref()
}

fn supplier_url_of(row: &ConfigurationRow) -> Option<&String> {
    row.supplier_url.as_ref()
}

/// The fields the export writes after `Name`, each when a row has it.
const FIELDS: [(&str, Field); 3] = [
    (DESIGNATION, designation_of),
    (PART_NUMBER, part_number_of),
    (SUPPLIER_URL, supplier_url_of),
];

/// The rows of a table as CSV: the options and the parameter columns in
/// their parameters' units, `Name`, and each field some row has.
fn write_rows(state: &DocState) -> String {
    let table = &state.configurations;
    let parameters = &state.parameters;
    let constants = ParameterTable::new(parameters.context());
    let mut columns: Vec<(String, Unit)> = table
        .options
        .iter()
        .chain(&table.parameters)
        .map(|name| {
            let unit = parameters
                .find(name)
                .and_then(|id| parameters.get(id))
                .map_or(Unit::NONE, |p| p.unit());
            (name.clone(), unit)
        })
        .collect();
    let values = columns.len();
    columns.push((NAME.to_owned(), Unit::TEXT));
    let fields: Vec<(&str, Field)> = FIELDS
        .into_iter()
        .filter(|(_, field)| table.rows.iter().any(|row| field(row).is_some()))
        .collect();
    for (header, _) in &fields {
        columns.push(((*header).to_owned(), Unit::TEXT));
    }
    let rows: Vec<Vec<CsvCell>> = table
        .rows
        .iter()
        .map(|row| {
            let mut cells: Vec<CsvCell> = columns[..values]
                .iter()
                .map(|(name, unit)| match row.values.get(name) {
                    Some(expression) => cell_of(expression, *unit, &constants),
                    None => CsvCell::Empty,
                })
                .collect();
            cells.push(CsvCell::Text(row.name.clone()));
            for (_, field) in &fields {
                cells.push(field(row).map_or(CsvCell::Empty, |text| CsvCell::Text(text.clone())));
            }
            cells
        })
        .collect();
    tables_csv::write(&columns, &rows)
}

/// What importing `text` does to `state`: the outcome, the table and the
/// parameters it makes.
fn preview(
    state: &DocState,
    text: &str,
) -> Result<(RowsCsvImport, Configurations, Vec<NewParameter>), ModelError> {
    let mut read =
        tables_csv::parse(text, state.parameters.context()).map_err(|e| invalid(e.to_string()))?;
    // The columns' roles: the fields by their headers, the first other
    // column the key.
    let mut roles: Vec<Option<RowsCsvRole>> = Vec::with_capacity(read.columns.len());
    let mut key = None;
    let mut seen = BTreeSet::new();
    for (i, column) in read.columns.iter().enumerate() {
        let role = field_role(&column.name);
        if let Some(field) = role
            && !seen.insert(field)
        {
            return Err(invalid(format!(
                "the file has two columns of {}: {} is one of them",
                match field {
                    RowsCsvRole::Name => NAME,
                    RowsCsvRole::Designation => DESIGNATION,
                    RowsCsvRole::PartNumber => PART_NUMBER,
                    _ => SUPPLIER_URL,
                },
                column.name
            )));
        }
        if role.is_none() && key.is_none() {
            key = Some(i);
        }
        roles.push(role);
    }
    let Some(key) = key else {
        return Err(invalid(
            "the file has no column of values: its first column besides Name, Designation, \
             Part Number and Supplier URL is the key option",
        ));
    };
    let is_value = |c: usize| roles.get(c).is_some_and(Option::is_none);

    // Refused cells of the fields are their texts; those of a parameter
    // column that are formulas are kept as such.
    let mut expressions: Vec<BTreeMap<String, String>> = vec![BTreeMap::new(); read.rows.len()];
    let mut texts: BTreeMap<(usize, usize), String> = BTreeMap::new();
    let mut refused = Vec::with_capacity(read.refused.len());
    for cell in std::mem::take(&mut read.refused) {
        let column = read
            .columns
            .iter()
            .position(|c| c.table_name() == cell.column);
        let row = read.lines.iter().position(|&line| line == cell.line);
        let (Some(c), Some(r)) = (column, row) else {
            refused.push(cell);
            continue;
        };
        if !is_value(c) {
            texts.insert((r, c), cell.text.clone());
        } else if c != key
            && !read.columns[c].unit.is_text()
            && Expr::parse(&cell.text).is_ok_and(|e| !e.is_constant())
        {
            expressions[r].insert(cell.column.clone(), cell.text.clone());
            if let Some(slot) = read.rows.get_mut(r).and_then(|cells| cells.get_mut(c)) {
                *slot = CsvCell::Text(cell.text.clone());
            }
        } else {
            refused.push(cell);
        }
    }
    read.refused = refused;

    let field_column = |role: RowsCsvRole| roles.iter().position(|r| *r == Some(role));
    let field = |r: usize, role: RowsCsvRole| {
        field_column(role).map(|c| {
            texts
                .get(&(r, c))
                .cloned()
                .unwrap_or_else(|| cell_text(read.rows.get(r).and_then(|cells| cells.get(c))))
        })
    };
    let fields: Vec<RowFields> = (0..read.rows.len())
        .map(|r| RowFields {
            name: field(r, RowsCsvRole::Name),
            designation: field(r, RowsCsvRole::Designation),
            part_number: field(r, RowsCsvRole::PartNumber),
            supplier_url: field(r, RowsCsvRole::SupplierUrl),
        })
        .collect();
    let key_name = read.columns[key].table_name().to_owned();
    let columns: Vec<(String, String)> = read
        .columns
        .iter()
        .enumerate()
        .filter(|&(c, _)| c != key && is_value(c))
        .map(|(_, column)| {
            let name = column.table_name().to_owned();
            (name.clone(), name)
        })
        .collect();

    // A formula of the file's rows is no cell for the reader of columns.
    let mut cells = read.clone();
    for (r, given) in expressions.iter().enumerate() {
        for column in given.keys() {
            if let Some(c) = cells
                .columns
                .iter()
                .position(|col| col.table_name() == column)
                && let Some(slot) = cells.rows.get_mut(r).and_then(|row| row.get_mut(c))
            {
                *slot = CsvCell::Empty;
            }
        }
    }
    let tables = FileColumns(&cells);
    let source = RowSource {
        tables: &tables,
        table: FILE,
        key: &key_name,
        option: &key_name,
        columns,
        per_line: true,
        fields,
        expressions,
        create_key: true,
    };
    let mut table = state.configurations.clone();
    let filled = table
        .fill_rows(&state.parameters, &source)
        .map_err(invalid)?;
    let mut trial = state.parameters.clone();
    let problem = match add_new_parameters(&mut trial, &filled.created) {
        Err(e) => Some(e.to_string()),
        Ok(()) => table.problems(&trial).first().cloned(),
    };
    let kept: Vec<String> = state
        .configurations
        .rows
        .iter()
        .filter(|row| !filled.rows.contains(&row.name))
        .map(|row| row.name.clone())
        .collect();
    let columns = read
        .columns
        .iter()
        .zip(&roles)
        .enumerate()
        .map(|(c, (column, role))| RowsCsvColumn {
            name: column.name.clone(),
            renamed: column.renamed.clone(),
            unit: column.unit.to_string(),
            inferred: column.inferred,
            role: match role {
                Some(field) => *field,
                None if c == key => RowsCsvRole::Key,
                None if table.is_option(column.table_name()) => RowsCsvRole::Option,
                None => RowsCsvRole::Parameter,
            },
        })
        .collect();
    let outcome = RowsCsvImport {
        key: key_name.clone(),
        columns,
        rows: read.rows,
        refused: read.refused,
        added: filled.added,
        updated: filled.updated,
        kept,
        created: filled.created.iter().map(|p| p.name.clone()).collect(),
        closed: table.closed,
        unchanged: table == state.configurations && filled.created.is_empty(),
        problem,
    };
    Ok((outcome, table, filled.created))
}

impl<K: Kernel> Document<K> {
    /// The configuration table's rows as CSV, and their number.
    pub fn configuration_rows_csv(&self) -> (String, usize) {
        (
            write_rows(&self.state),
            self.state.configurations.rows.len(),
        )
    }

    /// What importing a rows CSV would do; nothing changes.
    pub fn preview_configuration_rows_csv(&self, text: &str) -> Result<RowsCsvImport, ModelError> {
        Ok(preview(&self.state, text)?.0)
    }

    /// Imports a rows CSV into the configuration table (the module
    /// comment) as one undo step (`Import Configuration Rows`; none when
    /// the table is already as the file has it), with the parameters it
    /// makes; nothing is recomputed. Refused while a cell of the file is
    /// refused or the table would have a problem (see the preview).
    pub fn import_configuration_rows_csv(
        &mut self,
        text: &str,
    ) -> Result<RowsCsvImport, ModelError> {
        let (outcome, table, created) = preview(&self.state, text)?;
        if let Some(reason) = outcome.refusal() {
            return Err(invalid(reason));
        }
        if outcome.unchanged {
            return Ok(outcome);
        }
        self.apply_with(|state| {
            add_new_parameters(&mut state.parameters, &created)?;
            if let Some(problem) = table.problems(&state.parameters).first() {
                return Err(invalid(problem.clone()));
            }
            state.configurations = table;
            Ok(("Import Configuration Rows".to_owned(), outcome, false))
        })
    }
}
