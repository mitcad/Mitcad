// SPDX-License-Identifier: MIT
//! The parameters exported to and imported from CSV (mitcad#666; the
//! formats are `parameters_csv.rs`).
//!
//! An import works through the rows of the table as the `.f3d` import
//! works through a design's parameters (`core/import-replay/src/params.rs`):
//!
//! - A name Mitcad cannot use (a unit, a function, other characters) gets
//!   a free one ([`free_name`], from the other names of the table, so that
//!   importing the same table again finds it), and the expressions follow.
//! - An existing name is changed, a new one added as a user parameter. A
//!   model or reference parameter (a feature's or a driven dimension's)
//!   keeps its unit, comment and favourite; only its expression and limits
//!   change, and a reference's not at all (it is read-only and has no
//!   limits).
//! - The limits (mitcad#665): the `min`, `max`, `step` and `strict` columns
//!   the table has replace the parameter's (an empty cell removes a limit);
//!   a column the table lacks keeps what the parameter has. A limit that is
//!   wrong refuses the row with the reason.
//! - Rows are taken in dependency order (of their expressions and limits),
//!   so an expression may use a parameter of a later row. Rows in a
//!   circular reference, and rows whose expression does not evaluate (an
//!   unknown name, a wrong unit), take the `value` column instead (a number
//!   in the row's unit), with the reason; without a value that evaluates
//!   they are refused.
//! - Text parameters (unit `Text` or kind `text`, mitcad#664) are imported
//!   as text parameters: their expressions are text expressions (`'6qt'`),
//!   and a `value` cell is their text as it is (or a quoted text).
//!
//! The preview works on a copy of the state; the import is one undo step
//! (`Import Parameters`), none when nothing changes.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use super::{DocState, Document, ModelError, invalid};
use crate::expr::{
    ParamKind, ParamLimits, ParamSpec, Unit, text_literal, value_to_expression, with_decimal_points,
};
use crate::kernel::Kernel;
use crate::parameters::{ParameterError, unit_for};
use crate::parameters_csv::{self, CsvFormat, CsvRow, CsvTable, free_name, identifiers, rename_in};
use mitcad_model_base::internal::parameters::{expression_dims, expression_is_text};

/// What an import does with a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvAction {
    /// A new user parameter.
    Add,
    /// An existing parameter changes (`changes` says what).
    Change,
    /// An existing parameter is already as the row has it.
    Unchanged,
    /// The row cannot be imported (`reason` says why).
    Refuse,
    /// A row of a kind Mitcad does not import yet (a text parameter).
    Skip,
}

/// A row of a parameter CSV and what the import does with it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CsvRowOutcome {
    /// The line of the file the row starts on.
    pub line: usize,
    /// The name in the file.
    pub name: String,
    /// The Mitcad name, when the file's cannot be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed: Option<String>,
    pub action: CsvAction,
    /// What changes of an existing parameter: `expression`, `unit`,
    /// `limits`, `comment`, `favorite`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<&'static str>,
    /// The expression after the import (the file's for a refused or skipped
    /// row).
    pub expression: String,
    /// The unit after the import (the file's for a refused or skipped row).
    pub unit: String,
    /// The value in millimetres or radians, and as text in its unit, after
    /// the import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The limits after the import (mitcad#665), when it has any; the
    /// file's for a refused row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<CsvLimits>,
    /// Which limit changed the value after the import: `min`, `max`,
    /// `step`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limited: Option<&'static str>,
    /// The `value` column was used because the expression did not
    /// evaluate (`reason` says why).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub from_value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What else the import left as it was or ignored.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A parameter's limits as written (mitcad#665), with the names of the
/// `parameters` query's `limits`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CsvLimits {
    pub min: Option<String>,
    pub max: Option<String>,
    pub step: Option<String>,
    pub strict: bool,
    /// The allowed values (mitcad#798); left out when there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
}

impl CsvLimits {
    /// None for no limits and not strict.
    fn of(limits: &ParamLimits) -> Option<Self> {
        (!limits.is_none()).then(|| Self {
            min: limits.min.clone(),
            max: limits.max.clone(),
            step: limits.step.clone(),
            strict: limits.strict,
            values: limits.values.clone(),
        })
    }
}

/// The limits a row gives a parameter that has `current`: the row's
/// columns over the parameter's (an empty cell is no limit), the names the
/// import renamed renamed in them; None when the table has no limit
/// columns.
fn row_limits(
    row: &CsvRow,
    current: &ParamLimits,
    renamed: &HashMap<String, String>,
) -> Option<ParamLimits> {
    if row.min.is_none()
        && row.max.is_none()
        && row.step.is_none()
        && row.strict.is_none()
        && row.values.is_none()
    {
        return None;
    }
    // As the table keeps them: trimmed, decimal points, empty as none.
    let cell = |cell: &Option<String>, kept: &Option<String>| match cell {
        None => kept.clone(),
        Some(text) => {
            let text = rename_in(text.trim(), renamed);
            (!text.is_empty()).then(|| with_decimal_points(&text).into_owned())
        }
    };
    // Allowed values (mitcad#798): a JSON array of expression texts; a
    // cell that is no such array is read as one expression.
    let values = match &row.values {
        None => current.values.clone(),
        Some(text) if text.trim().is_empty() => None,
        Some(text) => {
            let list = serde_json::from_str::<Vec<String>>(text.trim())
                .unwrap_or_else(|_| vec![text.trim().to_owned()]);
            let list: Vec<String> = list
                .iter()
                .map(|t| with_decimal_points(&rename_in(t.trim(), renamed)).into_owned())
                .filter(|t| !t.is_empty())
                .collect();
            (!list.is_empty()).then_some(list)
        }
    };
    Some(ParamLimits {
        min: cell(&row.min, &current.min),
        max: cell(&row.max, &current.max),
        step: cell(&row.step, &current.step),
        strict: row.strict.unwrap_or(current.strict),
        values,
    })
}

/// What importing a parameter CSV does (the preview) or did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CsvImport {
    pub format: CsvFormat,
    /// In the file's order.
    pub rows: Vec<CsvRowOutcome>,
    pub added: usize,
    pub changed: usize,
    /// Rows added or changed under a name of Mitcad's own.
    pub renamed: usize,
    pub unchanged: usize,
    pub refused: usize,
    pub skipped: usize,
}

impl CsvImport {
    /// Whether the import changes the document.
    pub fn changes_anything(&self) -> bool {
        self.added + self.changed > 0
    }

    /// `3 to add, 1 to change, 1 renamed, 1 refused, 1 skipped, 2
    /// unchanged`, leaving out the nought counts (`nothing` when all are).
    pub fn summary(&self) -> String {
        let parts: Vec<String> = [
            (self.added, "to add"),
            (self.changed, "to change"),
            (self.renamed, "renamed"),
            (self.refused, "refused"),
            (self.skipped, "skipped"),
            (self.unchanged, "unchanged"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if parts.is_empty() {
            "nothing".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

impl<K: Kernel> Document<K> {
    /// The parameters as Mitcad's CSV table.
    pub fn parameters_csv(&self) -> String {
        parameters_csv::write(&self.state.parameters, &self.state.favorites)
    }

    /// What importing a parameter CSV would do; nothing changes.
    pub fn preview_parameters_csv(&self, text: &str) -> Result<CsvImport, ModelError> {
        let table = parameters_csv::parse(text).map_err(|e| invalid(e.to_string()))?;
        let mut state = self.state.clone();
        Ok(import_rows(&mut state, &table))
    }

    /// Imports a parameter CSV as one undo step (none when nothing
    /// changes): what the preview says.
    pub fn import_parameters_csv(&mut self, text: &str) -> Result<CsvImport, ModelError> {
        let table = parameters_csv::parse(text).map_err(|e| invalid(e.to_string()))?;
        let preview = {
            let mut state = self.state.clone();
            import_rows(&mut state, &table)
        };
        if !preview.changes_anything() {
            return Ok(preview);
        }
        self.apply(|state| {
            let done = import_rows(state, &table);
            Ok(("Import Parameters".to_owned(), done))
        })
    }
}

/// A row that is imported: its Mitcad name, its expression with the
/// renamed names, its unit (None when the table has no unit column), the
/// names of the table and the document its expression and limits use, and
/// the names the import renamed (for its limits).
struct Pending<'a> {
    index: usize,
    row: &'a CsvRow,
    name: String,
    renamed: Option<String>,
    expression: String,
    unit: Option<Unit>,
    depends: Vec<String>,
    renames: &'a HashMap<String, String>,
}

/// Imports the rows into `state`, row by row; returns what each did.
fn import_rows(state: &mut DocState, table: &CsvTable) -> CsvImport {
    let rows = &table.rows;
    let mut outcomes: Vec<Option<CsvRowOutcome>> = vec![None; rows.len()];

    // Names Mitcad cannot use get a free one, avoiding the table's other
    // names; expressions follow.
    let file_names: BTreeSet<String> = rows
        .iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| r.name.clone())
        .collect();
    let mut taken = file_names.clone();
    let mut renames: HashMap<String, String> = HashMap::new();
    for name in &file_names {
        if !crate::expr::is_valid_name(name) {
            let new = free_name(name, &taken);
            taken.insert(new.clone());
            renames.insert(name.clone(), new);
        }
    }
    let renamed = &renames;
    let doc_names: BTreeSet<String> = state
        .parameters
        .iter()
        .map(|p| p.name().to_owned())
        .collect();

    let mut pending: Vec<Pending<'_>> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let refuse = |reason: String| refused(row, CsvAction::Refuse, reason);
        if row.name.is_empty() {
            outcomes[index] = Some(refuse("the row has no name".to_owned()));
            continue;
        }
        let new_name = renamed.get(&row.name).cloned();
        let name = new_name.clone().unwrap_or_else(|| row.name.clone());
        if let Some(line) = seen.get(&name) {
            outcomes[index] = Some(refuse(format!("{name} is also on line {line}")));
            continue;
        }
        seen.insert(name.clone(), row.line);
        let unit = match row.unit.as_deref() {
            // A text parameter (mitcad#664), also by its kind alone.
            _ if row.is_text() => Some(Unit::TEXT),
            None => None,
            Some(text) => match Unit::parse(text) {
                Ok(unit) => Some(unit),
                Err(e) => {
                    outcomes[index] = Some(refuse(format!("unit “{text}”: {e}")));
                    continue;
                }
            },
        };
        let expression = rename_in(&row.expression, renamed);
        if expression.trim().is_empty() && row.value.is_none() {
            outcomes[index] = Some(refuse("the row has no expression and no value".to_owned()));
            continue;
        }
        pending.push(Pending {
            index,
            row,
            name,
            renamed: new_name,
            expression,
            unit,
            depends: Vec::new(),
            renames: renamed,
        });
    }

    // The names an expression and the limits use: the table's and the
    // document's.
    let known: BTreeSet<String> = doc_names
        .iter()
        .cloned()
        .chain(pending.iter().map(|p| p.name.clone()))
        .collect();
    for p in &mut pending {
        let mut texts = vec![p.expression.clone()];
        for cell in [&p.row.min, &p.row.max, &p.row.step].into_iter().flatten() {
            texts.push(rename_in(cell, renamed));
        }
        let mut depends: Vec<String> = texts
            .iter()
            .flat_map(|text| {
                identifiers(text)
                    .into_iter()
                    .map(move |(s, e)| text[s..e].to_owned())
            })
            .filter(|id| id != &p.name && known.contains(id))
            .collect();
        depends.sort();
        depends.dedup();
        p.depends = depends;
    }

    // Dependency order: repeatedly the rows whose names exist. What remains
    // is in a circular reference, or uses one: it takes its value.
    // A name of the table waits for its row, also when the design has it
    // already, so that a circular reference among the rows is one on every
    // import (importing the same table again changes nothing).
    let row_names: BTreeSet<String> = pending.iter().map(|p| p.name.clone()).collect();
    let mut done: BTreeSet<String> = doc_names.difference(&row_names).cloned().collect();
    let mut order: Vec<usize> = Vec::new();
    let mut placed = vec![false; pending.len()];
    loop {
        let mut progress = false;
        for (i, p) in pending.iter().enumerate() {
            if !placed[i] && p.depends.iter().all(|d| done.contains(d)) {
                placed[i] = true;
                done.insert(p.name.clone());
                order.push(i);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    let unresolved: Vec<usize> = (0..pending.len()).filter(|i| !placed[*i]).collect();
    for i in order {
        let p = &pending[i];
        outcomes[p.index] = Some(import_row(state, p, None));
    }
    for i in unresolved {
        let p = &pending[i];
        let cycle = "it is in a circular reference, or uses one".to_owned();
        outcomes[p.index] = Some(import_row(state, p, Some(cycle)));
    }

    // The values after the whole import: a later row may have changed a
    // parameter that an earlier one uses.
    let mut rows: Vec<CsvRowOutcome> = outcomes.into_iter().flatten().collect();
    for outcome in &mut rows {
        if matches!(outcome.action, CsvAction::Refuse | CsvAction::Skip) {
            continue;
        }
        let name = outcome.renamed.as_deref().unwrap_or(&outcome.name);
        if let Some(param) = state
            .parameters
            .find(name)
            .and_then(|id| state.parameters.get(id))
        {
            outcome.expression = param.expression().to_owned();
            outcome.unit = param.unit().to_string();
            // A text parameter has its text only (mitcad#664).
            outcome.value = (!param.is_text()).then(|| param.value());
            outcome.text = Some(param.formatted_value(crate::expr::DEFAULT_DECIMALS));
            outcome.limits = CsvLimits::of(param.limits());
            outcome.limited = param.limited().map(|limit| limit.as_str());
        }
    }
    let count = |action: CsvAction| rows.iter().filter(|r| r.action == action).count();
    CsvImport {
        format: table.format,
        added: count(CsvAction::Add),
        changed: count(CsvAction::Change),
        renamed: rows
            .iter()
            .filter(|r| {
                r.renamed.is_some() && matches!(r.action, CsvAction::Add | CsvAction::Change)
            })
            .count(),
        unchanged: count(CsvAction::Unchanged),
        refused: count(CsvAction::Refuse),
        skipped: count(CsvAction::Skip),
        rows,
    }
}

/// A row refused or skipped, with the file's expression and unit.
fn refused(row: &CsvRow, action: CsvAction, reason: String) -> CsvRowOutcome {
    CsvRowOutcome {
        line: row.line,
        name: row.name.clone(),
        renamed: None,
        action,
        changes: Vec::new(),
        expression: row.expression.clone(),
        unit: row.unit.clone().unwrap_or_default(),
        value: None,
        text: None,
        limits: row_limits(row, &ParamLimits::default(), &HashMap::new())
            .as_ref()
            .and_then(CsvLimits::of),
        limited: None,
        from_value: false,
        reason: Some(reason),
        notes: Vec::new(),
    }
}

/// The expression of the row's `value` column in `unit`: a number (read in
/// the unit) or a value with a unit, written as the parameter keeps it.
/// None without a value; the reason when it does not evaluate.
fn value_expression(state: &DocState, row: &CsvRow, unit: Unit) -> Option<Result<String, String>> {
    let text = row.value.as_deref()?;
    // A text parameter's value (mitcad#664): a quoted text as it is, any
    // other cell the text itself.
    if unit.is_text() {
        let table = state.parameters.table();
        let quoted = (text.starts_with('\'') || text.starts_with('"'))
            && table
                .evaluate_value(text)
                .is_ok_and(|v| matches!(v, crate::expr::Value::Text(_)));
        return Some(Ok(if quoted {
            text.to_owned()
        } else {
            text_literal(text)
        }));
    }
    Some(
        state
            .parameters
            .table()
            .evaluate(text, unit)
            .map(|q| value_to_expression(q.value, unit))
            .map_err(|e| format!("the value “{text}”: {e}")),
    )
}

/// Imports one row: its expression, or its value when the expression does
/// not evaluate (or `forced`, the reason it cannot be used).
fn import_row(state: &mut DocState, p: &Pending<'_>, forced: Option<String>) -> CsvRowOutcome {
    let row = p.row;
    let mut outcome = refused(row, CsvAction::Refuse, String::new());
    outcome.renamed = p.renamed.clone();
    outcome.expression = p.expression.clone();
    outcome.reason = None;
    let result = match state.parameters.find(&p.name) {
        Some(_) => change_existing(state, p, forced, &mut outcome),
        None => add_new(state, p, forced, &mut outcome),
    };
    // (The values are filled in after the last row.)
    match result {
        Ok(action) => outcome.action = action,
        Err(reason) => {
            outcome.action = CsvAction::Refuse;
            outcome.reason = Some(reason);
            outcome.from_value = false;
        }
    }
    outcome
}

/// Sets an expression (`try_set`) from the row's expression, or from its
/// value when that fails or is `forced` out; records which was used.
fn set_from_row(
    state: &mut DocState,
    p: &Pending<'_>,
    unit: Unit,
    forced: Option<String>,
    outcome: &mut CsvRowOutcome,
    try_set: &mut dyn FnMut(&mut DocState, &str) -> Result<(), String>,
) -> Result<(), String> {
    let first = match forced {
        Some(reason) => Err(reason),
        None if p.expression.trim().is_empty() => Err("the row has no expression".to_owned()),
        None => try_set(state, &p.expression),
    };
    let Err(reason) = first else {
        return Ok(());
    };
    match value_expression(state, p.row, unit) {
        None => Err(reason),
        Some(Err(value_reason)) => Err(format!("{reason}; {value_reason}")),
        Some(Ok(literal)) => {
            // The same error again (a wrong limit) is said once.
            try_set(state, &literal).map_err(|e| {
                if e == reason {
                    e
                } else {
                    format!("{reason}; the value: {e}")
                }
            })?;
            outcome.from_value = true;
            outcome.reason = Some(reason);
            Ok(())
        }
    }
}

/// Changes an existing parameter as the row has it.
fn change_existing(
    state: &mut DocState,
    p: &Pending<'_>,
    forced: Option<String>,
    outcome: &mut CsvRowOutcome,
) -> Result<CsvAction, String> {
    let row = p.row;
    let id = state.parameters.find(&p.name).expect("found by the caller");
    let before = state.parameters.get(id).expect("found").clone();
    let user = before.kind() == ParamKind::User;
    let favorite_before = state.favorites.contains(&id);
    let mut changes = Vec::new();
    // A feature's parameter keeps its unit, comment and favourite.
    let unit = if user {
        p.unit.unwrap_or(before.unit())
    } else {
        let mut kept = Vec::new();
        if p.unit.is_some_and(|u| u != before.unit()) {
            kept.push(format!("its unit {}", before.unit()));
        }
        if row
            .comment
            .as_deref()
            .is_some_and(|c| c != before.comment())
        {
            kept.push("its comment".to_owned());
        }
        if row.favorite.is_some_and(|f| f != favorite_before) {
            kept.push("its favorite".to_owned());
        }
        if !kept.is_empty() {
            outcome.notes.push(format!(
                "a {} parameter keeps {}",
                before.kind(),
                kept.join(", ")
            ));
        }
        before.unit()
    };
    // The row's limits over the parameter's; None keeps them (mitcad#665).
    let limits = row_limits(row, before.limits(), p.renames);
    let same_expression =
        with_decimal_points(p.expression.trim()) == before.expression() && unit == before.unit();
    let same_limits = limits.as_ref().is_none_or(|l| l == before.limits());
    if forced.is_some() || !same_expression || !same_limits {
        if before.kind() == ParamKind::Reference {
            if !same_limits {
                return Err(format!(
                    "{} is the value a driven dimension measures: it has no limits",
                    p.name
                ));
            }
            return Err(ParameterError::ReadOnly(p.name.clone()).to_string());
        }
        if unit != before.unit() {
            state
                .check_unit_change(id, unit)
                .map_err(|e| e.to_string())?;
        }
        let mut set = |state: &mut DocState, expression: &str| -> Result<(), String> {
            state
                .parameters
                .redefine(id, expression, unit, limits.clone())
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        set_from_row(state, p, unit, forced, outcome, &mut set)?;
        let after = state.parameters.get(id).expect("still there");
        if after.expression() != before.expression() {
            changes.push("expression");
        }
        if after.unit() != before.unit() {
            changes.push("unit");
        }
        if after.limits() != before.limits() {
            changes.push("limits");
        }
    }
    if user {
        if let Some(comment) = row.comment.as_deref()
            && comment != before.comment()
        {
            state
                .parameters
                .set_comment(id, comment)
                .map_err(|e| e.to_string())?;
            changes.push("comment");
        }
        if let Some(favorite) = row.favorite
            && favorite != favorite_before
        {
            if favorite {
                state.favorites.insert(id);
            } else {
                state.favorites.remove(&id);
            }
            changes.push("favorite");
        }
    }
    outcome.changes = changes;
    Ok(if outcome.changes.is_empty() {
        CsvAction::Unchanged
    } else {
        CsvAction::Change
    })
}

/// Adds the row as a new user parameter.
fn add_new(
    state: &mut DocState,
    p: &Pending<'_>,
    forced: Option<String>,
    outcome: &mut CsvRowOutcome,
) -> Result<CsvAction, String> {
    let row = p.row;
    // Without a unit column: the unit of the expression's value.
    let unit = match p.unit {
        Some(unit) => unit,
        // A text expression makes a text parameter (mitcad#664).
        None if expression_is_text(&state.parameters, &p.expression) => Unit::TEXT,
        None => expression_dims(&state.parameters, &p.expression)
            .ok()
            .and_then(|dims| unit_for(dims, &state.parameters.context()))
            .ok_or_else(|| format!("“{}” has no unit; give one", p.expression))?,
    };
    if matches!(row.kind.as_deref(), Some("model" | "reference")) {
        outcome
            .notes
            .push("added as a user parameter: no feature of this design owns it".to_owned());
    }
    let comment = row.comment.clone().unwrap_or_default();
    let name = p.name.clone();
    let limits = row_limits(row, &ParamLimits::default(), p.renames).unwrap_or_default();
    let mut set = |state: &mut DocState, expression: &str| -> Result<(), String> {
        let spec = ParamSpec::user(&name, expression, unit)
            .with_comment(&comment)
            .with_limits(limits.clone());
        state
            .parameters
            .add(spec, None)
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    set_from_row(state, p, unit, forced, outcome, &mut set)?;
    if row.favorite == Some(true)
        && let Some(id) = state.parameters.find(&p.name)
    {
        state.favorites.insert(id);
    }
    Ok(CsvAction::Add)
}
