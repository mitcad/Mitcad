// SPDX-License-Identifier: MIT
//! Configurations (mitcad#64, mitcad#797): a design that stands for a
//! family of products. The user picks a value for each **option** (a user
//! parameter: `Color`, `Length`, `Size`), **rules** remove the values and
//! combinations that are not allowed ([`rules`]), and named **rows** are
//! presets of option values. An **open** table allows every combination
//! the rules allow; a **closed** table only its rows' combinations, and
//! its rows may also set other parameters, as the tables before mitcad#798
//! did. A library component is inserted in one of its rows (the model's
//! `library`).
//!
//! The table is data the design's author writes: applying a row or a
//! choice only changes parameter expressions, as Change Parameters does.
//! A table of the old form (`selectors`, rows with `select`) is read and
//! migrated ([`Configurations::migrate_selectors`]); it is never written.
//! The design note is `docs/configurations.md`; the commands are in
//! `commands.md`, "Configurations: options, rows and rules".

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::expr::{DEFAULT_DECIMALS, Expr, ParamKind, TableError, format_number, text_literal};
use crate::parameters::{ParameterError, Parameters};

// Rules, the cascade, the reports of the choosers and Check Rules
// (mitcad#799).
mod rules;
pub use rules::{
    CLOSED_TABLE, Cascade, ConfigurationMove, ConfigureOutcome, ConfigureReport, Domain,
    DomainValue, EmptyIntersection, OptionReport, OptionValue, RuleProblem, RuleViolation,
    RulesReport, cascade,
};

// Tables in configurations: option values from a column, `in_table`, rows
// generated from a table (mitcad#806).
mod from_tables;
pub use from_tables::{
    Cell, ColumnMove, GeneratedRows, InTable, NoTables, TableColumn, TableColumns, column_domain,
    column_reference, columns_of, current_option_value, list_column, list_values,
    same_option_value, tables_of, values_column,
};

// Filling a closed table's rows from a table's columns or a file
// (mitcad#1044, decision 12).
mod fill;
pub use fill::{NewParameter, RowFields, RowSource};

/// A design's configuration table; empty when it has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configurations {
    /// The options: names of user parameters in choice order (the order
    /// in which the cascade repairs them and the choosers show them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Only the rows' combinations are valid; the rows may also set other
    /// `parameters`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    /// The other parameters the rows of a closed table set, by name.
    #[serde(default)]
    pub parameters: Vec<String>,
    /// The row a design is inserted in when none is asked for; the first
    /// row when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub rows: Vec<ConfigurationRow>,
    /// The rules (mitcad#799).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<ConfigurationRule>,
    /// The cascaded choices of a table written before mitcad#798: read
    /// and migrated to options ([`Configurations::migrate_selectors`]),
    /// never written.
    #[serde(default, skip_serializing)]
    pub selectors: Vec<String>,
}

/// One row of a configuration table: a named preset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRow {
    /// Unique in the table (`M5x16`).
    pub name: String,
    /// Expressions of the options (constants: `'M5'`, `16 mm`) and of a
    /// closed table's other parameters (`"5 mm"`); a parameter left out
    /// keeps the design's own expression.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// How a bill of materials names a part made in this row
    /// (`ISO 4762 M5x16`), when it differs from the library's pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designation: Option<String>,
    /// The supplier's article number of a part made in this row, when it
    /// differs from the library's (mitcad#375).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_number: Option<String>,
    /// The supplier's product page of the row (https), when it has its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier_url: Option<String>,
    /// The row's value of each selector of a table written before
    /// mitcad#798 (`{"Size": "M5"}`): migrated into `values`, never
    /// written.
    #[serde(default, skip_serializing)]
    pub select: BTreeMap<String, String>,
}

/// A rule of a configuration table (mitcad#799): when `when` holds (or is
/// left out), the actions of `then` apply, else those of `else`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRule {
    /// Unique in the table (`Black is short`).
    pub name: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// A condition in the expression language (`Color = 'Black'`); true
    /// when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default)]
    pub then: Vec<RuleAction>,
    #[serde(default, rename = "else", skip_serializing_if = "Vec::is_empty")]
    pub otherwise: Vec<RuleAction>,
}

impl Default for ConfigurationRule {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            comment: String::new(),
            when: None,
            then: Vec::new(),
            otherwise: Vec::new(),
        }
    }
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// An action of a rule. An option's restriction names the `option` and
/// gives any of `values`, `exclude`, `min`, `max` and `step`; a check of
/// the combination is `require` (a condition, with a `message`) or
/// `forbid` (the message).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<String>,
    /// Allow only these values (expressions in the option's unit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    /// Not these values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    /// A condition the combination must meet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require: Option<String>,
    /// The combination is not allowed; the text is the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbid: Option<String>,
    /// The message of a `require`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The combination must be a row of this table (mitcad#806): the
    /// options of `columns` take the values of one of its rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_table: Option<String>,
    /// The columns of an `in_table` action, by option ({option: column}).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<BTreeMap<String, String>>,
}

impl RuleAction {
    /// Whether it restricts an option (rather than checks the combination).
    pub fn is_restriction(&self) -> bool {
        self.values.is_some()
            || self.exclude.is_some()
            || self.min.is_some()
            || self.max.is_some()
            || self.step.is_some()
    }

    /// Its expression texts, with what each is (`values`, `exclude`,
    /// `min`, `max`, `step`, `require`).
    pub fn expressions(&self) -> Vec<(&'static str, &str)> {
        let mut out = Vec::new();
        for (what, list) in [("values", &self.values), ("exclude", &self.exclude)] {
            for text in list.iter().flatten() {
                out.push((what, text.as_str()));
            }
        }
        for (what, text) in [
            ("min", &self.min),
            ("max", &self.max),
            ("step", &self.step),
            ("require", &self.require),
        ] {
            if let Some(text) = text {
                out.push((what, text.as_str()));
            }
        }
        out
    }

    fn expressions_mut(&mut self) -> Vec<(&'static str, &mut String)> {
        let mut out = Vec::new();
        for list in [&mut self.values, &mut self.exclude].into_iter().flatten() {
            for text in list.iter_mut() {
                out.push(("value", text));
            }
        }
        for (what, text) in [
            ("min", &mut self.min),
            ("max", &mut self.max),
            ("step", &mut self.step),
        ] {
            if let Some(text) = text.as_mut() {
                out.push((what, text));
            }
        }
        if let Some(text) = self.require.as_mut() {
            out.push(("require", text));
        }
        out
    }
}

impl ConfigurationRule {
    /// Every action, of `then` and of `else`.
    pub fn actions(&self) -> impl Iterator<Item = &RuleAction> {
        self.then.iter().chain(&self.otherwise)
    }
}

/// A selector of an old table made an option
/// ([`Configurations::migrate_selectors`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorMigration {
    /// The selector's name in the old table.
    pub selector: String,
    /// The text parameter that is its option (`Size`, or `Size2` when
    /// `Size` was taken).
    pub option: String,
    /// The text the new parameter starts with: the value of the row the
    /// parameters have, else of the default row, else empty.
    pub text: String,
}

/// Parses a condition, which may compare with a single `=` at its top
/// level (`Color = 'Black'`): it is read in parentheses.
pub fn parse_condition(text: &str) -> Result<Expr, String> {
    Expr::parse(&format!("({text})")).map_err(|e| e.to_string())
}

/// A rule's or a row's expression with references to `old` renamed;
/// a condition (`condition`) is parsed as [`parse_condition`] reads it.
fn rename_in(text: &str, old: &str, new: &str, condition: bool) -> Result<String, String> {
    // A table column as a list (`Screws.Size`, mitcad#806) names no
    // parameter.
    if !condition && from_tables::column_reference(text).is_some() {
        return Ok(text.to_owned());
    }
    if condition {
        let wrapped = format!("({text})");
        let expr = Expr::parse(&wrapped).map_err(|e| e.to_string())?;
        let renamed = crate::expr::replace_references(&wrapped, &expr, old, new);
        Ok(renamed[1..renamed.len() - 1].to_owned())
    } else {
        let expr = Expr::parse(text).map_err(|e| e.to_string())?;
        Ok(crate::expr::replace_references(text, &expr, old, new))
    }
}

impl Configurations {
    /// Keeps the table's names and expression references in step with a
    /// parameter rename: options, parameters, rows, and the rules'
    /// conditions, options and action expressions. Parsed reference spans
    /// avoid changing substrings of other parameter names, units or
    /// functions.
    pub fn rename_parameter(&mut self, old: &str, new: &str) -> Result<(), String> {
        for name in self.options.iter_mut().chain(&mut self.parameters) {
            if name == old {
                new.clone_into(name);
            }
        }
        for row in &mut self.rows {
            if let Some(expression) = row.values.remove(old) {
                row.values.insert(new.to_owned(), expression);
            }
            for expression in row.values.values_mut() {
                *expression = rename_in(expression, old, new, false)?;
            }
        }
        for rule in &mut self.rules {
            if let Some(when) = rule.when.as_mut() {
                *when = rename_in(when, old, new, true)?;
            }
            for action in rule.then.iter_mut().chain(&mut rule.otherwise) {
                if action.option.as_deref() == Some(old) {
                    action.option = Some(new.to_owned());
                }
                if let Some(column) = action.columns.as_mut().and_then(|c| c.remove(old)) {
                    action
                        .columns
                        .get_or_insert_default()
                        .insert(new.to_owned(), column);
                }
                for (what, text) in action.expressions_mut() {
                    *text = rename_in(text, old, new, what == "require")?;
                }
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
            && self.parameters.is_empty()
            && self.options.is_empty()
            && self.rules.is_empty()
            && self.selectors.is_empty()
    }

    pub fn row(&self, name: &str) -> Option<&ConfigurationRow> {
        self.rows.iter().find(|row| row.name == name)
    }

    /// The row a design is inserted in when none is asked for.
    pub fn default_row(&self) -> Option<&ConfigurationRow> {
        self.default
            .as_deref()
            .and_then(|name| self.row(name))
            .or_else(|| self.rows.first())
    }

    /// Whether `name` is an option of the table.
    pub fn is_option(&self, name: &str) -> bool {
        self.options.iter().any(|o| o == name)
    }

    /// The names of the rules that use a parameter: in a condition, an
    /// action's expression or as the option an action restricts.
    pub fn rules_using(&self, name: &str) -> Vec<String> {
        let uses = |text: &str, condition: bool| {
            let parsed = if condition {
                parse_condition(text).ok()
            } else {
                Expr::parse(text).ok()
            };
            parsed.is_some_and(|e| e.references().contains(&name))
        };
        self.rules
            .iter()
            .filter(|rule| {
                rule.when.as_deref().is_some_and(|w| uses(w, true))
                    || rule.actions().any(|a| {
                        a.option.as_deref() == Some(name)
                            || a.columns.as_ref().is_some_and(|c| c.contains_key(name))
                            || a.expressions()
                                .into_iter()
                                .any(|(what, text)| uses(text, what == "require"))
                    })
            })
            .map(|rule| rule.name.clone())
            .collect()
    }

    /// What the table needs of a parameter so that it cannot be deleted:
    /// `option`, `the configuration table` (a row sets it) or the rules
    /// that use it; empty when it is free.
    pub fn users_of(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        if self.is_option(name) {
            out.push("the configurations as an option".to_owned());
        } else if self.parameters.iter().any(|p| p == name)
            || self.rows.iter().any(|r| r.values.contains_key(name))
        {
            out.push("the configuration table".to_owned());
        }
        for rule in self.rules_using(name) {
            out.push(format!("the rule {rule}"));
        }
        out
    }

    /// The text of each option's value in a row, as the old selectors
    /// gave them ([`constant_text`]): a text option's text, a number
    /// option's number without its unit (`16`).
    pub fn row_option_texts(&self, row: &ConfigurationRow) -> BTreeMap<String, String> {
        self.options
            .iter()
            .filter_map(|option| Some((option.clone(), constant_text(row.values.get(option)?)?)))
            .collect()
    }

    /// The text of each option's current value, as a designation pattern
    /// takes it (`{Size}`, `{Length}`): a text option's text, a number
    /// option's number in its unit without the unit.
    pub fn designation_values(&self, parameters: &Parameters) -> BTreeMap<String, String> {
        self.options
            .iter()
            .filter_map(|option| {
                let p = parameters.get(parameters.find(option)?)?;
                let text = match p.text() {
                    Some(text) => text.to_owned(),
                    None => format_number(p.value_in_unit(), DEFAULT_DECIMALS),
                };
                Some((option.clone(), text))
            })
            .collect()
    }

    /// The values each option takes in the rows, as texts
    /// ([`Configurations::row_option_texts`]) in natural order (`M2.5`,
    /// `M3`, `M10`): what the old tables' cascaded choices showed.
    pub fn selector_values(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = self
            .options
            .iter()
            .map(|o| (o.clone(), BTreeSet::new()))
            .collect();
        for row in &self.rows {
            for (option, text) in self.row_option_texts(row) {
                out.entry(option).or_default().insert(text);
            }
        }
        out.into_iter()
            .map(|(option, values)| {
                let mut values: Vec<String> = values.into_iter().collect();
                values.sort_by(|a, b| natural_cmp(a, b));
                (option, values)
            })
            .collect()
    }

    /// The first row whose combination the parameters have
    /// ([`Configurations::row_matches`]).
    pub fn matching_row(&self, parameters: &Parameters) -> Option<&ConfigurationRow> {
        self.rows
            .iter()
            .find(|row| self.row_matches(row, parameters))
    }

    /// The row of the combination the parameters have: `named` when the
    /// parameters have its values, or in a table without options (its rows
    /// are chosen by name, they may set nothing), else the first row that
    /// matches ([`Configurations::matching_row`]); None when the
    /// combination is no row.
    pub fn combination_row(
        &self,
        parameters: &Parameters,
        named: Option<&str>,
    ) -> Option<&ConfigurationRow> {
        named
            .and_then(|name| self.row(name))
            .filter(|row| self.options.is_empty() || self.row_matches(row, parameters))
            .or_else(|| self.matching_row(parameters))
    }

    /// Whether the parameters have the row's combination: its option
    /// values (every value of a table without options), compared by value
    /// (a text by its text). A row without such values matches nothing.
    pub fn row_matches(&self, row: &ConfigurationRow, parameters: &Parameters) -> bool {
        let mut compared = row
            .values
            .iter()
            .filter(|(name, _)| self.options.is_empty() || self.is_option(name))
            .peekable();
        compared.peek().is_some()
            && compared.all(|(name, expression)| has_value(parameters, name, expression))
    }

    /// Makes the selectors of an old table (before mitcad#798) options: a
    /// closed table whose options are text parameters named after the
    /// selectors (the next free name when `taken` says a name is used),
    /// whose rows' `select` becomes their option values. The new options
    /// start at the values of the row `start` (the row the parameters
    /// have), else of the default row. The caller adds the parameters
    /// ([`SelectorMigration`]). Nothing happens to a table without
    /// selectors.
    pub fn migrate_selectors(
        &mut self,
        taken: &dyn Fn(&str) -> bool,
        start: Option<&str>,
    ) -> Vec<SelectorMigration> {
        // An old table without selectors (rows chosen by name) sets other
        // parameters: it is closed too.
        if self.options.is_empty() && self.selectors.is_empty() && !self.parameters.is_empty() {
            self.closed = true;
        }
        if self.selectors.is_empty() {
            for row in &mut self.rows {
                row.select.clear();
            }
            return Vec::new();
        }
        let start_row = start
            .and_then(|name| self.row(name))
            .or_else(|| self.default_row())
            .map(|r| r.name.clone());
        let mut used: BTreeSet<String> = self.options.iter().cloned().collect();
        let mut out = Vec::new();
        for selector in std::mem::take(&mut self.selectors) {
            let mut option = selector.clone();
            let mut n = 2;
            while taken(&option) || used.contains(&option) || !valid_name(&option) {
                let base = if valid_name(&selector) {
                    selector.clone()
                } else {
                    "Option".to_owned()
                };
                option = format!("{base}{n}");
                n += 1;
            }
            used.insert(option.clone());
            let text = start_row
                .as_deref()
                .and_then(|name| self.row(name))
                .and_then(|row| row.select.get(&selector).cloned())
                .unwrap_or_default();
            for row in &mut self.rows {
                if let Some(value) = row.select.get(&selector) {
                    row.values.insert(option.clone(), text_literal(value));
                }
            }
            self.options.push(option.clone());
            out.push(SelectorMigration {
                selector,
                option,
                text,
            });
        }
        for row in &mut self.rows {
            // A value for a selector the table did not list stays a problem.
            let stray: Vec<String> = row.select.keys().cloned().collect();
            for key in stray {
                if !out.iter().any(|m| m.selector == key) {
                    let value = row.select[&key].clone();
                    row.values.insert(key, text_literal(&value));
                }
            }
            row.select.clear();
        }
        self.closed = true;
        out
    }

    /// What is wrong with the table for a design with these parameters,
    /// one message per problem; empty when it is right. Values are checked
    /// by setting them on a copy of the parameters, so a row that would
    /// make a cycle or a unit mismatch is reported too. The rules'
    /// problems follow ([`Configurations::rule_problems`]).
    pub fn problems(&self, parameters: &Parameters) -> Vec<String> {
        let mut out = Vec::new();
        if self.is_empty() {
            return out;
        }
        if self.rows.is_empty() && self.closed {
            out.push("the configuration table has no rows".to_owned());
        }
        if self.rows.is_empty() && self.options.is_empty() && self.rules.is_empty() {
            out.push("the configuration table has no rows".to_owned());
        }
        let mut options = BTreeSet::new();
        for option in &self.options {
            if option.trim().is_empty() {
                out.push("an option has no name".to_owned());
            } else if !options.insert(option.as_str()) {
                out.push(format!("option {option} is in the table twice"));
            } else {
                match parameters.find(option).and_then(|id| parameters.get(id)) {
                    None => out.push(format!("option {option} of the table does not exist")),
                    Some(p) if p.kind() != ParamKind::User => out.push(format!(
                        "option {option} is not a user parameter (it is a {} parameter)",
                        p.kind()
                    )),
                    Some(_) => {}
                }
            }
        }
        let mut seen = BTreeSet::new();
        for name in &self.parameters {
            if !seen.insert(name) {
                out.push(format!("parameter {name} is in the table twice"));
            } else if options.contains(name.as_str()) {
                out.push(format!(
                    "parameter {name} of the table is also one of its options"
                ));
            } else if parameters.find(name).is_none() {
                out.push(format!("parameter {name} of the table does not exist"));
            }
        }
        if !self.closed && !self.parameters.is_empty() {
            out.push(format!(
                "the rows of an open table set options only, not {}: make the table closed",
                self.parameters.join(", ")
            ));
        }
        let mut names = BTreeSet::new();
        let mut combinations = BTreeSet::new();
        for (i, row) in self.rows.iter().enumerate() {
            let label = if row.name.trim().is_empty() {
                out.push(format!("row {} has no name", i + 1));
                format!("row {}", i + 1)
            } else {
                row.name.clone()
            };
            if !names.insert(row.name.as_str()) {
                out.push(format!("{label}: the name is used by another row"));
            }
            for (key, expression) in &row.values {
                if options.contains(key.as_str()) {
                    if Expr::parse(expression).is_ok_and(|e| !e.is_constant()) {
                        out.push(format!(
                            "{label}: the value of option {key} is not a constant: {expression}"
                        ));
                    }
                } else if !self.parameters.contains(key) {
                    out.push(format!(
                        "{label}: {key} is not {} of the table",
                        if self.closed {
                            "an option or a parameter"
                        } else {
                            "an option"
                        }
                    ));
                }
            }
            if self.closed && !self.options.is_empty() {
                let missing: Vec<&str> = self
                    .options
                    .iter()
                    .filter(|o| {
                        row.values
                            .get(*o)
                            .is_none_or(|v| v.trim().is_empty() || v.trim() == "''")
                    })
                    .map(String::as_str)
                    .collect();
                if !missing.is_empty() {
                    out.push(format!("{label}: no value for {}", missing.join(", ")));
                } else {
                    let combination: Vec<String> = self
                        .options
                        .iter()
                        .map(|o| squeeze(&row.values[o]))
                        .collect();
                    if !combinations.insert(combination) {
                        out.push(format!(
                            "{label}: another row has the same {}",
                            self.options.join(" and ")
                        ));
                    }
                }
            }
            let mut trial = parameters.clone();
            if let Err(e) = apply_values(&mut trial, row) {
                out.push(format!("{label}: {e}"));
            }
        }
        if let Some(default) = &self.default
            && self.row(default).is_none()
        {
            out.push(format!("the default row {default} is not in the table"));
        }
        if out.is_empty() {
            out.extend(self.rule_problems(parameters));
        }
        out
    }

    /// The row of the table whose values the parameters have now, if any:
    /// each of its expressions is the parameter's (with decimal points and
    /// spaces as written), or gives the parameter's value.
    pub fn current(&self, parameters: &Parameters) -> Option<&ConfigurationRow> {
        self.rows
            .iter()
            .find(|row| {
                row.values.iter().all(|(name, expression)| {
                    parameters
                        .find(name)
                        .and_then(|id| parameters.get(id))
                        .is_some_and(|p| same_expression(p.expression(), expression))
                })
            })
            .or_else(|| self.matching_row(parameters))
    }
}

/// The text of a constant expression as a designation or a chooser shows
/// it: a text's text (`'M5'` gives `M5`), a number's number without its
/// unit, in millimetres or degrees for a length or an angle (`16 mm` gives
/// `16`); None for an expression that does not evaluate without
/// parameters.
pub fn constant_text(expression: &str) -> Option<String> {
    let expr = Expr::parse(expression).ok()?;
    let context = crate::parameters::default_context();
    match expr.eval_value(&(), &context).ok()? {
        crate::expr::Value::Text(text) => Some(text),
        crate::expr::Value::Number(q) => {
            let value = crate::expr::Unit::display_for(q.dims)
                .map_or(q.value, |unit| unit.canonical_to_unit(q.value));
            Some(format_number(value, DEFAULT_DECIMALS))
        }
    }
}

/// Whether a parameter has the value an expression gives it (a text by its
/// text, a number within a billionth).
fn has_value(parameters: &Parameters, name: &str, expression: &str) -> bool {
    let Some(id) = parameters.find(name) else {
        return false;
    };
    let Some(p) = parameters.get(id) else {
        return false;
    };
    if same_expression(p.expression(), expression) {
        return true;
    }
    // A constant (an option's value) is evaluated without the others.
    if let Ok(expr) = Expr::parse(expression)
        && expr.is_constant()
    {
        let context = p.expression_context().unwrap_or(parameters.context());
        return match p.text() {
            Some(text) => expr.eval_text(&(), &context).is_ok_and(|t| t == text),
            None => expr.eval_as(&(), &context, p.unit()).is_ok_and(|q| {
                let (a, b) = (p.value(), q.value);
                (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
            }),
        };
    }
    let mut trial = parameters.clone();
    if trial.set_expression(id, expression).is_err() {
        return false;
    }
    let Some(q) = trial.get(id) else {
        return false;
    };
    match (p.text(), q.text()) {
        (Some(a), Some(b)) => a == b,
        (None, None) => {
            let (a, b) = (p.value(), q.value());
            (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
        }
        _ => false,
    }
}

fn valid_name(name: &str) -> bool {
    crate::expr::is_valid_name(name)
}

/// An expression without spaces, with decimal points: what two rows'
/// values are compared by.
fn squeeze(text: &str) -> String {
    crate::expr::with_decimal_points(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn same_expression(a: &str, b: &str) -> bool {
    squeeze(a) == squeeze(b)
}

/// Sets the row's expressions atomically (each parameter keeps its unit);
/// returns the names of the parameters whose final values changed.
#[doc(hidden)]
pub fn apply_values(
    parameters: &mut Parameters,
    row: &ConfigurationRow,
) -> Result<Vec<String>, String> {
    let mut expressions = Vec::with_capacity(row.values.len());
    for (name, expression) in &row.values {
        let id = parameters
            .find(name)
            .ok_or_else(|| format!("parameter {name} does not exist"))?;
        expressions.push((id, expression.as_str()));
    }
    let changed = parameters.set_expressions(&expressions).map_err(|e| {
        if let ParameterError::Table(
            TableError::Parse { parameter, .. }
            | TableError::Eval { parameter, .. }
            | TableError::Limit { parameter, .. },
        ) = &e
            && let Some(expression) = row.values.get(parameter)
        {
            return format!("{parameter} = {expression}: {e}");
        }
        e.to_string()
    })?;
    let changed: BTreeSet<String> = changed.iter().map(|id| parameters.name(*id)).collect();
    Ok(changed.into_iter().collect())
}

/// Natural order of texts with numbers in them: `M2.5` < `M3` < `M10`,
/// `8` < `16` < `100`; otherwise case-insensitive, then exact.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn chunks(text: &str) -> Vec<Result<f64, String>> {
        let mut out = Vec::new();
        let mut current = String::new();
        let mut numeric = false;
        let flush = |current: &mut String, numeric: bool, out: &mut Vec<Result<f64, String>>| {
            if current.is_empty() {
                return;
            }
            let chunk = std::mem::take(current);
            match chunk.parse::<f64>() {
                Ok(value) if numeric => out.push(Ok(value)),
                _ => out.push(Err(chunk.to_lowercase())),
            }
        };
        for c in text.chars() {
            let digit = c.is_ascii_digit() || (c == '.' && numeric);
            if digit != numeric {
                flush(&mut current, numeric, &mut out);
                numeric = digit;
            }
            current.push(c);
        }
        flush(&mut current, numeric, &mut out);
        out
    }
    let (x, y) = (chunks(a), chunks(b));
    for (p, q) in x.iter().zip(&y) {
        let order = match (p, q) {
            (Ok(p), Ok(q)) => p.partial_cmp(q).unwrap_or(Ordering::Equal),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(p), Err(q)) => p.cmp(q),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_sort_naturally() {
        let mut sizes = vec!["M10", "M2.5", "M3", "M12", "M4"];
        sizes.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(sizes, ["M2.5", "M3", "M4", "M10", "M12"]);
        let mut lengths = vec!["100", "16", "8", "20"];
        lengths.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(lengths, ["8", "16", "20", "100"]);
    }
}
