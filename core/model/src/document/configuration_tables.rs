// SPDX-License-Identifier: MIT
//! Tables in configurations (mitcad#806; `docs/configurations.md`, "Tables
//! and configurations"): rows generated from a table column, and the
//! options a table edit moves off values it removed. The logic is
//! `mitcad_model_base::configurations` (`from_tables.rs`); this module is
//! the document's side of it, and [`design_tables`] the one place that
//! reads the document's tables (mitcad#803).

use super::{DocState, Document, ModelError, invalid};
use crate::configurations::{
    ConfigurationMove, GeneratedRows, NewParameter, TableColumns, columns_of, tables_of,
};
use crate::expr::ParamSpec;
use crate::kernel::Kernel;
use crate::parameters::{ChangeSet, Parameters};
use crate::tables::Tables;

/// The design's tables as configurations read them
/// ([`tables_of`]).
pub(crate) fn design_tables(state: &DocState) -> impl TableColumns + '_ {
    tables_of(&state.parameters)
}

/// Moves the options whose allowed values are a column of `table` off the
/// values an edit of that table removed (`tables`: the design's tables
/// after the edit). The new tables are put in the parameters without
/// evaluating them, and the moved options evaluate with them, before the
/// table's users are re-evaluated (a text that is no longer allowed would
/// refuse the edit). Returns the parameters whose values changed and the
/// moves, which the table command reports with the cascade's.
pub(crate) fn move_off_removed_values(
    next: &mut DocState,
    tables: &Tables,
    table: &str,
) -> Result<(ChangeSet, Vec<ConfigurationMove>), ModelError> {
    let moves = next
        .configurations
        .column_moves(&next.parameters, &columns_of(tables), table)
        .map_err(invalid)?;
    if moves.is_empty() {
        return Ok((ChangeSet::new(), Vec::new()));
    }
    next.parameters.set_tables(tables.source(), &[])?;
    let mut ids = Vec::with_capacity(moves.len());
    for m in &moves {
        let id = next
            .parameters
            .find(&m.option)
            .ok_or_else(|| invalid(format!("option {} does not exist", m.option)))?;
        ids.push((id, m.expression.as_str()));
    }
    let changed = next.parameters.set_expressions(&ids)?;
    Ok((changed, moves.into_iter().map(|m| m.change).collect()))
}

/// Adds the user parameters a fill of the rows makes (mitcad#1044), in
/// their order; refused with the parameter's name.
pub(crate) fn add_new_parameters(
    parameters: &mut Parameters,
    created: &[NewParameter],
) -> Result<(), ModelError> {
    for new in created {
        parameters
            .add(ParamSpec::user(&new.name, &new.expression, new.unit), None)
            .map_err(|e| invalid(format!("parameter {}: {e}", new.name)))?;
    }
    Ok(())
}

impl<K: Kernel> Document<K> {
    /// Makes one configuration row per key of a table column (`Screws.Size`),
    /// named by the key and setting `option` to it (the
    /// `generate_configuration_rows` command). Rows of the same name are
    /// updated, not duplicated; the option becomes an option of the table
    /// when it is not one yet (a parameter column moves to the options).
    /// `columns` (mitcad#1044) are other columns of the table that become
    /// the closed table's parameter columns, each row set from its key's
    /// row; the parameters the design lacks are made with the columns'
    /// units. One undo step (`Generate Rows from Screws`) that recomputes
    /// nothing; refused with the table's first problem.
    pub fn generate_configuration_rows(
        &mut self,
        table: &str,
        column: &str,
        option: &str,
        columns: &[String],
    ) -> Result<GeneratedRows, ModelError> {
        let mut configurations = self.state.configurations.clone();
        let generated = configurations
            .generate_rows(
                &self.state.parameters,
                &design_tables(&self.state),
                table,
                column,
                option,
                columns,
            )
            .map_err(invalid)?;
        if generated.created.is_empty() {
            if let Some(problem) = configurations.problems(&self.state.parameters).first() {
                return Err(invalid(problem.clone()));
            }
            if configurations == self.state.configurations {
                return Ok(generated);
            }
        }
        self.apply_with(|state| {
            add_new_parameters(&mut state.parameters, &generated.created)?;
            if let Some(problem) = configurations.problems(&state.parameters).first() {
                return Err(invalid(problem.clone()));
            }
            state.configurations = configurations;
            Ok((format!("Generate Rows from {table}"), generated, false))
        })
    }
}
