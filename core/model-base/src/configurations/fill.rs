// SPDX-License-Identifier: MIT
//! Filling a closed table's rows (mitcad#1044; `docs/configurations.md`,
//! "Filling a closed table", decision 12): the classic design table, a row
//! per size and a column per parameter the row sets, filled from a table
//! of the design (Generate Rows from Table… with its other columns,
//! [`Configurations::generate_rows`]) or from a file (the rows CSV of the
//! Rows tab, which the model's `document/configuration_rows_csv.rs` reads
//! into a [`RowSource`]).
//!
//! - The **key column** sets the key option: each row is named by its key
//!   (or by the file's `Name`), and a parameter column that is the key
//!   moves to the options with its rows' values.
//! - Every **other column** sets the parameter of its name: an option's
//!   value when the parameter is an option of the table, else a parameter
//!   column of the closed table (the table becomes closed). A parameter the
//!   design lacks is made with the column's unit, starting at the value of
//!   the row the options choose now ([`GeneratedRows::created`]).
//! - A row of the same name is updated, not duplicated: the columns the
//!   source has are set (an empty cell takes the value out), the rest is
//!   kept. Rows the source lacks are kept.
//! - In a closed table, a row without a value of an option takes the
//!   option's current value, as Add Option… gives every row
//!   (decision 12).
//!
//! The tables are read only through [`TableColumns`] (`from_tables.rs`).

use std::collections::BTreeMap;

use crate::expr::{Expr, ParamKind, Unit, is_valid_name, text_literal, value_to_expression};
use crate::parameters::Parameters;

use super::from_tables::{
    GeneratedRows, Key, TableColumns, column_keys, current_option_value, same_option_value,
};
use super::{ConfigurationRow, Configurations};

/// A parameter a fill makes: a column (or the key) whose parameter the
/// design lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewParameter {
    pub name: String,
    /// The column's unit (`Text` for a text column, none for a unitless
    /// one).
    pub unit: Unit,
    /// Its expression: the value of the row the options choose now, else
    /// of the first row.
    pub expression: String,
}

/// What a line of a file sets besides its values. A field is None when the
/// file has no such column; an empty text clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowFields {
    /// The row's name; None or empty: the row is named by its key.
    pub name: Option<String>,
    pub designation: Option<String>,
    pub part_number: Option<String>,
    pub supplier_url: Option<String>,
}

/// Where a fill's rows come from: a table of the design or a file read as
/// one.
pub struct RowSource<'a> {
    pub tables: &'a dyn TableColumns,
    /// The table's name, as [`TableColumns`] knows it and messages name its
    /// columns (`Screws.d`).
    pub table: &'a str,
    /// The key column and the option it sets.
    pub key: &'a str,
    pub option: &'a str,
    /// The other columns, each with the parameter it sets, in the source's
    /// order.
    pub columns: Vec<(String, String)>,
    /// A row per line of the source (a file: a name twice is refused), else
    /// one per distinct key, from its first line (a table).
    pub per_line: bool,
    /// Per line: what the file sets besides the values; empty for a table.
    pub fields: Vec<RowFields>,
    /// Per line: cells of parameter columns taken as expressions as they
    /// are (a formula of a file's cell), by column.
    pub expressions: Vec<BTreeMap<String, String>>,
    /// The key option's parameter is made when the design lacks it (a
    /// file); else it must exist.
    pub create_key: bool,
}

/// The unit of a user parameter; None when the design has no parameter of
/// that name, refused for a parameter of another kind.
fn user_unit(parameters: &Parameters, name: &str) -> Result<Option<Unit>, String> {
    let Some(p) = parameters.find(name).and_then(|id| parameters.get(id)) else {
        return Ok(None);
    };
    if p.kind() != ParamKind::User {
        return Err(format!(
            "{name} is not a user parameter (it is a {} parameter)",
            p.kind()
        ));
    }
    Ok(Some(p.unit()))
}

/// The unit of a parameter a column makes; refused for a column whose name
/// no parameter can have.
fn new_unit(source: &RowSource<'_>, column: &str, parameter: &str) -> Result<Unit, String> {
    if !is_valid_name(parameter) {
        return Err(format!(
            "{}.{column}: {parameter} cannot name a parameter (a reserved word or no name): \
             rename the column",
            source.table
        ));
    }
    Ok(source.tables.table_column(source.table, column)?.unit)
}

/// An option's current value as the constant a row keeps: its expression
/// when that is a constant, else its value; a parameter the fill makes,
/// its expression.
fn current_constant(
    parameters: &Parameters,
    name: &str,
    created: &[NewParameter],
) -> Option<String> {
    if let Some(new) = created.iter().find(|n| n.name == name) {
        return Some(new.expression.clone());
    }
    let p = parameters.get(parameters.find(name)?)?;
    if Expr::parse(p.expression()).is_ok_and(|e| e.is_constant()) {
        return Some(p.expression().to_owned());
    }
    Some(match p.text() {
        Some(text) => text_literal(text),
        None => value_to_expression(p.value(), p.unit()),
    })
}

/// Sets a row's values (None: taken out) and the fields a file gives;
/// whether anything changed.
fn fill_row(
    row: &mut ConfigurationRow,
    values: &[(&str, Option<&str>)],
    fields: Option<&RowFields>,
) -> bool {
    let mut changed = false;
    for &(parameter, value) in values {
        match value {
            Some(v) if row.values.get(parameter).map(String::as_str) != Some(v) => {
                row.values.insert(parameter.to_owned(), v.to_owned());
                changed = true;
            }
            Some(_) => {}
            None => changed |= row.values.remove(parameter).is_some(),
        }
    }
    if let Some(f) = fields {
        for (field, given) in [
            (&mut row.designation, &f.designation),
            (&mut row.part_number, &f.part_number),
            (&mut row.supplier_url, &f.supplier_url),
        ] {
            if let Some(text) = given {
                let new = Some(text.trim())
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned);
                if *field != new {
                    *field = new;
                    changed = true;
                }
            }
        }
    }
    changed
}

/// A parameter column of the source with its value per line.
struct Filled {
    parameter: String,
    values: Vec<Option<String>>,
}

impl Configurations {
    /// Fills the table's rows from `source` (module comment). The caller
    /// adds [`GeneratedRows::created`] to the parameters with the table and
    /// checks the table's problems then. Refused when the key option is no
    /// user parameter (or is missing and the source does not make it), when
    /// a column cannot give its parameter values or names no parameter,
    /// when two columns set one parameter, when the key column has no
    /// values, and when a file has a row twice.
    pub fn fill_rows(
        &mut self,
        parameters: &Parameters,
        source: &RowSource<'_>,
    ) -> Result<GeneratedRows, String> {
        let option = source.option;
        let mut created: Vec<NewParameter> = Vec::new();
        let (key_unit, key_created) = match user_unit(parameters, option)? {
            Some(unit) => (unit, false),
            None if source.create_key => (new_unit(source, source.key, option)?, true),
            None => return Err(format!("parameter {option} does not exist")),
        };
        let keys = column_keys(
            source.tables,
            source.table,
            source.key,
            &format!("option {option}"),
            key_unit,
        )?;
        // The lines that make rows: every line with a key (a file), else the
        // first line of each key (a table).
        let mut lines: Vec<(usize, &Key, String)> = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            let Some(key) = key else {
                continue;
            };
            if !source.per_line
                && lines
                    .iter()
                    .any(|(_, seen, _)| same_option_value(&seen.value.value, &key.value.value))
            {
                continue;
            }
            let name = source
                .fields
                .get(i)
                .and_then(|f| f.name.as_deref())
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map_or_else(|| key.name.clone(), str::to_owned);
            if lines.iter().any(|(_, _, seen)| *seen == name) {
                if source.per_line {
                    return Err(format!("the row {name} is in the file twice"));
                }
                continue;
            }
            lines.push((i, key, name));
        }
        let Some(&(first, first_key, _)) = lines.first() else {
            return Err(format!("{}.{} has no values", source.table, source.key));
        };
        if key_created {
            created.push(NewParameter {
                name: option.to_owned(),
                unit: key_unit,
                expression: first_key.value.expression.clone(),
            });
        }
        // The line of the row the options choose now: the created
        // parameters start at its values.
        let chosen = current_option_value(parameters, option)
            .and_then(|now| {
                lines
                    .iter()
                    .find(|(_, key, _)| same_option_value(&key.value.value, &now))
                    .map(|(i, _, _)| *i)
            })
            .unwrap_or(first);

        let mut filled: Vec<Filled> = Vec::new();
        for (column, parameter) in &source.columns {
            if column == source.key || parameter == option {
                continue;
            }
            if filled.iter().any(|f| f.parameter == *parameter) {
                return Err(format!("two columns set the parameter {parameter}"));
            }
            let (unit, made) = match user_unit(parameters, parameter)? {
                Some(unit) => (unit, false),
                None => (new_unit(source, column, parameter)?, true),
            };
            let cells = column_keys(
                source.tables,
                source.table,
                column,
                &format!("parameter {parameter}"),
                unit,
            )?;
            let mut values: Vec<Option<String>> = cells
                .into_iter()
                .map(|cell| cell.map(|k| k.value.expression))
                .collect();
            // A file's formulas, as they are.
            for (i, given) in source.expressions.iter().enumerate() {
                if let (Some(expression), Some(value)) = (given.get(column), values.get_mut(i)) {
                    *value = Some(expression.clone());
                }
            }
            if made {
                let expression = values
                    .get(chosen)
                    .cloned()
                    .flatten()
                    .or_else(|| {
                        lines
                            .iter()
                            .find_map(|(i, _, _)| values.get(*i).cloned().flatten())
                    })
                    .unwrap_or_else(|| {
                        if unit.is_text() {
                            "''".to_owned()
                        } else {
                            value_to_expression(0.0, unit)
                        }
                    });
                created.push(NewParameter {
                    name: parameter.clone(),
                    unit,
                    expression,
                });
            }
            filled.push(Filled {
                parameter: parameter.clone(),
                values,
            });
        }

        // The key is an option (a parameter column moves to the options);
        // the other columns are options' values or parameter columns.
        self.parameters.retain(|p| p != option);
        if !self.is_option(option) {
            self.options.push(option.to_owned());
        }
        for f in &filled {
            if !self.is_option(&f.parameter) && !self.parameters.contains(&f.parameter) {
                self.parameters.push(f.parameter.clone());
            }
        }
        if filled.iter().any(|f| !self.is_option(&f.parameter)) {
            self.closed = true;
        }

        let mut out = GeneratedRows::default();
        for (i, key, name) in &lines {
            let mut values: Vec<(&str, Option<&str>)> =
                vec![(option, Some(key.value.expression.as_str()))];
            for f in &filled {
                values.push((
                    f.parameter.as_str(),
                    f.values.get(*i).and_then(Option::as_deref),
                ));
            }
            let fields = source.fields.get(*i);
            out.rows.push(name.clone());
            match self.rows.iter_mut().find(|row| row.name == *name) {
                Some(row) => {
                    if fill_row(row, &values, fields) {
                        out.updated.push(name.clone());
                    }
                }
                None => {
                    let mut row = ConfigurationRow {
                        name: name.clone(),
                        ..Default::default()
                    };
                    fill_row(&mut row, &values, fields);
                    self.rows.push(row);
                    out.added.push(name.clone());
                }
            }
        }
        // A closed table's rows have a value of every option: a row without
        // one takes the option's current value.
        if self.closed {
            for option in self.options.clone() {
                let Some(constant) = current_constant(parameters, &option, &created) else {
                    continue;
                };
                for row in &mut self.rows {
                    if row.values.get(&option).is_none_or(|v| v.trim().is_empty()) {
                        row.values.insert(option.clone(), constant.clone());
                        if !out.added.contains(&row.name) && !out.updated.contains(&row.name) {
                            out.updated.push(row.name.clone());
                        }
                    }
                }
            }
        }
        out.created = created;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configurations::{Cell, TableColumn};
    use crate::expr::ParamSpec;

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

    /// Screws of a size table: size (text), d and k (mm), with M4 twice.
    fn screws() -> MockTables {
        MockTables::default()
            .with(
                "Screws",
                "size",
                Unit::TEXT,
                vec![text("M3"), text("M4"), text("M5"), text("M4")],
            )
            .with(
                "Screws",
                "d",
                Unit::MM,
                vec![number(3.0), number(4.0), number(5.0), number(4.5)],
            )
            .with(
                "Screws",
                "k",
                Unit::MM,
                vec![number(1.86), number(2.48), None, number(9.0)],
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

    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn generated_rows_set_the_other_columns_and_make_their_parameters() {
        // size exists and is M4; d exists; k does not.
        let p = parameters(&[("size", "'M4'", Unit::TEXT), ("d", "6 mm", Unit::MM)]);
        let tables = screws();
        let mut table = Configurations::default();
        let done = table
            .generate_rows(
                &p,
                &tables,
                "Screws",
                "size",
                "size",
                &columns(&["size", "d", "k"]),
            )
            .unwrap();
        assert_eq!(done.rows, ["M3", "M4", "M5"]);
        assert_eq!(done.added, ["M3", "M4", "M5"]);
        // k is made in the column's unit, at the value of the row the
        // options choose now (M4).
        assert_eq!(
            done.created,
            [NewParameter {
                name: "k".to_owned(),
                unit: Unit::MM,
                expression: "2.48 mm".to_owned(),
            }]
        );
        assert!(table.closed);
        assert_eq!(table.options, ["size"]);
        assert_eq!(table.parameters, ["d", "k"]);
        // The first line of a key sets its row; an empty cell sets nothing.
        let m4 = table.row("M4").unwrap();
        assert_eq!(m4.values["d"], "4 mm");
        assert_eq!(m4.values["k"], "2.48 mm");
        let m5 = table.row("M5").unwrap();
        assert_eq!(m5.values["size"], "'M5'");
        assert!(!m5.values.contains_key("k"));

        // Again after a cell changed: the row of the same name is updated,
        // its designation kept.
        table.rows[0].designation = Some("ISO 7380 M3".to_owned());
        let tables = tables.with(
            "Screws",
            "d",
            Unit::MM,
            vec![number(3.1), number(4.0), number(5.0), number(4.5)],
        );
        let again = table
            .generate_rows(&p, &tables, "Screws", "size", "size", &columns(&["d", "k"]))
            .unwrap();
        assert!(again.added.is_empty());
        assert_eq!(again.updated, ["M3"]);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.row("M3").unwrap().values["d"], "3.1 mm");
        assert_eq!(
            table.row("M3").unwrap().designation.as_deref(),
            Some("ISO 7380 M3")
        );
    }

    #[test]
    fn a_parameter_column_as_the_option_moves_to_the_options() {
        let p = parameters(&[
            ("size", "'M3'", Unit::TEXT),
            ("d", "3 mm", Unit::MM),
            ("k", "2 mm", Unit::MM),
        ]);
        // A closed table whose rows set size as a parameter column.
        let mut table = Configurations {
            closed: true,
            options: Vec::new(),
            parameters: columns(&["size", "d"]),
            rows: vec![ConfigurationRow {
                name: "Small".to_owned(),
                values: BTreeMap::from([
                    ("size".to_owned(), "'M3'".to_owned()),
                    ("d".to_owned(), "3 mm".to_owned()),
                ]),
                ..Default::default()
            }],
            ..Default::default()
        };
        table
            .generate_rows(&p, &screws(), "Screws", "size", "size", &columns(&["k"]))
            .unwrap();
        assert_eq!(table.options, ["size"]);
        assert_eq!(table.parameters, ["d", "k"]);
        // The kept row keeps its value, now an option's.
        assert_eq!(table.row("Small").unwrap().values["size"], "'M3'");
        assert!(
            table.problems(&p).iter().any(|e| e.contains("another row")),
            "{:?}",
            table.problems(&p)
        );
    }

    #[test]
    fn a_closed_table_gives_a_new_option_the_current_value_in_every_row() {
        let p = parameters(&[
            ("size", "'M3'", Unit::TEXT),
            ("Color", "'Black'", Unit::TEXT),
            ("d", "3 mm", Unit::MM),
        ]);
        let mut table = Configurations {
            options: columns(&["Color"]),
            ..Default::default()
        };
        let done = table
            .generate_rows(&p, &screws(), "Screws", "size", "size", &columns(&["d"]))
            .unwrap();
        assert!(table.closed);
        assert_eq!(table.options, ["Color", "size"]);
        for row in &table.rows {
            assert_eq!(row.values["Color"], "'Black'", "{}", row.name);
        }
        assert!(done.created.is_empty());
        assert!(table.problems(&p).is_empty(), "{:?}", table.problems(&p));
    }

    #[test]
    fn a_file_makes_a_row_per_line_with_its_names_fields_and_formulas() {
        let p = parameters(&[("Length", "10 mm", Unit::MM)]);
        let tables = screws();
        let mut table = Configurations::default();
        let fields = vec![
            RowFields {
                name: Some("Small".to_owned()),
                designation: Some("ISO 7380 M3".to_owned()),
                ..Default::default()
            },
            RowFields::default(),
            RowFields::default(),
            RowFields {
                name: Some("Long M4".to_owned()),
                ..Default::default()
            },
        ];
        let mut expressions = vec![BTreeMap::new(); 4];
        expressions[2].insert("k".to_owned(), "Length / 4".to_owned());
        let source = RowSource {
            tables: &tables,
            table: "Screws",
            key: "size",
            option: "size",
            columns: vec![
                ("d".to_owned(), "d".to_owned()),
                ("k".to_owned(), "k".to_owned()),
            ],
            per_line: true,
            fields,
            expressions,
            create_key: true,
        };
        let done = table.fill_rows(&p, &source).unwrap();
        assert_eq!(done.rows, ["Small", "M4", "M5", "Long M4"]);
        let made: Vec<&str> = done.created.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(made, ["size", "d", "k"]);
        assert_eq!(done.created[0].expression, "'M3'");
        assert_eq!(done.created[0].unit, Unit::TEXT);
        let small = table.row("Small").unwrap();
        assert_eq!(small.values["size"], "'M3'");
        assert_eq!(small.designation.as_deref(), Some("ISO 7380 M3"));
        assert_eq!(table.row("M5").unwrap().values["k"], "Length / 4");
        assert_eq!(table.row("Long M4").unwrap().values["d"], "4.5 mm");
        // A name twice in a file is refused.
        let mut twice = Configurations::default();
        let source = RowSource {
            fields: Vec::new(),
            expressions: Vec::new(),
            ..source
        };
        let e = twice.fill_rows(&p, &source).unwrap_err();
        assert_eq!(e, "the row M4 is in the file twice");
    }

    #[test]
    fn refusals_name_the_column() {
        let p = parameters(&[("size", "'M3'", Unit::TEXT), ("k", "'x'", Unit::TEXT)]);
        let mut table = Configurations::default();
        let e = table
            .generate_rows(&p, &screws(), "Screws", "size", "size", &columns(&["k"]))
            .unwrap_err();
        assert_eq!(e, "Screws.k is a number column, and parameter k is a text");
        let tables = screws().with("Screws", "in", Unit::MM, vec![number(1.0)]);
        let e = table
            .generate_rows(&p, &tables, "Screws", "size", "size", &columns(&["in"]))
            .unwrap_err();
        assert!(e.contains("in cannot name a parameter"), "{e}");
        let e = table
            .generate_rows(&p, &screws(), "Screws", "size", "nope", &[])
            .unwrap_err();
        assert_eq!(e, "parameter nope does not exist");
    }
}
