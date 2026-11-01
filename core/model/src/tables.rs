// SPDX-License-Identifier: MIT
//! Tables of data in the design (mitcad#667, `docs/table-lookups.md`):
//! named tables of constant cells (screw sizes, standard dimensions) that
//! parameter expressions look values up in with `Table.Column` and the
//! lookup functions (`XLOOKUP`, `MATCH`, `INDEX`). A table depends on
//! nothing; tables share the parameters' namespace. The document keeps
//! them beside its configurations (`DocState::tables`), and its parameters
//! evaluate with the same tables ([`Tables::source`]).

use std::fmt;
use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::expr::{self, DEFAULT_DECIMALS, MAX_TEXT_BYTES, Unit, format_value};

/// The most rows a table has.
pub const MAX_ROWS: usize = 10_000;
/// The most columns a table has.
pub const MAX_COLUMNS: usize = 100;

/// A column of a table: its name and unit. Its numbers are in its unit
/// (`5.5` in an `mm` column is 5.5 mm); `Text` columns hold texts, a
/// unitless one (`""`) plain numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableColumn {
    pub name: String,
    #[serde(with = "unit_text")]
    pub unit: Unit,
}

impl TableColumn {
    pub fn new(name: &str, unit: Unit) -> Self {
        Self {
            name: name.to_owned(),
            unit,
        }
    }
}

/// A cell: a number in its column's unit, a text, or empty. In the file a
/// JSON number, a string or `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TableCell {
    Number(f64),
    Text(String),
    Empty,
}

impl TableCell {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// The cell as people read it in a column of `unit`: `8.5 mm`, `M5`,
    /// or `-` when empty.
    pub fn display(&self, unit: Unit) -> String {
        match self {
            Self::Number(n) if unit.is_unitless() => expr::format_number(*n, DEFAULT_DECIMALS),
            Self::Number(n) => format_value(unit.to_canonical(*n), unit, DEFAULT_DECIMALS),
            Self::Text(text) => text.clone(),
            Self::Empty => "-".to_owned(),
        }
    }
}

/// A named table of constant cells (mitcad#667).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// A name as parameters have (case-sensitive, not reserved), which no
    /// parameter has.
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub columns: Vec<TableColumn>,
    /// Each with a cell per column.
    #[serde(default)]
    pub rows: Vec<Vec<TableCell>>,
}

impl Table {
    /// An empty table of these columns.
    pub fn new(name: &str, columns: Vec<TableColumn>) -> Self {
        Self {
            name: name.to_owned(),
            comment: String::new(),
            columns,
            rows: Vec::new(),
        }
    }

    /// The index of the column `name`.
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    /// The column `name`.
    pub fn column(&self, name: &str) -> Option<&TableColumn> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// The row `row` (from 0) as people name it: its number from 1 and its
    /// first cell, `3 (M5)`.
    pub fn row_label(&self, row: usize) -> String {
        let first = self
            .rows
            .get(row)
            .and_then(|cells| cells.first().zip(self.columns.first()))
            .filter(|(cell, _)| !cell.is_empty());
        match first {
            Some((cell, column)) => format!("{} ({})", row + 1, cell.display(column.unit)),
            None => (row + 1).to_string(),
        }
    }

    /// The first thing wrong with the table, if anything is: its name, a
    /// column's name, a row's length, a cell of the wrong kind, a size
    /// over the limits.
    pub fn check(&self) -> Result<(), String> {
        let name = &self.name;
        if !expr::is_valid_name(name) {
            return Err(if expr::is_reserved_name(name) {
                format!("{name} is a unit, function or constant and cannot name a table")
            } else {
                format!(
                    "{name} is not a valid table name: use letters, digits and '_', starting \
                     with a letter"
                )
            });
        }
        if self.columns.len() > MAX_COLUMNS {
            return Err(format!(
                "table {name} has {} columns; a table has at most {MAX_COLUMNS}",
                self.columns.len()
            ));
        }
        if self.rows.len() > MAX_ROWS {
            return Err(format!(
                "table {name} has {} rows; a table has at most {MAX_ROWS}",
                self.rows.len()
            ));
        }
        for (i, column) in self.columns.iter().enumerate() {
            if !is_column_name(&column.name) {
                return Err(format!(
                    "table {name}: {} is not a valid column name: use letters, digits and \
                     '_', starting with a letter",
                    column.name
                ));
            }
            if self.columns[..i].iter().any(|c| c.name == column.name) {
                return Err(format!("table {name} has the column {} twice", column.name));
            }
        }
        for (r, row) in self.rows.iter().enumerate() {
            if row.len() != self.columns.len() {
                return Err(format!(
                    "table {name} row {}: {} cells for {} columns",
                    r + 1,
                    row.len(),
                    self.columns.len()
                ));
            }
            for (cell, column) in row.iter().zip(&self.columns) {
                let at = || format!("table {name} row {}, column {}", r + 1, column.name);
                match cell {
                    TableCell::Number(n) if column.unit.is_text() => {
                        return Err(format!(
                            "{}: the number {} in a Text column",
                            at(),
                            expr::format_number(*n, DEFAULT_DECIMALS)
                        ));
                    }
                    TableCell::Number(n) if !n.is_finite() => {
                        return Err(format!("{}: the number must be finite", at()));
                    }
                    TableCell::Text(text) if !column.unit.is_text() => {
                        return Err(format!(
                            "{}: the text “{text}” in a column of numbers ({})",
                            at(),
                            unit_name(column.unit)
                        ));
                    }
                    TableCell::Text(text) if text.len() > MAX_TEXT_BYTES => {
                        return Err(format!(
                            "{}: a text is at most {MAX_TEXT_BYTES} bytes",
                            at()
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

/// A unit as the file writes it: `mm`, `Text`, `""` for unitless.
fn unit_name(unit: Unit) -> String {
    if unit.is_unitless() && !unit.is_text() {
        "unitless".to_owned()
    } else {
        unit.to_string()
    }
}

/// Whether `name` can name a column: the characters of parameter names
/// (a letter or `_`, then letters, digits and `_`). Reserved words (`in`,
/// `Max`) may: a column is only ever written after `Table.`.
pub fn is_column_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// The design's tables (mitcad#667), in the order they were made. Cheap to
/// clone: the tables are shared.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tables {
    items: Vec<Arc<Table>>,
}

impl Tables {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The tables in the order they were made.
    pub fn iter(&self) -> impl Iterator<Item = &Table> {
        self.items.iter().map(|t| &**t)
    }

    /// The table `name`.
    pub fn get(&self, name: &str) -> Option<&Table> {
        self.iter().find(|t| t.name == name)
    }

    /// Whether a table of this name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Tables from a list, each checked, their names unique.
    pub fn from_list(tables: Vec<Table>) -> Result<Self, String> {
        let mut out = Self::default();
        for table in tables {
            table.check()?;
            if out.contains(&table.name) {
                return Err(format!("table {} is listed more than once", table.name));
            }
            out.items.push(Arc::new(table));
        }
        Ok(out)
    }

    /// These tables with `table` in place of the one of its name, or added
    /// after the others.
    pub fn with(&self, table: Table) -> Self {
        let mut out = self.clone();
        match out.items.iter().position(|t| t.name == table.name) {
            Some(i) => out.items[i] = Arc::new(table),
            None => out.items.push(Arc::new(table)),
        }
        out
    }

    /// These tables without the table `name`.
    pub fn without(&self, name: &str) -> Self {
        Self {
            items: self
                .items
                .iter()
                .filter(|t| t.name != name)
                .cloned()
                .collect(),
        }
    }

    /// These tables with the table `old` named `new`, where it was.
    pub fn renamed(&self, old: &str, new: &str) -> Self {
        let mut out = self.clone();
        for table in &mut out.items {
            if table.name == old {
                Arc::make_mut(table).name = new.to_owned();
            }
        }
        out
    }

    /// What the parameters evaluate with: these tables, or None when there
    /// are none.
    pub fn source(&self) -> Option<Arc<dyn expr::TableSource>> {
        (!self.is_empty()).then(|| Arc::new(self.clone()) as Arc<dyn expr::TableSource>)
    }

    /// `name`, or `name_1`, `name_2`, ... the first that `taken` does not
    /// refuse and that can name a table.
    pub fn free_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
        std::iter::once(name.to_owned())
            .chain((1..).map(|k| format!("{name}_{k}")))
            .find(|n| expr::is_valid_name(n) && !taken(n))
            .expect("a free name exists")
    }
}

impl fmt::Display for Tables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.iter().map(|t| t.name.as_str()).collect();
        f.write_str(&names.join(", "))
    }
}

impl Serialize for Tables {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Tables {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_list(Vec::<Table>::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

// The lookups' view of the tables (`Lookup::column`, mitcad#802's
// interface): a column's unit and cells.
impl expr::Lookup for Tables {
    fn lookup(&self, _name: &str) -> Option<expr::Quantity> {
        None
    }

    fn column(
        &self,
        table: &str,
        column: &str,
    ) -> Result<expr::TableColumn<'_>, expr::ColumnError> {
        let table = self.get(table).ok_or(expr::ColumnError::NoTable)?;
        let index = table
            .column_index(column)
            .ok_or(expr::ColumnError::NoColumn)?;
        let cells = |i: usize| {
            table
                .rows
                .iter()
                .map(|row| match &row[i] {
                    TableCell::Number(n) => expr::TableCell::Number(*n),
                    TableCell::Text(text) => expr::TableCell::Text(text),
                    TableCell::Empty => expr::TableCell::Empty,
                })
                .collect::<Vec<_>>()
        };
        // The first column's cells name the rows in messages.
        Ok(expr::TableColumn::new(table.columns[index].unit, cells(index)).with_labels(cells(0)))
    }
}

impl expr::TableSource for Tables {
    fn has_table(&self, name: &str) -> bool {
        self.contains(name)
    }
}

/// A column's unit as text in the file: `mm`, `Text`, `""`.
mod unit_text {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::expr::Unit;

    pub(super) fn serialize<S: Serializer>(unit: &Unit, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(unit)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Unit, D::Error> {
        let text = String::deserialize(deserializer)?;
        Unit::parse(&text).map_err(|e| D::Error::custom(format!("unit “{text}”: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn screws() -> Table {
        serde_json::from_value(json!({
            "name": "Screws",
            "comment": "ISO 4762 socket head cap screws",
            "columns": [
                {"name": "Size", "unit": "Text"},
                {"name": "d", "unit": "mm"},
                {"name": "dk", "unit": "mm"},
                {"name": "k", "unit": "mm"},
                {"name": "Pitch", "unit": "mm"}],
            "rows": [
                ["M3", 3, 5.5, 3, 0.5],
                ["M4", 4, 7, 4, 0.7],
                ["M5", 5, 8.5, 5, 0.8]]
        }))
        .unwrap()
    }

    #[test]
    fn tables_read_and_write_the_file_form() {
        let table = screws();
        assert_eq!(table.check(), Ok(()));
        assert_eq!(table.columns[0].unit, Unit::TEXT);
        assert_eq!(table.columns[1].unit, Unit::MM);
        assert_eq!(table.rows[2][2], TableCell::Number(8.5));
        assert_eq!(table.rows[0][0], TableCell::Text("M3".to_owned()));
        let value = serde_json::to_value(&table).unwrap();
        assert_eq!(value["columns"][0], json!({"name": "Size", "unit": "Text"}));
        assert_eq!(value["rows"][1], json!(["M4", 4.0, 7.0, 4.0, 0.7]));
        let back: Table = serde_json::from_value(value).unwrap();
        assert_eq!(back, table);
        // Empty cells are null; a unitless column's unit is "".
        let t: Table = serde_json::from_value(json!({
            "name": "T", "columns": [{"name": "n", "unit": ""}], "rows": [[null], [2]]}))
        .unwrap();
        assert_eq!(t.rows[0][0], TableCell::Empty);
        assert!(t.columns[0].unit.is_unitless() && !t.columns[0].unit.is_text());
        assert_eq!(serde_json::to_value(&t).unwrap()["columns"][0]["unit"], "");
        assert_eq!(t.row_label(0), "1");
        assert_eq!(table.row_label(2), "3 (M5)");
    }

    #[test]
    fn wrong_tables_are_refused() {
        let check = |f: &dyn Fn(&mut Table)| {
            let mut t = screws();
            f(&mut t);
            t.check().unwrap_err()
        };
        assert_eq!(
            check(&|t| t.rows[1][2] = TableCell::Text("7".to_owned())),
            "table Screws row 2, column dk: the text “7” in a column of numbers (mm)"
        );
        assert_eq!(
            check(&|t| t.rows[0][0] = TableCell::Number(3.0)),
            "table Screws row 1, column Size: the number 3 in a Text column"
        );
        assert_eq!(
            check(&|t| {
                t.rows[0].pop();
            }),
            "table Screws row 1: 4 cells for 5 columns"
        );
        assert_eq!(
            check(&|t| t.columns[2].name = "d".to_owned()),
            "table Screws has the column d twice"
        );
        assert!(check(&|t| t.name = "mm".to_owned()).contains("cannot name a table"));
        assert!(check(&|t| t.name = "2nd".to_owned()).contains("not a valid table name"));
        assert!(check(&|t| t.columns[1].name = "a b".to_owned()).contains("column name"));
        assert!(check(&|t| t.rows[0][1] = TableCell::Number(f64::INFINITY)).contains("finite"));
        // Reserved words name columns.
        let mut t = screws();
        t.columns[1].name = "in".to_owned();
        t.columns[2].name = "Max".to_owned();
        assert_eq!(t.check(), Ok(()));
        // Names are unique in a list of tables.
        let error = Tables::from_list(vec![screws(), screws()]).unwrap_err();
        assert_eq!(error, "table Screws is listed more than once");
        let error = serde_json::from_value::<Tables>(json!([{
            "name": "T", "columns": [{"name": "n", "unit": "furlong"}]}]))
        .unwrap_err();
        assert!(error.to_string().contains("unit “furlong”"), "{error}");
    }

    #[test]
    fn tables_are_replaced_renamed_and_removed_by_name() {
        let tables = Tables::from_list(vec![screws()]).unwrap();
        let mut nuts = Table::new("Nuts", vec![TableColumn::new("m", Unit::MM)]);
        let tables = tables.with(nuts.clone());
        assert_eq!(tables.to_string(), "Screws, Nuts");
        nuts.rows.push(vec![TableCell::Number(4.0)]);
        let tables = tables.with(nuts);
        assert_eq!(tables.get("Nuts").unwrap().rows.len(), 1);
        let renamed = tables.renamed("Screws", "Bolts");
        assert_eq!(renamed.to_string(), "Bolts, Nuts");
        assert_eq!(renamed.without("Nuts").to_string(), "Bolts");
        assert_eq!(
            Tables::free_name("Screws", &|n| tables.contains(n)),
            "Screws_1"
        );
        assert!(Tables::default().source().is_none());
        assert!(tables.source().is_some_and(|s| s.has_table("Nuts")));
    }
}
