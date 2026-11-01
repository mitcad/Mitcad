// SPDX-License-Identifier: MIT
//! Tables in configurations (mitcad#806; `docs/configurations.md`, "Tables
//! and configurations", decision 9). The design's tables (mitcad#803) meet
//! its configurations in three places:
//!
//! - **Option values from a column.** An option's `values` limit can name
//!   a table column (`"values": "Screws.Size"`, kept as the one-entry list
//!   `["Screws.Size"]`): its domain is the column's distinct non-empty
//!   cells, in the table's order. A table edit that takes an option's value
//!   out of its column moves the option ([`Configurations::column_moves`]),
//!   and the cascade runs after it.
//! - **Combinations in a table.** The rule action `in_table` requires the
//!   values of some options to be a row of a table (`{"in_table": "Made",
//!   "columns": {"Color": "Color", "Length": "Length"}}`: option to
//!   column). For the choosers and the cascade it narrows each mapped
//!   option to the cells of the rows that match the mapped options before
//!   it ([`Configurations::in_table_allowed`]); the final check of the
//!   whole combination is exact ([`Configurations::in_table_violation`]).
//! - **Rows generated from a table** ([`Configurations::generate_rows`]):
//!   one row per key of a column, named by the key, setting one option to
//!   it. A row of the same name is updated, not duplicated. The table's
//!   other columns can become the closed table's parameter columns
//!   (mitcad#1044, `fill.rs`).
//!
//! The tables are read only through [`TableColumns`], the one place here
//! that knows how a design keeps them.

use std::collections::BTreeMap;

use crate::expr::{
    ColumnError, DEFAULT_DECIMALS, Expr, Lookup, TableCell as ExprCell, Unit, Value, format_number,
    format_value, is_valid_name, replace_table_references, text_literal, value_to_expression,
};
use crate::parameters::Parameters;

use super::fill::{NewParameter, RowSource};
use super::{ConfigurationMove, Configurations, DomainValue, OptionValue, RuleAction};

// The tables' side (mitcad#803).

/// A cell of a table column as configurations read it.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// A number in canonical units (millimetres, radians); a unitless
    /// column's number as it is.
    Number(f64),
    Text(String),
}

/// A column of a table: its unit ([`Unit::TEXT`] for a text column) and
/// one cell per row (None: empty).
#[derive(Debug, Clone, PartialEq)]
pub struct TableColumn {
    pub unit: Unit,
    pub cells: Vec<Option<Cell>>,
}

/// The design's tables as configurations read them.
pub trait TableColumns {
    /// A column of a table, or what is missing (`the design has no table
    /// Screws`, `table Screws has no column dk`).
    fn table_column(&self, table: &str, column: &str) -> Result<TableColumn, String>;
}

/// The tables of a design that has none.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTables;

impl TableColumns for NoTables {
    fn table_column(&self, table: &str, _column: &str) -> Result<TableColumn, String> {
        Err(format!("the design has no table {table}"))
    }
}

/// The design's tables as configurations read them: the one place that
/// knows where a design keeps its tables (mitcad#803). The rules, the
/// cascade and the document's commands all read them from here.
pub fn tables_of(parameters: &Parameters) -> impl TableColumns + '_ {
    // The parameters evaluate with the design's tables (mitcad#803).
    LookupColumns(parameters.tables().map(|tables| &**tables as &dyn Lookup))
}

/// Tables as a [`Lookup`] gives their columns (`Lookup::column`,
/// mitcad#802): the design's tables, or the tables a table edit is about
/// to make the design's.
pub fn columns_of(tables: &dyn Lookup) -> impl TableColumns + '_ {
    LookupColumns(Some(tables))
}

/// The columns of a [`Lookup`]'s tables; None: no tables.
struct LookupColumns<'a>(Option<&'a dyn Lookup>);

impl TableColumns for LookupColumns<'_> {
    fn table_column(&self, table: &str, column: &str) -> Result<TableColumn, String> {
        let Some(tables) = self.0 else {
            return NoTables.table_column(table, column);
        };
        let data = tables.column(table, column).map_err(|e| match e {
            ColumnError::NoTable => format!("the design has no table {table}"),
            ColumnError::NoColumn => format!("table {table} has no column {column}"),
        })?;
        let unit = data.unit;
        Ok(TableColumn {
            unit,
            cells: data
                .cells
                .iter()
                .map(|cell| match *cell {
                    ExprCell::Empty => None,
                    // In the column's unit; a unitless number as it is.
                    ExprCell::Number(n) => Some(Cell::Number(unit.to_canonical(n))),
                    ExprCell::Text(text) => Some(Cell::Text(text.to_owned())),
                })
                .collect(),
        })
    }
}

// Column references.

/// The table and the column of a column reference (`Screws.Size`); None
/// for any other text. A column's name may be a reserved word (`in`),
/// since it is only ever written after its table.
pub fn column_reference(text: &str) -> Option<(&str, &str)> {
    let (table, column) = text.trim().split_once('.')?;
    let mut chars = column.chars();
    let column_name = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_');
    (is_valid_name(table) && column_name).then_some((table, column))
}

/// The column a list of allowed values names: a list of one column
/// reference (`["Screws.Size"]`). A column is a whole list; it is not
/// mixed with listed values.
pub fn list_column(values: &[String]) -> Option<(&str, &str)> {
    match values {
        [only] => column_reference(only),
        _ => None,
    }
}

/// Rewrites a column reference for a table rename (`old` to `new`); other
/// texts stay.
pub fn rename_table_in(text: &mut String, old: &str, new: &str) {
    if let Some((table, column)) = column_reference(text)
        && table == old
    {
        *text = format!("{new}.{column}");
    }
}

/// Rewrites a column reference for a column rename of `table`.
pub fn rename_column_in(text: &mut String, table: &str, old: &str, new: &str) {
    if let Some((t, column)) = column_reference(text)
        && t == table
        && column == old
    {
        *text = format!("{table}.{new}");
    }
}

/// Rewrites parsed table references in a rule expression. Conditions are
/// wrapped like `parse_condition`, so a top-level single `=` is accepted.
fn rename_expression_table(
    text: &mut String,
    table: &str,
    column: Option<&str>,
    new: &str,
    condition: bool,
) {
    if !condition && column_reference(text).is_some() {
        match column {
            None => rename_table_in(text, table, new),
            Some(old) => rename_column_in(text, table, old, new),
        }
    } else if condition {
        let wrapped = format!("({text})");
        if let Ok(expr) = Expr::parse(&wrapped) {
            let renamed = replace_table_references(&wrapped, &expr, table, column, new);
            *text = renamed[1..renamed.len() - 1].to_owned();
        }
    } else if let Ok(expr) = Expr::parse(text) {
        *text = replace_table_references(text, &expr, table, column, new);
    }
}

// Values of options.

/// A key of a column: the row name it gives ([`Configurations::generate_rows`])
/// and the option value it is.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Key {
    pub(super) name: String,
    pub(super) value: DomainValue,
}

/// Whether two numbers are equal as `==` compares them (relative 1e-9).
fn same_number(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// Whether two values of an option are the same: texts by their
/// characters (case included), numbers within a billionth.
pub fn same_option_value(a: &OptionValue, b: &OptionValue) -> bool {
    match (a, b) {
        (OptionValue::Number(x), OptionValue::Number(y)) => same_number(*x, *y),
        (OptionValue::Text(x), OptionValue::Text(y)) => x == y,
        _ => false,
    }
}

/// An option's current value: its text, or its number in canonical units.
pub fn current_option_value(parameters: &Parameters, option: &str) -> Option<OptionValue> {
    let p = parameters.get(parameters.find(option)?)?;
    Some(match p.text() {
        Some(text) => OptionValue::Text(text.to_owned()),
        None => OptionValue::Number(p.value()),
    })
}

/// An option's value as text for people: `M5`, `16 mm`.
fn value_text(value: &OptionValue, unit: Unit) -> String {
    match value {
        OptionValue::Number(v) => format_value(*v, unit, DEFAULT_DECIMALS),
        OptionValue::Text(text) => text.clone(),
        OptionValue::Expression(text) => text.clone(),
    }
}

/// The unit of an option's parameter, and whether it is driven (its
/// expression is a formula).
fn option_unit(parameters: &Parameters, option: &str) -> Result<(Unit, bool), String> {
    let p = parameters
        .find(option)
        .and_then(|id| parameters.get(id))
        .ok_or_else(|| format!("option {option} does not exist"))?;
    let driven = Expr::parse(p.expression()).is_ok_and(|e| !e.is_constant());
    Ok((p.unit(), driven))
}

/// The cells of a column as values of an option (or, `who` says, of a
/// closed table's parameter: `option Size`, `parameter d`) in `unit`, one
/// per row (None: an empty cell). A text option takes a text column; a
/// number option a number column of its dimension, or a unitless one, whose
/// numbers are read in the option's unit (`16` in a length option in
/// millimetres is 16 mm).
pub(super) fn column_keys(
    tables: &dyn TableColumns,
    table: &str,
    column: &str,
    who: &str,
    unit: Unit,
) -> Result<Vec<Option<Key>>, String> {
    let cells = tables.table_column(table, column)?;
    let col = cells.unit;
    if unit.is_text() != col.is_text() {
        return Err(if col.is_text() {
            format!("{table}.{column} is a text column, and {who} is a number")
        } else {
            format!("{table}.{column} is a number column, and {who} is a text")
        });
    }
    let promoted =
        !unit.is_text() && col.dims().is_dimensionless() && !unit.dims().is_dimensionless();
    if !unit.is_text() && !promoted && col.dims() != unit.dims() {
        return Err(format!(
            "{table}.{column} is in {}, which {who} in {unit} cannot take",
            if col.is_unitless() {
                "no unit".to_owned()
            } else {
                col.to_string()
            }
        ));
    }
    Ok(cells
        .cells
        .into_iter()
        .map(|cell| {
            let key = match cell? {
                Cell::Text(text) => {
                    if text.trim().is_empty() {
                        return None;
                    }
                    Key {
                        name: text.clone(),
                        value: DomainValue {
                            text: text.clone(),
                            expression: text_literal(&text),
                            value: OptionValue::Text(text),
                        },
                    }
                }
                Cell::Number(v) => {
                    let canonical = if promoted { unit.to_canonical(v) } else { v };
                    let value = OptionValue::Number(canonical);
                    Key {
                        name: format_number(col.canonical_to_unit(v), DEFAULT_DECIMALS),
                        value: DomainValue {
                            text: value_text(&value, unit),
                            // The column's unit keeps the cell exact
                            // (`0.25 in`); a unitless cell takes the
                            // option's.
                            expression: if promoted {
                                value_to_expression(canonical, unit)
                            } else {
                                value_to_expression(canonical, col)
                            },
                            value,
                        },
                    }
                }
            };
            Some(key)
        })
        .collect())
}

/// The distinct keys of a column, in the table's order.
fn distinct(keys: Vec<Option<Key>>) -> Vec<Key> {
    let mut out: Vec<Key> = Vec::new();
    for key in keys.into_iter().flatten() {
        if !out
            .iter()
            .any(|k| same_option_value(&k.value.value, &key.value.value))
        {
            out.push(key);
        }
    }
    out
}

/// An option's domain from a table column: the column's distinct
/// non-empty cells, in the table's order, as values of the option.
pub fn column_domain(
    parameters: &Parameters,
    tables: &dyn TableColumns,
    table: &str,
    column: &str,
    option: &str,
) -> Result<Vec<DomainValue>, String> {
    let (unit, _) = option_unit(parameters, option)?;
    Ok(distinct(column_keys(
        tables,
        table,
        column,
        &format!("option {option}"),
        unit,
    )?)
    .into_iter()
    .map(|k| k.value)
    .collect())
}

/// The column a parameter's `values` limit names, if it names one.
pub fn values_column(parameters: &Parameters, name: &str) -> Option<(String, String)> {
    let p = parameters.get(parameters.find(name)?)?;
    let values = p.limits().values.as_deref()?;
    list_column(values).map(|(t, c)| (t.to_owned(), c.to_owned()))
}

/// The values of a rule's `values` or `exclude` list that is a table
/// column (`["Screws.Size"]`), as values of `option`; None when the list
/// is no column (its entries are then expressions).
pub fn list_values(
    parameters: &Parameters,
    tables: &dyn TableColumns,
    option: &str,
    list: &[String],
) -> Option<Result<Vec<DomainValue>, String>> {
    let (table, column) = list_column(list)?;
    Some(column_domain(parameters, tables, table, column, option))
}

// The rule action `in_table`.

/// An `in_table` action: the values of the mapped options must be a row
/// of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InTable<'a> {
    pub table: &'a str,
    /// Option to column.
    pub columns: &'a BTreeMap<String, String>,
}

impl RuleAction {
    /// The action as an `in_table` action, if it is one.
    pub fn as_in_table(&self) -> Option<InTable<'_>> {
        Some(InTable {
            table: self.in_table.as_deref()?,
            columns: self.columns.as_ref()?,
        })
    }
}

impl Configurations {
    /// Every `in_table` action of the rules, enabled or not.
    pub fn in_table_actions(&self) -> impl Iterator<Item = InTable<'_>> {
        self.rules
            .iter()
            .flat_map(|rule| rule.actions())
            .filter_map(RuleAction::as_in_table)
    }

    /// Keeps rule expressions and `in_table` actions in step with a table
    /// rename. The options' `values` follow with their parameter limits.
    pub fn rename_table(&mut self, old: &str, new: &str) {
        for rule in &mut self.rules {
            if let Some(when) = rule.when.as_mut() {
                rename_expression_table(when, old, None, new, true);
            }
            for action in rule.then.iter_mut().chain(&mut rule.otherwise) {
                if action.in_table.as_deref() == Some(old) {
                    action.in_table = Some(new.to_owned());
                }
                for (what, text) in action.expressions_mut() {
                    rename_expression_table(text, old, None, new, what == "require");
                }
            }
        }
    }

    /// Keeps rule expressions and column mappings in step with a column
    /// rename of `table`.
    pub fn rename_table_column(&mut self, table: &str, old: &str, new: &str) {
        for rule in &mut self.rules {
            if let Some(when) = rule.when.as_mut() {
                rename_expression_table(when, table, Some(old), new, true);
            }
            for action in rule.then.iter_mut().chain(&mut rule.otherwise) {
                if action.in_table.as_deref() == Some(table)
                    && let Some(columns) = action.columns.as_mut()
                {
                    for column in columns.values_mut() {
                        if column == old {
                            new.clone_into(column);
                        }
                    }
                }
                for (what, text) in action.expressions_mut() {
                    rename_expression_table(text, table, Some(old), new, what == "require");
                }
            }
        }
    }
}

/// The rows of an `in_table` action's table as values of its options in
/// the table's option order; a row with an empty cell in a mapped column
/// takes part in nothing.
struct Combinations {
    options: Vec<String>,
    rows: Vec<Vec<DomainValue>>,
}

impl Configurations {
    fn in_table_rows(
        &self,
        action: InTable<'_>,
        parameters: &Parameters,
        tables: &dyn TableColumns,
    ) -> Result<Combinations, String> {
        let options: Vec<String> = self
            .options
            .iter()
            .filter(|o| action.columns.contains_key(*o))
            .cloned()
            .collect();
        let mut columns = Vec::with_capacity(options.len());
        for option in &options {
            let (unit, _) = option_unit(parameters, option)?;
            columns.push(column_keys(
                tables,
                action.table,
                &action.columns[option],
                &format!("option {option}"),
                unit,
            )?);
        }
        let count = columns.iter().map(Vec::len).max().unwrap_or(0);
        let rows = (0..count)
            .filter_map(|i| {
                columns
                    .iter()
                    .map(|keys| keys.get(i).cloned().flatten().map(|k| k.value))
                    .collect::<Option<Vec<DomainValue>>>()
            })
            .collect();
        Ok(Combinations { options, rows })
    }

    /// The values an `in_table` action allows `option`, given the current
    /// values of the options before it that the action maps: the option's
    /// cells in the rows that match them, distinct, in the table's order.
    /// None when the action does not map the option.
    pub fn in_table_allowed(
        &self,
        action: InTable<'_>,
        option: &str,
        parameters: &Parameters,
        tables: &dyn TableColumns,
    ) -> Result<Option<Vec<DomainValue>>, String> {
        let combinations = self.in_table_rows(action, parameters, tables)?;
        let Some(at) = combinations.options.iter().position(|o| o == option) else {
            return Ok(None);
        };
        let before: Vec<Option<OptionValue>> = combinations.options[..at]
            .iter()
            .map(|o| current_option_value(parameters, o))
            .collect();
        let mut out: Vec<DomainValue> = Vec::new();
        for row in &combinations.rows {
            let matches = row[..at].iter().zip(&before).all(|(cell, value)| {
                value
                    .as_ref()
                    .is_some_and(|v| same_option_value(&cell.value, v))
            });
            if matches
                && !out
                    .iter()
                    .any(|v| same_option_value(&v.value, &row[at].value))
            {
                out.push(row[at].clone());
            }
        }
        Ok(Some(out))
    }

    /// Whether the current values of an `in_table` action's options are a
    /// row of its table: None when they are, else what is wrong (`Color
    /// Red and Length 1500 mm is no row of Made`).
    pub fn in_table_violation(
        &self,
        action: InTable<'_>,
        parameters: &Parameters,
        tables: &dyn TableColumns,
    ) -> Result<Option<String>, String> {
        let combinations = self.in_table_rows(action, parameters, tables)?;
        let mut values = Vec::with_capacity(combinations.options.len());
        for option in &combinations.options {
            values.push(
                current_option_value(parameters, option)
                    .ok_or_else(|| format!("option {option} does not exist"))?,
            );
        }
        let found = combinations.rows.iter().any(|row| {
            row.iter()
                .zip(&values)
                .all(|(cell, value)| same_option_value(&cell.value, value))
        });
        if found {
            return Ok(None);
        }
        let mut words = Vec::with_capacity(values.len());
        for (option, value) in combinations.options.iter().zip(&values) {
            let (unit, _) = option_unit(parameters, option)?;
            words.push(format!("{option} {}", value_text(value, unit)));
        }
        Ok(Some(format!(
            "{} is no row of {}",
            words.join(" and "),
            action.table
        )))
    }

    /// What is wrong with an `in_table` action, one message per problem:
    /// no columns, a mapped name that is no option of the table, a table or
    /// column that does not exist, a column of the wrong kind.
    pub fn in_table_problems(
        &self,
        action: InTable<'_>,
        parameters: &Parameters,
        tables: &dyn TableColumns,
    ) -> Vec<String> {
        let mut out = Vec::new();
        if action.columns.is_empty() {
            out.push(format!(
                "in_table {} maps no option to a column",
                action.table
            ));
        }
        for (option, column) in action.columns {
            if !self.is_option(option) {
                out.push(format!(
                    "in_table {}: {option} is not an option of the table",
                    action.table
                ));
                continue;
            }
            match option_unit(parameters, option) {
                Ok((unit, _)) => {
                    if let Err(e) = column_keys(
                        tables,
                        action.table,
                        column,
                        &format!("option {option}"),
                        unit,
                    ) {
                        out.push(format!("in_table {}: {e}", action.table));
                    }
                }
                Err(e) => out.push(e),
            }
        }
        out
    }
}

// Options whose values are a column, after a table edit.

/// An option that a table edit moves: the constant it is set to, and the
/// move as the cascade reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnMove {
    pub option: String,
    pub expression: String,
    pub change: ConfigurationMove,
}

/// The value an option moves to in a list of allowed values: the nearest
/// for a number (the lower of two as near), else `preferred` when it is in
/// the list, else the first.
fn move_target<'a>(
    list: &'a [DomainValue],
    current: Option<&OptionValue>,
    preferred: Option<&OptionValue>,
) -> Option<&'a DomainValue> {
    if let Some(OptionValue::Number(v)) = current {
        let mut best: Option<(&DomainValue, f64)> = None;
        for candidate in list {
            if let OptionValue::Number(c) = candidate.value {
                let d = (c - v).abs();
                let closer = match best {
                    None => true,
                    Some((b, bd)) => {
                        d < bd || (d == bd && matches!(b.value, OptionValue::Number(bv) if c < bv))
                    }
                };
                if closer {
                    best = Some((candidate, d));
                }
            }
        }
        if let Some((b, _)) = best {
            return Some(b);
        }
    }
    preferred
        .and_then(|p| list.iter().find(|v| same_option_value(&v.value, p)))
        .or_else(|| list.first())
}

impl Configurations {
    /// The options whose allowed values are a column of `table`, in option
    /// order.
    pub fn options_on_table(&self, parameters: &Parameters, table: &str) -> Vec<String> {
        self.options
            .iter()
            .filter(|o| values_column(parameters, o).is_some_and(|(t, _)| t == table))
            .cloned()
            .collect()
    }

    /// The rules that use `table`, for a refused delete: those with an
    /// `in_table` action or a list of its column (`the rule Made sizes`);
    /// empty when none does. (An option whose values are its column is a
    /// parameter that uses it, as the parameters say.)
    pub fn table_users(&self, table: &str) -> Vec<String> {
        let mut out = Vec::new();
        for rule in &self.rules {
            let uses = rule.actions().any(|action| {
                action.in_table.as_deref() == Some(table)
                    || [&action.values, &action.exclude]
                        .into_iter()
                        .flatten()
                        .flatten()
                        .any(|text| column_reference(text).is_some_and(|(t, _)| t == table))
            });
            if uses {
                out.push(format!("the rule {}", rule.name));
            }
        }
        out
    }

    /// The value of an option in the default row, if the row sets it.
    fn default_row_value(&self, parameters: &Parameters, option: &str) -> Option<OptionValue> {
        let expression = self.default_row()?.values.get(option)?;
        let (unit, _) = option_unit(parameters, option).ok()?;
        match parameters.table().evaluate_value(expression).ok()? {
            Value::Text(text) => Some(OptionValue::Text(text)),
            Value::Number(q) => {
                // A unitless constant in a number option is in its unit.
                let v = if q.dims.is_dimensionless() && !unit.dims().is_dimensionless() {
                    unit.to_canonical(q.value)
                } else {
                    q.value
                };
                Some(OptionValue::Number(v))
            }
        }
    }

    /// After an edit of `table` (`tables` as they are after it): each
    /// option whose allowed values are a column of that table and whose
    /// value is no longer one of them, with the value it moves to: the
    /// nearest for a number, else the default row's value when the column
    /// has it, else the column's first value. A driven option is not
    /// moved (the edit then fails on it as on any parameter), nor one whose
    /// column has no values left.
    pub fn column_moves(
        &self,
        parameters: &Parameters,
        tables: &dyn TableColumns,
        table: &str,
    ) -> Result<Vec<ColumnMove>, String> {
        let mut out = Vec::new();
        for option in self.options_on_table(parameters, table) {
            let Some((t, column)) = values_column(parameters, &option) else {
                continue;
            };
            let (unit, driven) = option_unit(parameters, &option)?;
            if driven {
                continue;
            }
            let list = column_domain(parameters, tables, &t, &column, &option)?;
            let current = current_option_value(parameters, &option);
            if current
                .as_ref()
                .is_some_and(|c| list.iter().any(|v| same_option_value(&v.value, c)))
            {
                continue;
            }
            let preferred = self.default_row_value(parameters, &option);
            let Some(to) = move_target(&list, current.as_ref(), preferred.as_ref()) else {
                continue;
            };
            let from_expression = parameters
                .find(&option)
                .and_then(|id| parameters.get(id))
                .map(|p| p.expression().to_owned())
                .unwrap_or_default();
            out.push(ColumnMove {
                option: option.clone(),
                expression: to.expression.clone(),
                change: ConfigurationMove {
                    option: option.clone(),
                    from: current
                        .as_ref()
                        .map(|c| value_text(c, unit))
                        .unwrap_or_default(),
                    to: to.text.clone(),
                    from_expression,
                    to_expression: to.expression.clone(),
                    rule: format!("its allowed values {t}.{column}"),
                },
            });
        }
        Ok(out)
    }
}

// Rows generated from a table.

/// What [`Configurations::generate_rows`] (and the rows CSV,
/// [`Configurations::fill_rows`]) did, by row name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratedRows {
    /// Every row of the column's keys, in the table's order.
    pub rows: Vec<String>,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    /// The parameters the design lacks that the rows set (mitcad#1044):
    /// the caller adds them with the table.
    pub created: Vec<NewParameter>,
}

impl Configurations {
    /// Makes one row per key of a column (`Screws.Size`): the row is named
    /// by the key (`M5`; a number without its unit, `16`) and sets
    /// `option` to it. A row of the same name is updated, not duplicated:
    /// it keeps its other values, designation, part number and supplier
    /// URL. The option becomes an option of the table when it is not one
    /// yet (a closed table's parameter column moves to the options with its
    /// rows' values, mitcad#1044). Refused when the option is no user
    /// parameter or cannot take the column's values, and when the column
    /// has none.
    ///
    /// `columns` (mitcad#1044, decision 12) are other columns of the table
    /// that become the closed table's parameter columns: each row sets the
    /// parameter of the column's name to the cell of its key's first row.
    /// A parameter the design lacks is made with the column's unit
    /// ([`GeneratedRows::created`]); the table becomes closed.
    pub fn generate_rows(
        &mut self,
        parameters: &Parameters,
        tables: &dyn TableColumns,
        table: &str,
        column: &str,
        option: &str,
        columns: &[String],
    ) -> Result<GeneratedRows, String> {
        let source = RowSource {
            tables,
            table,
            key: column,
            option,
            columns: columns
                .iter()
                .filter(|c| c.as_str() != column)
                .map(|c| (c.clone(), c.clone()))
                .collect(),
            per_line: false,
            fields: Vec::new(),
            expressions: Vec::new(),
            create_key: false,
        };
        self.fill_rows(parameters, &source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::ParamSpec;

    /// The constants of a list of values (`'M5'`, `16 mm`).
    fn domain_expressions(values: &[DomainValue]) -> Vec<String> {
        values.iter().map(|v| v.expression.clone()).collect()
    }

    /// A value of an option in `unit` as a chooser lists it.
    fn domain_value(value: &OptionValue, unit: Unit) -> DomainValue {
        DomainValue {
            value: value.clone(),
            text: value_text(value, unit),
            expression: match value {
                OptionValue::Number(v) => value_to_expression(*v, unit),
                OptionValue::Text(text) => text_literal(text),
                OptionValue::Expression(text) => text.clone(),
            },
        }
    }

    /// Tables kept as columns by (table, column).
    #[derive(Default)]
    struct MockTables(BTreeMap<(String, String), TableColumn>);

    impl MockTables {
        fn with(mut self, table: &str, column: &str, unit: Unit, cells: Vec<Option<Cell>>) -> Self {
            self.0.insert(
                (table.to_owned(), column.to_owned()),
                TableColumn { unit, cells },
            );
            self
        }
    }

    impl TableColumns for MockTables {
        fn table_column(&self, table: &str, column: &str) -> Result<TableColumn, String> {
            self.0
                .get(&(table.to_owned(), column.to_owned()))
                .cloned()
                .ok_or_else(|| format!("table {table} has no column {column}"))
        }
    }

    fn text(t: &str) -> Option<Cell> {
        Some(Cell::Text(t.to_owned()))
    }

    fn number(v: f64) -> Option<Cell> {
        Some(Cell::Number(v))
    }

    /// Screws: Size (text), d (mm), with a repeated and an empty key.
    fn screws() -> MockTables {
        MockTables::default()
            .with(
                "Screws",
                "Size",
                Unit::TEXT,
                vec![text("M3"), text("M4"), None, text("M5"), text("M4")],
            )
            .with(
                "Screws",
                "d",
                Unit::MM,
                vec![
                    number(3.0),
                    number(4.0),
                    number(4.5),
                    number(5.0),
                    number(4.0),
                ],
            )
    }

    /// Made: the combinations of Color and Length that are made.
    fn made() -> MockTables {
        MockTables::default()
            .with(
                "Made",
                "Color",
                Unit::TEXT,
                vec![text("Black"), text("Black"), text("White"), text("White")],
            )
            .with(
                "Made",
                "Length",
                Unit::MM,
                vec![number(500.0), number(800.0), number(800.0), number(1200.0)],
            )
    }

    fn parameters(specs: &[(&str, &str, Unit)]) -> Parameters {
        let mut p = Parameters::default();
        for (name, expression, unit) in specs {
            p.add(ParamSpec::user(name, expression, *unit), None)
                .unwrap();
        }
        p
    }

    fn table(options: &[&str]) -> Configurations {
        Configurations {
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn column_references_name_a_table_and_a_column() {
        assert_eq!(column_reference("Screws.Size"), Some(("Screws", "Size")));
        assert_eq!(column_reference(" Screws.in "), Some(("Screws", "in")));
        assert_eq!(column_reference("1.5"), None);
        assert_eq!(column_reference("'M5'"), None);
        assert_eq!(column_reference("Screws.d.k"), None);
        assert_eq!(
            list_column(&["Screws.Size".to_owned()]),
            Some(("Screws", "Size"))
        );
        assert_eq!(
            list_column(&["Screws.Size".to_owned(), "'M5'".to_owned()]),
            None
        );
        let mut t = "Screws.Size".to_owned();
        rename_table_in(&mut t, "Screws", "Bolts");
        assert_eq!(t, "Bolts.Size");
        rename_column_in(&mut t, "Bolts", "Size", "Thread");
        assert_eq!(t, "Bolts.Thread");
    }

    #[test]
    fn an_options_values_come_from_a_column() {
        let p = parameters(&[("Size", "'M4'", Unit::TEXT), ("Dia", "4 mm", Unit::MM)]);
        let tables = screws();
        let sizes = column_domain(&p, &tables, "Screws", "Size", "Size").unwrap();
        // Distinct, non-empty, in the table's order.
        assert_eq!(domain_expressions(&sizes), ["'M3'", "'M4'", "'M5'"]);
        assert_eq!(sizes[2].text, "M5");
        let dias = column_domain(&p, &tables, "Screws", "d", "Dia").unwrap();
        assert_eq!(dias.len(), 4);
        assert_eq!(dias[2].value, OptionValue::Number(4.5));
        // A text column is no number option's domain, nor the other way.
        let e = column_domain(&p, &tables, "Screws", "Size", "Dia").unwrap_err();
        assert!(e.contains("is a text column"), "{e}");
        let e = column_domain(&p, &tables, "Screws", "d", "Size").unwrap_err();
        assert!(e.contains("is a number column"), "{e}");
        let e = column_domain(&p, &tables, "Screws", "k", "Dia").unwrap_err();
        assert!(e.contains("no column k"), "{e}");
    }

    #[test]
    fn inch_and_unitless_columns_give_values_in_the_options_unit() {
        let p = parameters(&[("Dia", "4 mm", Unit::MM)]);
        let tables = MockTables::default()
            .with("Imperial", "d", Unit::IN, vec![number(6.35)])
            .with("Plain", "d", Unit::NONE, vec![number(16.0)])
            .with("Angles", "a", Unit::DEG, vec![number(0.5)]);
        let inch = column_domain(&p, &tables, "Imperial", "d", "Dia").unwrap();
        assert_eq!(inch[0].value, OptionValue::Number(6.35));
        assert_eq!(inch[0].expression, "0.25 in");
        let plain = column_domain(&p, &tables, "Plain", "d", "Dia").unwrap();
        assert_eq!(plain[0].value, OptionValue::Number(16.0));
        assert_eq!(plain[0].expression, "16 mm");
        assert!(column_domain(&p, &tables, "Angles", "a", "Dia").is_err());
    }

    #[test]
    fn in_table_allows_only_the_tables_combinations() {
        let mut p = parameters(&[
            ("Color", "'Black'", Unit::TEXT),
            ("Length", "800 mm", Unit::MM),
        ]);
        let tables = made();
        let configurations = table(&["Color", "Length"]);
        let columns = BTreeMap::from([
            ("Color".to_owned(), "Color".to_owned()),
            ("Length".to_owned(), "Length".to_owned()),
        ]);
        let action = InTable {
            table: "Made",
            columns: &columns,
        };
        assert!(
            configurations
                .in_table_problems(action, &p, &tables)
                .is_empty()
        );
        // The first option is narrowed by nothing before it.
        let colors = configurations
            .in_table_allowed(action, "Color", &p, &tables)
            .unwrap()
            .unwrap();
        assert_eq!(domain_expressions(&colors), ["'Black'", "'White'"]);
        // Length given Color = 'Black'.
        let lengths = configurations
            .in_table_allowed(action, "Length", &p, &tables)
            .unwrap()
            .unwrap();
        assert_eq!(domain_expressions(&lengths), ["500 mm", "800 mm"]);
        assert_eq!(
            configurations
                .in_table_violation(action, &p, &tables)
                .unwrap(),
            None
        );
        // Black in 1200 mm is no row: refused with the table's name.
        let id = p.find("Length").unwrap();
        p.set_expression(id, "1200 mm").unwrap();
        let message = configurations
            .in_table_violation(action, &p, &tables)
            .unwrap()
            .unwrap();
        assert_eq!(message, "Color Black and Length 1200 mm is no row of Made");
        // White in 1200 mm is.
        let id = p.find("Color").unwrap();
        p.set_expression(id, "'White'").unwrap();
        assert_eq!(
            configurations
                .in_table_violation(action, &p, &tables)
                .unwrap(),
            None
        );
        let lengths = configurations
            .in_table_allowed(action, "Length", &p, &tables)
            .unwrap()
            .unwrap();
        assert_eq!(domain_expressions(&lengths), ["800 mm", "1200 mm"]);
        // An option the action does not map is not narrowed.
        assert_eq!(
            configurations
                .in_table_allowed(action, "Size", &p, &tables)
                .unwrap(),
            None
        );
    }

    #[test]
    fn in_table_problems_name_what_is_wrong() {
        let p = parameters(&[
            ("Color", "'Black'", Unit::TEXT),
            ("Length", "800 mm", Unit::MM),
        ]);
        let tables = made();
        let configurations = table(&["Color"]);
        let columns = BTreeMap::from([
            ("Color".to_owned(), "Length".to_owned()),
            ("Length".to_owned(), "Length".to_owned()),
        ]);
        let problems = configurations.in_table_problems(
            InTable {
                table: "Made",
                columns: &columns,
            },
            &p,
            &tables,
        );
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("is a number column"), "{problems:?}");
        assert!(
            problems[1].contains("Length is not an option"),
            "{problems:?}"
        );
    }

    #[test]
    fn generating_rows_twice_updates_them() {
        let p = parameters(&[("Size", "'M4'", Unit::TEXT), ("Dia", "4 mm", Unit::MM)]);
        let tables = screws();
        let mut configurations = Configurations::default();
        let first = configurations
            .generate_rows(&p, &tables, "Screws", "Size", "Size", &[])
            .unwrap();
        assert_eq!(first.rows, ["M3", "M4", "M5"]);
        assert_eq!(first.added, ["M3", "M4", "M5"]);
        assert!(first.updated.is_empty());
        assert_eq!(configurations.options, ["Size"]);
        assert_eq!(configurations.rows[2].values["Size"], "'M5'");
        // A row of the same name keeps what it has and is not duplicated.
        configurations.rows[1].designation = Some("ISO 4762 M4".to_owned());
        configurations.rows[1]
            .values
            .insert("Size".to_owned(), "'M4x'".to_owned());
        let second = configurations
            .generate_rows(&p, &tables, "Screws", "Size", "Size", &[])
            .unwrap();
        assert!(second.added.is_empty());
        assert_eq!(second.updated, ["M4"]);
        assert_eq!(configurations.rows.len(), 3);
        assert_eq!(configurations.options, ["Size"]);
        assert_eq!(configurations.rows[1].values["Size"], "'M4'");
        assert_eq!(
            configurations.rows[1].designation.as_deref(),
            Some("ISO 4762 M4")
        );
        // Number keys name their rows without the unit.
        let dias = configurations
            .generate_rows(&p, &tables, "Screws", "d", "Dia", &[])
            .unwrap();
        assert_eq!(dias.rows, ["3", "4", "4.5", "5"]);
        assert_eq!(configurations.row("4.5").unwrap().values["Dia"], "4.5 mm");
        assert_eq!(configurations.options, ["Size", "Dia"]);
        // Refusals.
        assert!(
            configurations
                .generate_rows(&p, &tables, "Screws", "Size", "Dia", &[])
                .is_err()
        );
        assert!(
            configurations
                .generate_rows(&p, &tables, "Screws", "Size", "Nope", &[])
                .is_err()
        );
        let empty = MockTables::default().with("E", "k", Unit::TEXT, vec![None]);
        let e = configurations
            .generate_rows(&p, &empty, "E", "k", "Size", &[])
            .unwrap_err();
        assert_eq!(e, "E.k has no values");
    }

    #[test]
    fn moves_go_to_the_nearest_value_else_the_preferred_else_the_first() {
        let list: Vec<DomainValue> = [300.0, 500.0, 800.0]
            .iter()
            .map(|v| domain_value(&OptionValue::Number(*v), Unit::MM))
            .collect();
        let to = move_target(&list, Some(&OptionValue::Number(650.0)), None).unwrap();
        assert_eq!(to.expression, "500 mm");
        let to = move_target(&list, Some(&OptionValue::Number(700.0)), None).unwrap();
        assert_eq!(to.expression, "800 mm");
        let texts: Vec<DomainValue> = ["M3", "M4"]
            .iter()
            .map(|t| domain_value(&OptionValue::Text((*t).to_owned()), Unit::TEXT))
            .collect();
        let to = move_target(
            &texts,
            Some(&OptionValue::Text("M7".to_owned())),
            Some(&OptionValue::Text("M4".to_owned())),
        )
        .unwrap();
        assert_eq!(to.expression, "'M4'");
        let to = move_target(&texts, Some(&OptionValue::Text("M7".to_owned())), None).unwrap();
        assert_eq!(to.expression, "'M3'");
    }
}
