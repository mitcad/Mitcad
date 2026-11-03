// SPDX-License-Identifier: MIT
//! The configuration table of a design and its commands (mitcad#64,
//! mitcad#797): setting the table (reading the old form with its
//! migration), applying a row, choosing option values (`configure`), the
//! report the choosers read and Check Rules. The table, the rules and the
//! cascade are `mitcad_model_base::configurations`; the cascade runs after
//! every command that changes parameters (`Document::apply_inner`).

use std::collections::{BTreeMap, BTreeSet};

use super::{DocState, Document, ModelError, invalid};
use crate::configurations::{
    ConfigurationMove, Configurations, ConfigureOutcome, ConfigureReport, OptionValue, RulesReport,
    SelectorMigration, apply_values, cascade,
};
use crate::expr::{ParamSpec, Unit, text_literal, value_to_expression};
use crate::kernel::Kernel;
use crate::parameters::Parameters;

/// What `set_configurations` did besides setting the table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigurationsSet {
    /// The table's options, after a migration of an old table.
    pub options: Vec<String>,
    /// The text parameters the migration of an old table made.
    pub added: Vec<String>,
    /// What the migration says (a selector whose name was taken).
    pub warnings: Vec<String>,
}

/// Makes an old table's selectors options ([`Configurations::migrate_selectors`])
/// and adds their text parameters to `parameters`; returns the names
/// added and the warnings. A selector whose name is a text parameter that
/// is already an option of `current` keeps that parameter.
pub(crate) fn migrate(
    table: &mut Configurations,
    parameters: &mut Parameters,
    current: &Configurations,
) -> Result<(Vec<String>, Vec<String>), String> {
    if table.selectors.is_empty() {
        table.migrate_selectors(&|_| false, None);
        return Ok((Vec::new(), Vec::new()));
    }
    let start = table.current(parameters).map(|r| r.name.clone());
    let reusable = |name: &str| {
        current.is_option(name)
            && parameters
                .find(name)
                .and_then(|id| parameters.get(id))
                .is_some_and(|p| p.is_text())
    };
    let taken = |name: &str| parameters.find(name).is_some() && !reusable(name);
    let migrated: Vec<SelectorMigration> = table.migrate_selectors(&taken, start.as_deref());
    let mut added = Vec::new();
    let mut warnings = Vec::new();
    for m in migrated {
        if m.option != m.selector {
            warnings.push(format!(
                "the configuration table's selector {} is the option {}: a parameter {} \
                 exists",
                m.selector, m.option, m.selector
            ));
        }
        if parameters.find(&m.option).is_some() {
            continue;
        }
        let spec = ParamSpec::user(&m.option, &text_literal(&m.text), Unit::TEXT);
        parameters
            .add(spec, None)
            .map_err(|e| format!("option {}: {e}", m.option))?;
        added.push(m.option);
    }
    Ok((added, warnings))
}

/// The constant expression that chooses `value` for an option.
fn option_expression(
    parameters: &Parameters,
    name: &str,
    value: &OptionValue,
) -> Result<String, String> {
    let p = parameters
        .find(name)
        .and_then(|id| parameters.get(id))
        .ok_or_else(|| format!("option {name} does not exist"))?;
    let expression = match value {
        OptionValue::Number(_) if p.is_text() => {
            return Err(format!("option {name} is a text: give it a text"));
        }
        OptionValue::Number(v) if !v.is_finite() => {
            return Err(format!("option {name}: {v} is not a finite number"));
        }
        OptionValue::Number(v) => value_to_expression(*v, p.unit()),
        OptionValue::Text(text) if p.is_text() => text_literal(text),
        OptionValue::Text(text) | OptionValue::Expression(text) => text.trim().to_owned(),
    };
    let expr = crate::expr::Expr::parse(&expression).map_err(|e| format!("{name}: {e}"))?;
    if !expr.is_constant() {
        return Err(format!(
            "option {name}: {expression} is not a constant (a formula makes the option driven: \
             change it with set_parameter)"
        ));
    }
    Ok(expression)
}

/// The value as the undo label shows it: a text's text, else the
/// expression.
fn label_value(value: &OptionValue, expression: &str) -> String {
    match value {
        OptionValue::Text(text) => text.clone(),
        _ => expression.to_owned(),
    }
}

/// Sets the chosen options' expressions on `parameters` and runs the
/// cascade with them pinned; the moves and the rules still broken.
fn try_values(
    parameters: &mut Parameters,
    table: &Configurations,
    expressions: &[(String, String)],
) -> Result<crate::configurations::Cascade, String> {
    let mut ids = Vec::with_capacity(expressions.len());
    for (name, expression) in expressions {
        let id = parameters
            .find(name)
            .ok_or_else(|| format!("option {name} does not exist"))?;
        ids.push((id, expression.as_str()));
    }
    parameters
        .set_expressions(&ids)
        .map_err(|e| e.to_string())?;
    let pinned: BTreeSet<String> = expressions.iter().map(|(n, _)| n.clone()).collect();
    cascade(parameters, table, &pinned)
}

impl<K: Kernel> Document<K> {
    /// The design's configuration table (empty when it has none).
    pub fn configurations(&self) -> &Configurations {
        &self.state.configurations
    }

    /// The options the cascade moved in the command that made the current
    /// state (mitcad#799); empty when it moved none, or when the state
    /// came from another command (an undo, a command that changed nothing).
    pub fn configuration_moves(&self) -> &[ConfigurationMove] {
        if self.configuration_moves.0 == self.revision {
            &self.configuration_moves.1
        } else {
            &[]
        }
    }

    /// Sets the design's configuration table (empty: none), checked
    /// against its parameters; an undo step. A table of the old form
    /// (`selectors`) is migrated: its selectors become text parameters in
    /// the same undo step.
    pub fn set_configurations(
        &mut self,
        table: Configurations,
    ) -> Result<ConfigurationsSet, ModelError> {
        let mut table = table;
        if table == self.state.configurations && table.selectors.is_empty() {
            return Ok(ConfigurationsSet {
                options: table.options,
                ..ConfigurationsSet::default()
            });
        }
        let current = self.state.configurations.clone();
        self.apply_with(|state| {
            let (added, warnings) =
                migrate(&mut table, &mut state.parameters, &current).map_err(invalid)?;
            if let Some(problem) = table.problems(&state.parameters).first() {
                return Err(invalid(problem.clone()));
            }
            let set = ConfigurationsSet {
                options: table.options.clone(),
                added,
                warnings,
            };
            state.configurations = table;
            Ok(("Edit Configurations".to_owned(), set, false))
        })
    }

    /// Sets the parameters to a row of the table; returns the parameters
    /// whose values changed. The cascade's moves are
    /// [`Document::configuration_moves`].
    pub fn apply_configuration(&mut self, name: &str) -> Result<Vec<String>, ModelError> {
        let row = self
            .state
            .configurations
            .row(name)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "the design has no configuration {name} (its rows: {})",
                    super::library_parts::row_names(&self.state.configurations)
                ))
            })?;
        self.apply_with(|state| {
            let changed = apply_values(&mut state.parameters, &row).map_err(invalid)?;
            let recompute = !changed.is_empty();
            Ok((format!("Apply {name}"), changed, recompute))
        })
    }

    /// Chooses option values (the `configure` command): writes each as its
    /// option's constant and runs the cascade, as one undo step (`Configure
    /// Color = Black`). When the cascade cannot repair the change nothing
    /// changes and the outcome says which rule refused it.
    pub fn configure(
        &mut self,
        values: &BTreeMap<String, OptionValue>,
    ) -> Result<ConfigureOutcome, ModelError> {
        let expressions = self.option_expressions(values)?;
        // Tried first, so that a refusal is an answer, not an error.
        let mut trial = self.state.parameters.clone();
        let table = self.state.configurations.clone();
        let before = broken_rules(&table, &self.state.parameters)?;
        let tried = try_values(&mut trial, &table, &expressions).map_err(invalid)?;
        if let Some(refused) = tried
            .violations
            .iter()
            .find(|v| !before.contains(&v.rule))
            .cloned()
        {
            return Ok(ConfigureOutcome {
                refused: Some(refused),
                ..ConfigureOutcome::default()
            });
        }
        let label = format!(
            "Configure {}",
            expressions
                .iter()
                .map(|(name, expression)| format!(
                    "{name} = {}",
                    label_value(&values[name], expression)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let row = |doc: &Self| {
            doc.state
                .configurations
                .matching_row(&doc.state.parameters)
                .map(|r| r.name.clone())
        };
        // The same values again: no command.
        let same = expressions.iter().all(|(name, expression)| {
            self.state
                .parameters
                .find(name)
                .and_then(|id| self.state.parameters.get(id))
                .is_some_and(|p| p.expression() == crate::expr::with_decimal_points(expression))
        });
        if same {
            return Ok(ConfigureOutcome {
                row: row(self),
                ..ConfigureOutcome::default()
            });
        }
        let old = self.state.parameters.clone();
        self.apply_with(|state| {
            let mut ids = Vec::with_capacity(expressions.len());
            for (name, expression) in &expressions {
                let id = state.parameters.find(name).expect("checked above");
                ids.push((id, expression.as_str()));
            }
            let changed = state.parameters.set_expressions(&ids)?;
            let recompute = !changed.is_empty();
            Ok((label, (), recompute))
        })?;
        // Every parameter whose value changed: those set, the moved
        // options, a closed row's parameters and what uses them.
        let changed = self
            .state
            .parameters
            .iter()
            .filter(|p| {
                old.find(p.name())
                    .and_then(|id| old.get(id))
                    .is_none_or(|q| q.value() != p.value() || q.text() != p.text())
            })
            .map(|p| p.name().to_owned())
            .collect();
        Ok(ConfigureOutcome {
            changed,
            moves: self.configuration_moves().to_vec(),
            row: row(self),
            refused: None,
        })
    }

    /// The `configure` query: each option as the choosers show it after
    /// the cascade of `values` (none: the current values), on a copy;
    /// nothing changes.
    pub fn configure_report(
        &self,
        values: &BTreeMap<String, OptionValue>,
    ) -> Result<ConfigureReport, ModelError> {
        let expressions = self.option_expressions(values)?;
        let table = &self.state.configurations;
        let mut trial = self.state.parameters.clone();
        let before = broken_rules(table, &self.state.parameters)?;
        let mut report = ConfigureReport::default();
        match try_values(&mut trial, table, &expressions) {
            Ok(tried) => {
                report.refused = tried
                    .violations
                    .iter()
                    .find(|v| !before.contains(&v.rule))
                    .cloned();
                report.moves = tried.moves;
            }
            Err(e) => return Err(invalid(e)),
        }
        // A refused try shows the options as they are.
        let shown = if report.refused.is_some() {
            &self.state.parameters
        } else {
            &trial
        };
        report.options = table.option_reports(shown).map_err(invalid)?;
        report.row = table.matching_row(shown).map(|r| r.name.clone());
        report.designation_values = table.designation_values(shown);
        report.problems = table.problems(shown);
        for violation in table.violations(shown).map_err(invalid)? {
            let text = violation.text();
            if !report.problems.contains(&text) {
                report.problems.push(text);
            }
        }
        Ok(report)
    }

    /// Check Rules (the `check_rules` query) over the options' finite
    /// domains, up to `limit` combinations.
    pub fn check_rules(&self, limit: u64) -> RulesReport {
        self.state
            .configurations
            .check_rules(&self.state.parameters, limit)
    }

    /// The constants `values` writes, by option, in the table's option
    /// order; a name that is no option, or a driven option, is refused.
    fn option_expressions(
        &self,
        values: &BTreeMap<String, OptionValue>,
    ) -> Result<Vec<(String, String)>, ModelError> {
        let table = &self.state.configurations;
        let params = &self.state.parameters;
        for name in values.keys() {
            if !table.is_option(name) {
                return Err(invalid(format!(
                    "{name} is not an option of the configurations (options: {})",
                    if table.options.is_empty() {
                        "none".to_owned()
                    } else {
                        table.options.join(", ")
                    }
                )));
            }
            let driven = params
                .find(name)
                .and_then(|id| params.get(id))
                .is_some_and(|p| !p.expr().is_constant());
            if driven {
                return Err(invalid(format!(
                    "option {name} is by formula: its value is calculated from its expression; \
                     change the expression with set_parameter"
                )));
            }
        }
        table
            .options
            .iter()
            .filter_map(|name| values.get(name).map(|value| (name, value)))
            .map(|(name, value)| {
                option_expression(params, name, value)
                    .map(|expression| (name.clone(), expression))
                    .map_err(invalid)
            })
            .collect()
    }
}

/// The names of the rules (and [`crate::configurations::CLOSED_TABLE`])
/// the parameters break.
fn broken_rules(
    table: &Configurations,
    parameters: &Parameters,
) -> Result<Vec<String>, ModelError> {
    Ok(table
        .violations(parameters)
        .map_err(invalid)?
        .into_iter()
        .map(|v| v.rule)
        .collect())
}

/// Runs the cascade of the configuration rules on a command's new state
/// (from `Document::apply_inner`): the options whose expressions the
/// command changed are pinned. Returns the moves; a rule that held before
/// and is broken after refuses the command.
pub(super) fn enforce(
    before: &DocState,
    next: &mut DocState,
) -> Result<Vec<ConfigurationMove>, ModelError> {
    let table = next.configurations.clone();
    if !table.enforces() {
        return Ok(Vec::new());
    }
    let pinned: BTreeSet<String> = table
        .options
        .iter()
        .filter(|name| {
            let old = before
                .parameters
                .find(name)
                .and_then(|id| before.parameters.get(id))
                .map(|p| p.expression().to_owned());
            let new = next
                .parameters
                .find(name)
                .and_then(|id| next.parameters.get(id))
                .map(|p| p.expression().to_owned());
            old != new
        })
        .cloned()
        .collect();
    // What was broken before, by the new table: a table just edited
    // refuses nothing the parameters broke already.
    let broken_before: Vec<String> = table
        .violations(&before.parameters)
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.rule)
        .collect();
    let mut done = cascade(&mut next.parameters, &table, &pinned).map_err(invalid)?;
    // A closed table's row goes with its combination: choosing its options
    // sets its other parameters too, except those the command set itself.
    if table.closed
        && (!pinned.is_empty() || !done.moves.is_empty())
        && let Some(row) = table.matching_row(&next.parameters).cloned()
    {
        let mut expressions = Vec::new();
        for (name, expression) in &row.values {
            if table.is_option(name) {
                continue;
            }
            let (Some(id), old) = (
                next.parameters.find(name),
                before
                    .parameters
                    .find(name)
                    .and_then(|id| before.parameters.get(id))
                    .map(|p| p.expression().to_owned()),
            ) else {
                continue;
            };
            let now = next.parameters.get(id).map(|p| p.expression().to_owned());
            if old.is_some() && old != now {
                continue;
            }
            expressions.push((id, expression.as_str()));
        }
        if !expressions.is_empty() {
            next.parameters
                .set_expressions(&expressions)
                .map_err(|e| invalid(format!("{}: {e}", row.name)))?;
            done.violations = table.violations(&next.parameters).map_err(invalid)?;
        }
    }
    if let Some(new) = done
        .violations
        .iter()
        .find(|v| !broken_before.contains(&v.rule))
    {
        return Err(invalid(new.text()));
    }
    Ok(done.moves)
}
