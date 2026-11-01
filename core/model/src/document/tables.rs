// SPDX-License-Identifier: MIT
//! Tables of data in the design (mitcad#667, `tables.rs`,
//! `docs/table-lookups.md`): creating or replacing a table, renaming it or
//! one of its columns, deleting it. The parameters evaluate with the
//! design's tables; an edit re-evaluates the parameters that use the
//! table, and their dependents, in the same step, and is refused when one
//! of them would fail. Renames rewrite the expressions and limits that use
//! the table (and the configuration rows' values); a table in use is not
//! deleted.

use std::collections::{BTreeMap, HashMap};

use super::configuration_tables::move_off_removed_values;
use super::{DocState, Document, ModelError, invalid};
use crate::configurations::ConfigurationMove;
use crate::expr::{Expr, replace_table_references};
use crate::kernel::Kernel;
use crate::parameters::{ChangeSet, ParamId};
use crate::tables::{Table, Tables, is_column_name};

impl DocState {
    /// The design's tables.
    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    /// Makes `tables` the design's tables, which the parameters then
    /// evaluate with, re-evaluating the parameters that use one of the
    /// tables `changed` in the same step; on failure nothing changes.
    /// Returns the parameters whose values changed.
    pub(crate) fn put_tables(
        &mut self,
        tables: Tables,
        changed: &[&str],
    ) -> Result<ChangeSet, ModelError> {
        let changed = self.parameters.set_tables(tables.source(), changed)?;
        self.tables = tables;
        Ok(changed)
    }

    /// The names of the parameters whose expressions or limits use the
    /// table `name`, in creation order.
    pub fn table_users(&self, name: &str) -> Vec<String> {
        self.parameters
            .table_users(name)
            .into_iter()
            .map(|id| self.parameters.name(id))
            .collect()
    }

    /// The configuration rows whose values use the table `name`.
    fn configuration_table_users(&self, name: &str) -> Vec<String> {
        self.configurations
            .rows
            .iter()
            .filter(|row| {
                row.values.values().any(|expression| {
                    Expr::parse(expression)
                        .is_ok_and(|e| e.table_references().iter().any(|(t, _)| *t == name))
                })
            })
            .map(|row| format!("configuration {}", row.name))
            .collect()
    }

    /// Creates the table of `table`'s name, or replaces it, and
    /// re-evaluates the parameters that use it. Refused with the table's
    /// first problem, a column a parameter still uses, or the first
    /// parameter that would fail. Returns the parameters whose values
    /// changed.
    pub(crate) fn set_table(&mut self, table: Table) -> Result<ChangeSet, ModelError> {
        self.set_table_moving(table).map(|(changed, _)| changed)
    }

    /// [`DocState::set_table`], which also moves the options whose allowed
    /// values are a column of the table off values the edit removed
    /// (mitcad#806); returns the moves too.
    pub(crate) fn set_table_moving(
        &mut self,
        table: Table,
    ) -> Result<(ChangeSet, Vec<ConfigurationMove>), ModelError> {
        table.check().map_err(invalid)?;
        let name = table.name.clone();
        if self.parameters.find(&name).is_some() {
            return Err(invalid(format!("{name} is the name of a parameter")));
        }
        // A column a parameter uses stays; its unit or kind may change, which
        // the re-evaluation checks.
        let mut gone: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in self.parameters.table_users(&name) {
            let p = self.parameters.get(id).expect("users exist");
            for (t, column) in p.table_references() {
                if t == name && table.column(column).is_none() {
                    let users = gone.entry(column.to_owned()).or_default();
                    if !users.iter().any(|u| u == p.name()) {
                        users.push(p.name().to_owned());
                    }
                }
            }
        }
        if let Some((column, users)) = gone.into_iter().next() {
            return Err(invalid(format!(
                "column {name}.{column} is used by {}",
                users.join(", ")
            )));
        }
        let tables = self.tables.with(table);
        let (mut changed, moves) = move_off_removed_values(self, &tables, &name)?;
        changed.extend(self.put_tables(tables, &[name.as_str()])?);
        Ok((changed, moves))
    }

    /// Renames the table `old`; the expressions, limits and configuration
    /// values that use it follow. Returns the parameters rewritten.
    pub(crate) fn rename_table(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<Vec<ParamId>, ModelError> {
        self.require_table(old)?;
        if old == new {
            return Ok(Vec::new());
        }
        if self.tables.contains(new) {
            return Err(invalid(format!("table {new} already exists")));
        }
        if self.parameters.find(new).is_some() {
            return Err(invalid(format!("{new} is the name of a parameter")));
        }
        let tables = self.tables.renamed(old, new);
        tables.get(new).expect("renamed").check().map_err(invalid)?;
        let rewritten = self
            .parameters
            .rename_table_references(tables.source(), old, None, new)?;
        self.tables = tables;
        self.rename_in_table_expressions(old, None, new);
        Ok(rewritten)
    }

    /// Renames the column `old` of the table `table`; the expressions,
    /// limits and configuration values that use it follow. Returns the
    /// parameters rewritten.
    pub(crate) fn rename_table_column(
        &mut self,
        table: &str,
        old: &str,
        new: &str,
    ) -> Result<Vec<ParamId>, ModelError> {
        let mut renamed = self.require_table(table)?.clone();
        let i = renamed
            .column_index(old)
            .ok_or_else(|| invalid(format!("table {table} has no column {old}")))?;
        if old == new {
            return Ok(Vec::new());
        }
        if !is_column_name(new) {
            return Err(invalid(format!(
                "{new} is not a valid column name: use letters, digits and '_', starting with \
                 a letter"
            )));
        }
        if renamed.column(new).is_some() {
            return Err(invalid(format!("table {table} already has a column {new}")));
        }
        new.clone_into(&mut renamed.columns[i].name);
        let tables = self.tables.with(renamed);
        let rewritten =
            self.parameters
                .rename_table_references(tables.source(), table, Some(old), new)?;
        self.tables = tables;
        self.rename_in_table_expressions(table, Some(old), new);
        Ok(rewritten)
    }

    /// Deletes the table `name`, refused while a parameter or a
    /// configuration row uses it.
    pub(crate) fn delete_table(&mut self, name: &str) -> Result<(), ModelError> {
        self.require_table(name)?;
        let mut users = self.table_users(name);
        users.extend(self.configuration_table_users(name));
        // Rules with an `in_table` action or a list of its column
        // (mitcad#806).
        users.extend(self.configurations.table_users(name));
        if !users.is_empty() {
            return Err(invalid(format!(
                "table {name} is used by {}",
                users.join(", ")
            )));
        }
        let tables = self.tables.without(name);
        self.put_tables(tables, &[])?;
        Ok(())
    }

    fn require_table(&self, name: &str) -> Result<&Table, ModelError> {
        self.tables.get(name).ok_or_else(|| {
            if self.tables.is_empty() {
                invalid(format!("the design has no table {name}"))
            } else {
                invalid(format!(
                    "the design has no table {name} (its tables: {})",
                    self.tables
                ))
            }
        })
    }

    /// Bindings and configuration expressions with the table `table`
    /// (`column` None) or its column renamed.
    fn rename_in_table_expressions(&mut self, table: &str, column: Option<&str>, new: &str) {
        self.edit_binding_texts(|expression| {
            if let Ok(expr) = Expr::parse(expression) {
                *expression = replace_table_references(expression, &expr, table, column, new);
            }
        });
        // The rules' `in_table` actions and column lists (mitcad#806).
        match column {
            None => self.configurations.rename_table(table, new),
            Some(old) => self.configurations.rename_table_column(table, old, new),
        }
        for row in &mut self.configurations.rows {
            for expression in row.values.values_mut() {
                if let Ok(expr) = Expr::parse(expression) {
                    *expression = replace_table_references(expression, &expr, table, column, new);
                }
            }
        }
    }

    /// Brings the tables `names` of `source` (another design) that this
    /// one lacks, for parameters copied from it (mitcad#667): the same
    /// table here is used as it is, a different one of the same name (or a
    /// parameter of that name) makes the copy take a free name. Returns the
    /// renamed tables, for the copied expressions to follow.
    pub(crate) fn bring_tables<'a>(
        &mut self,
        source: &DocState,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<HashMap<String, String>, ModelError> {
        let mut renamed = HashMap::new();
        let mut tables = self.tables.clone();
        for name in names {
            let Some(table) = source.tables.get(name) else {
                continue;
            };
            if tables.get(name) == Some(table) || renamed.contains_key(name) {
                continue;
            }
            let taken = |n: &str| tables.contains(n) || self.parameters.find(n).is_some();
            let new = if taken(name) {
                Tables::free_name(name, &taken)
            } else {
                name.to_owned()
            };
            let mut copy = table.clone();
            copy.name.clone_from(&new);
            tables = tables.with(copy);
            if new != name {
                renamed.insert(name.to_owned(), new);
            }
        }
        if tables != self.tables {
            self.put_tables(tables, &[])?;
        }
        Ok(renamed)
    }
}

/// `text` with the tables in `renamed` renamed, all at once (a copy's
/// expressions, mitcad#667); a text that does not parse is left as it is.
pub(crate) fn rename_tables_in(text: &str, renamed: &HashMap<String, String>) -> String {
    if renamed.is_empty() {
        return text.to_owned();
    }
    let Ok(expr) = Expr::parse(text) else {
        return text.to_owned();
    };
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (table, _, span) in expr.column_reference_spans() {
        if let Some(new) = renamed.get(table) {
            out.push_str(&text[copied..span.start]);
            out.push_str(new);
            copied = span.start + table.len();
        }
    }
    out.push_str(&text[copied..]);
    out
}

impl<K: Kernel> Document<K> {
    /// The design's tables (mitcad#667).
    pub fn tables(&self) -> &Tables {
        &self.state.tables
    }

    /// The table `name`.
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.state.tables.get(name)
    }

    /// The names of the parameters that use the table `name`.
    pub fn table_users(&self, name: &str) -> Vec<String> {
        self.state.table_users(name)
    }

    /// Creates the table of `table`'s name or replaces it; the parameters
    /// that use it are re-evaluated in the same undo step (`Edit Table
    /// <name>`), and the features whose parameters changed value recompute.
    /// Refused with the first problem or the first parameter that would
    /// fail, changing nothing. Returns the parameters whose values changed;
    /// the same table is no command at all.
    pub fn set_table(&mut self, table: Table) -> Result<Vec<String>, ModelError> {
        let label = format!("Edit Table {}", table.name);
        self.set_table_as(table, &label)
    }

    /// [`Document::set_table`] as the undo step `label` (a CSV import's
    /// `Import Table <name>`).
    pub fn set_table_as(&mut self, table: Table, label: &str) -> Result<Vec<String>, ModelError> {
        if self.state.tables.get(&table.name) == Some(&table) {
            return Ok(Vec::new());
        }
        let (names, moves) = self.apply_with(|state| {
            let (changed, moves) = state.set_table_moving(table)?;
            let names: Vec<String> = changed
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect();
            let recompute = !names.is_empty();
            Ok((label.to_owned(), (names, moves), recompute))
        })?;
        // The options the edit moved off removed values (mitcad#806) come
        // before the cascade's moves.
        if !moves.is_empty() {
            let mut all = moves;
            all.append(&mut self.configuration_moves.1);
            self.configuration_moves.1 = all;
        }
        Ok(names)
    }

    /// What [`Document::set_table`] would do, without doing it: the
    /// parameters whose values would change, or why it is refused.
    pub fn check_table(&self, table: &Table) -> Result<Vec<String>, ModelError> {
        let mut state = self.state.clone();
        let changed = state.set_table(table.clone())?;
        Ok(changed
            .iter()
            .map(|id| state.parameters.name(*id))
            .collect())
    }

    /// Renames a table; the expressions and limits that use it follow
    /// (`Rename Table <name> to <new>`). Values do not change.
    pub fn rename_table(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        self.state.require_table(name)?;
        if name == new_name {
            return Ok(());
        }
        self.apply_with(|state| {
            state.rename_table(name, new_name)?;
            Ok((format!("Rename Table {name} to {new_name}"), (), false))
        })
    }

    /// Renames a column of a table; the expressions and limits that use it
    /// follow (`Rename Column <table>.<name> to <new>`).
    pub fn rename_table_column(
        &mut self,
        table: &str,
        name: &str,
        new_name: &str,
    ) -> Result<(), ModelError> {
        if name == new_name && self.state.require_table(table)?.column(name).is_some() {
            return Ok(());
        }
        self.apply_with(|state| {
            state.rename_table_column(table, name, new_name)?;
            Ok((
                format!("Rename Column {table}.{name} to {new_name}"),
                (),
                false,
            ))
        })
    }

    /// Deletes a table that no parameter and no configuration row uses
    /// (`Delete Table <name>`).
    pub fn delete_table(&mut self, name: &str) -> Result<(), ModelError> {
        self.apply_with(|state| {
            state.delete_table(name)?;
            Ok((format!("Delete Table {name}"), (), false))
        })
    }
}
