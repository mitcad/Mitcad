// SPDX-License-Identifier: MIT
//! Options of inserted components (mitcad#801; `docs/configurations.md`,
//! "Inserted components"; `commands.md`, "Options of inserted
//! components").
//!
//! A component inserted from a library or from a linked design with
//! `parameters`, and every BUILD part, is made by a derived part
//! (`features/derived_part.rs`): its source's design is evaluated in a
//! configuration row with the `parameters`, each an expression of the
//! design's own parameters. The options belong to the component, so all
//! of its occurrences share them; a second configuration is a second
//! component.
//!
//! The source's rules hold. The cascade (mitcad#799) moves the
//! component's **constant** values and reports the moves. A value
//! **bound** to the design's parameters (its expression names one) is
//! never moved: when it breaks a rule the derived part fails with the
//! rule's name, as a failed feature does, and the design's own change
//! goes through. The parts list names the combination by its row, else by
//! the library's designation pattern with the option values.
//!
//! Everything the model reads of a source's configurations for this (the
//! cascade, the combination's row, the option values of a designation and
//! the choosers' option reports) goes through this module.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::build::LibrarySourceDef;
use crate::configurations::{ConfigurationMove, Domain, DomainValue, OptionValue, RuleViolation};
use crate::document::DocState;
use crate::expr::{DEFAULT_DECIMALS, Expr, Unit, format_value, text_literal, value_to_expression};
use crate::features::derived_part::DerivedPartDef;
use crate::features::{FeatureDef, ValueInput};
use crate::ids::FeatureUid;
use crate::parameters::{ParamId, Parameters};

/// A value a derived part gives a parameter of its source: a number in
/// millimetres or radians, or a text.
#[derive(Debug, Clone, PartialEq)]
pub enum PartValue {
    Number(f64),
    Text(String),
}

/// A constant value of a component's option that the cascade moved, and
/// the rule that moved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionMove {
    pub option: String,
    /// The values as text: `1500 mm`, `Black`.
    pub from: String,
    pub to: String,
    /// The constants it moved from and to (`1000 mm`, `'White'`).
    pub from_expression: String,
    pub to_expression: String,
    pub rule: String,
    /// `Length 1500 mm → 1000 mm: Black is short`.
    pub text: String,
}

impl OptionMove {
    /// The move as the answers list it (as the `configure` command's
    /// `moves`).
    pub fn json(&self) -> Value {
        json!({
            "option": self.option, "from": self.from, "to": self.to,
            "from_expression": self.from_expression, "to_expression": self.to_expression,
            "rule": self.rule, "text": self.text,
        })
    }
}

/// A rule a component's values break: its name, what is wrong and both in
/// words (`the rule Black is short: …`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenRule {
    pub rule: String,
    pub message: String,
    pub text: String,
}

impl BrokenRule {
    /// As the `configure` answers' `refused` gives it.
    pub fn json(&self) -> Value {
        json!({"rule": self.rule, "message": self.message, "text": self.text})
    }
}

impl From<&RuleViolation> for BrokenRule {
    fn from(v: &RuleViolation) -> Self {
        Self {
            rule: v.rule.clone(),
            message: v.message.clone(),
            text: v.text(),
        }
    }
}

impl From<&ConfigurationMove> for OptionMove {
    fn from(m: &ConfigurationMove) -> Self {
        Self {
            option: m.option.clone(),
            from: m.from.clone(),
            to: m.to.clone(),
            from_expression: m.from_expression.clone(),
            to_expression: m.to_expression.clone(),
            rule: m.rule.clone(),
            text: m.text(),
        }
    }
}

/// A source's design in a row with a component's values set and the
/// cascade run.
#[derive(Debug, Clone)]
pub struct ConfiguredSource {
    pub state: DocState,
    /// The constants the cascade moved.
    pub moves: Vec<OptionMove>,
    /// The first rule the values still break (a pinned value that breaks a
    /// rule is not moved).
    pub broken: Option<BrokenRule>,
    /// The rules the source's own values broke before the component's
    /// values were set (a library's problem, not the component's).
    pub broken_before: Vec<BrokenRule>,
}

/// The source's design in the row `config` (else its default row) with
/// `values` set and the source's cascade run: the `pinned` values (those
/// bound to the design's parameters) are never moved, the others may be.
/// Its bound values (a feature suppressed by a condition, an appearance
/// that follows a text) then follow the parameters.
pub fn configure(
    source: &LibrarySourceDef,
    config: Option<&str>,
    values: &[(String, PartValue)],
    pinned: &BTreeSet<String>,
) -> Result<ConfiguredSource, String> {
    let design = source.design.state()?;
    configure_state(&design, &source.label(), config, values, pinned)
}

/// [`configure`] on a design's state (`label` names it in messages).
pub fn configure_state(
    design: &DocState,
    label: &str,
    config: Option<&str>,
    values: &[(String, PartValue)],
    pinned: &BTreeSet<String>,
) -> Result<ConfiguredSource, String> {
    let mut state = design.clone();
    let table = state.configurations.clone();
    let row = match config {
        Some(name) => Some(
            table
                .row(name)
                .cloned()
                .ok_or_else(|| format!("{label} has no configuration {name}"))?,
        ),
        None => table.default_row().cloned(),
    };
    if let Some(row) = &row {
        mitcad_model_base::internal::configurations::apply_values(&mut state.parameters, row)
            .map_err(|e| format!("{label} {}: {e}", row.name))?;
    }
    let broken_before = violations(&state)?;
    set_values(&mut state.parameters, label, values)?;
    let (moves, broken) = cascade(&mut state, pinned)?;
    // The source's bindings follow its parameters (mitcad#800), as they do
    // in the design itself: a row that rounds a steel tube's inner corners
    // suppresses its sharp bore (mitcad#873).
    state.settle_bindings();
    // A rule the source broke in its row already is not the values' fault.
    let broken = broken
        .into_iter()
        .find(|b| !broken_before.iter().any(|before| before.rule == b.rule));
    Ok(ConfiguredSource {
        state,
        moves,
        broken,
        broken_before,
    })
}

/// The rules a design's parameters break now.
fn violations(state: &DocState) -> Result<Vec<BrokenRule>, String> {
    let table = &state.configurations;
    if !table.enforces() {
        return Ok(Vec::new());
    }
    Ok(table
        .violations(&state.parameters)?
        .iter()
        .map(BrokenRule::from)
        .collect())
}

/// Sets the source's parameters to the values, in one step: a number is
/// written in the parameter's unit, a text as a text.
fn set_values(
    parameters: &mut Parameters,
    label: &str,
    values: &[(String, PartValue)],
) -> Result<(), String> {
    let mut expressions = Vec::with_capacity(values.len());
    for (name, value) in values {
        let id = parameters
            .find(name)
            .ok_or_else(|| format!("{label} has no parameter {name}"))?;
        let param = parameters.get(id).expect("found");
        let expression = match value {
            PartValue::Number(number) if param.unit().is_text() => {
                return Err(format!(
                    "{name} of {label} is a text, not the number {number}"
                ));
            }
            PartValue::Number(number) if !number.is_finite() => {
                return Err(format!("{name}: the value {number} is not a finite number"));
            }
            PartValue::Number(number) => value_to_expression(*number, param.unit()),
            PartValue::Text(text) if !param.unit().is_text() => {
                return Err(format!(
                    "{name} of {label} is a number, not the text '{text}'"
                ));
            }
            PartValue::Text(text) => text_literal(text),
        };
        expressions.push((id, expression));
    }
    let expressions: Vec<(ParamId, &str)> = expressions
        .iter()
        .map(|(id, e)| (*id, e.as_str()))
        .collect();
    parameters
        .set_expressions(&expressions)
        .map_err(|e| format!("{label}: {e}"))?;
    Ok(())
}

/// Runs the source's cascade (mitcad#799) when its table has rules or is
/// closed: the moves of the constants, and the rules still broken.
fn cascade(
    state: &mut DocState,
    pinned: &BTreeSet<String>,
) -> Result<(Vec<OptionMove>, Vec<BrokenRule>), String> {
    let table = state.configurations.clone();
    if !table.enforces() {
        return Ok((Vec::new(), Vec::new()));
    }
    let done = crate::configurations::cascade(&mut state.parameters, &table, pinned)?;
    let moves = done.moves.iter().map(OptionMove::from).collect();
    let broken = done.violations.iter().map(BrokenRule::from).collect();
    Ok((moves, broken))
}

/// Whether a derived part's value (the parameter `id` of its slot) is
/// bound to the design's parameters: a parameter of the design itself, or
/// the part's own whose expression names another. A constant is not.
pub fn is_bound(params: &Parameters, id: ParamId, owner: FeatureUid) -> bool {
    if params.owner(id) != Some(owner) {
        return true;
    }
    params
        .get(id)
        .is_some_and(|p| !Expr::parse(p.expression()).is_ok_and(|e| e.is_constant()))
}

/// A derived part's values as the design's parameters give them now, and
/// the names of the bound ones ([`is_bound`]).
pub fn part_values(
    def: &DerivedPartDef,
    params: &Parameters,
    owner: FeatureUid,
) -> (Vec<(String, PartValue)>, BTreeSet<String>) {
    let mut values = Vec::with_capacity(def.parameters.len());
    let mut pinned = BTreeSet::new();
    for (name, id) in &def.parameters {
        let value = match params.text(*id) {
            Some(text) => PartValue::Text(text.to_owned()),
            None => match params.value(*id) {
                Some(value) => PartValue::Number(value),
                None => continue,
            },
        };
        if is_bound(params, *id, owner) {
            pinned.insert(name.clone());
        }
        values.push((name.clone(), value));
    }
    (values, pinned)
}

/// The units of a derived part's `parameters.<name>` slots that are not
/// lengths: those of its source's parameters (a text, an angle, a count),
/// for resolving its values.
pub(crate) fn slot_units(state: &DocState, def: &FeatureDef<ValueInput>) -> BTreeMap<String, Unit> {
    let FeatureDef::DerivedPart(part) = def else {
        return BTreeMap::new();
    };
    let Some(design) = state
        .library_sources
        .get(part.source)
        .and_then(|s| s.design.state().ok())
    else {
        return BTreeMap::new();
    };
    part.parameters
        .keys()
        .filter_map(|name| {
            let p = design.parameters.get(design.parameters.find(name)?)?;
            let unit = p.unit();
            let length = !unit.is_text() && unit.dims() == Unit::MM.dims();
            (!length).then(|| (format!("parameters.{name}"), unit))
        })
        .collect()
}

/// How a library names its parts: its designation pattern (`ISO 4762
/// M{Size}x{Length}`; empty: none), its standard and the component's name,
/// and the pattern used without one when a row matches (`{standard}
/// {config}`).
#[derive(Debug, Clone, Copy)]
pub struct Naming<'a> {
    pub pattern: &'a str,
    pub standard: &'a str,
    pub name: &'a str,
    pub fallback: &'a str,
    /// The manifest's blocks of the rows (mitcad#949: their standards),
    /// when the library gives them, and the standard asked for (the
    /// Standard filter's; empty: each row's first).
    pub rows: Option<&'a BTreeMap<String, crate::library::LibraryRow>>,
    pub chosen: &'a str,
}

/// What a parts list says of a combination of options: its row, and the
/// designation, part number and product page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Combination {
    pub row: Option<String>,
    pub designation: String,
    /// The row's own, if it has one.
    pub part_number: Option<String>,
    pub supplier_url: Option<String>,
}

/// The combination a configured source's state stands for: its row (the
/// row `start` it was made in, else the default row, when the parameters
/// have its values, else the first row that matches), the row's own
/// designation, else the pattern with
/// `{standard}`, `{name}`, `{config}`, each option (`{Size}`: a text's
/// text, a number in its unit without the unit) and the `numbers` (`{L}`,
/// in millimetres) replaced. Without a pattern: `fallback` when a row
/// matches, else the name or standard followed by the option values.
pub fn combination(
    state: &DocState,
    start: Option<&str>,
    naming: &Naming<'_>,
    numbers: &BTreeMap<String, f64>,
) -> Combination {
    let table = &state.configurations;
    let start = start.or_else(|| table.default_row().map(|r| r.name.as_str()));
    let row = table.combination_row(&state.parameters, start);
    let mut out = Combination {
        row: row.map(|r| r.name.clone()),
        part_number: row.and_then(|r| r.part_number.clone()),
        supplier_url: row.and_then(|r| r.supplier_url.clone()),
        ..Combination::default()
    };
    if let Some(own) = row.and_then(|r| r.designation.clone()) {
        out.designation = own;
        return out;
    }
    let pattern = if !naming.pattern.trim().is_empty() {
        naming.pattern.to_owned()
    } else if row.is_some() || table.options.is_empty() {
        naming.fallback.to_owned()
    } else {
        let mut pattern = if naming.standard.trim().is_empty() {
            "{name}".to_owned()
        } else {
            "{standard}".to_owned()
        };
        for option in &table.options {
            pattern.push_str(&format!(" {{{option}}}"));
        }
        pattern
    };
    // A row tagged with standards is named by one of them (mitcad#949).
    let named = row
        .zip(naming.rows)
        .and_then(|(r, rows)| rows.get(&r.name))
        .and_then(|r| r.named_standard(naming.chosen));
    let (pattern, non_standard) =
        crate::library::standard_in_pattern(&pattern, naming.standard, named);
    let mut text = pattern
        .replace("{name}", naming.name)
        .replace("{config}", row.map_or("", |r| r.name.as_str()));
    for (option, value) in table.designation_values(&state.parameters) {
        text = text.replace(&format!("{{{option}}}"), &value);
    }
    for (name, value) in numbers {
        text = text.replace(&format!("{{{name}}}"), &crate::build::round(*value));
    }
    if non_standard {
        text.push_str(&format!(" ({})", crate::library::NON_STANDARD));
    }
    out.designation = text.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

/// The options of a design for the pickers, in choice order (the shape of
/// the `configure` query's `options`, mitcad#799, `model-api`'s
/// `configurations.rs`): each with its `name`,
/// `kind` (`number` or `text`), `unit`, `value`, `text`, `expression`,
/// `driven`, `domain`, `allowed` (given the options before it) and
/// `restricted_by`. With a component's `parameters` (its expressions, by
/// option), each option also has `parameter` (the expression or null)
/// and `bound` (whether it names the design's parameters). Empty when the
/// design has no options or they cannot be evaluated.
pub fn options_json(state: &DocState, parameters: &BTreeMap<String, String>) -> Value {
    let table = &state.configurations;
    if table.options.is_empty() {
        return json!([]);
    }
    let Ok(reports) = table.option_reports(&state.parameters) else {
        return json!([]);
    };
    let options: Vec<Value> = reports
        .iter()
        .map(|r| {
            let unit = r.unit;
            let mut option = json!({
                "name": r.name,
                "kind": if unit.is_text() { "text" } else { "number" },
                "unit": unit.to_string(),
                "value": value_json(&r.value),
                "text": r.text,
                "expression": r.expression,
                "driven": r.driven,
                "domain": domain_json(&r.domain, unit),
                "allowed": domain_json(&r.allowed, unit),
                "restricted_by": r.restricted_by,
            });
            if !parameters.is_empty() {
                let given = parameters.get(&r.name);
                let bound = given.is_some_and(|e| !Expr::parse(e).is_ok_and(|e| e.is_constant()));
                option["parameter"] = json!(given);
                option["bound"] = json!(bound);
                // A value bound to the design's parameters is not chosen here.
                if bound {
                    option["driven"] = json!(true);
                }
            }
            option
        })
        .collect();
    Value::Array(options)
}

/// What the choosers of a library part or an inserted component show: the
/// `configure` query's answer (mitcad#799: `options`, `row`,
/// `designation_values`, `moves`, `refused`, `problems`) for a source's
/// design in the row `config` with `values` tried as [`configure_state`]
/// sets them (the `pinned` ones are not moved; one that breaks a rule is
/// `refused`). `parameters` are a component's expressions by option (its
/// bound ones are shown driven, [`options_json`]); with `naming` the
/// answer also has the combination's `designation`. Nothing is changed.
pub fn configure_json(
    design: &DocState,
    label: &str,
    config: Option<&str>,
    values: &[(String, PartValue)],
    pinned: &BTreeSet<String>,
    parameters: &BTreeMap<String, String>,
    naming: Option<&Naming<'_>>,
) -> Result<Value, String> {
    let configured = configure_state(design, label, config, values, pinned)?;
    let state = &configured.state;
    let table = &state.configurations;
    let mut problems = table.problems(&state.parameters);
    if table.enforces() {
        for v in table.violations(&state.parameters)? {
            let text = v.text();
            if !problems.contains(&text) {
                problems.push(text);
            }
        }
    }
    let start = config.or_else(|| table.default_row().map(|r| r.name.as_str()));
    let mut answer = json!({
        "options": options_json(state, parameters),
        "row": table.combination_row(&state.parameters, start).map(|r| r.name.clone()),
        "designation_values": table.designation_values(&state.parameters),
        "moves": configured.moves.iter().map(OptionMove::json).collect::<Vec<_>>(),
        "refused": configured.broken.as_ref().map_or(Value::Null, BrokenRule::json),
        "problems": problems,
    });
    if let Some(naming) = naming {
        answer["designation"] =
            json!(combination(state, config, naming, &BTreeMap::new()).designation);
    }
    Ok(answer)
}

/// A value for the parameter `name` of a design as a command gives it: a
/// number (millimetres or radians), a text option's text, or a constant
/// expression in the parameter's unit (`800 mm`).
pub fn command_value(
    parameters: &Parameters,
    name: &str,
    value: &Value,
) -> Result<PartValue, String> {
    let id = parameters
        .find(name)
        .ok_or_else(|| format!("there is no parameter {name}"))?;
    let text = parameters.get(id).is_some_and(|p| p.unit().is_text());
    match value {
        Value::Number(n) => n
            .as_f64()
            .map(PartValue::Number)
            .ok_or_else(|| format!("{name}: not a number")),
        Value::String(s) if text && !s.trim_start().starts_with('\'') => {
            Ok(PartValue::Text(s.clone()))
        }
        Value::String(s) => {
            let mut trial = parameters.clone();
            trial
                .set_expression(id, s)
                .map_err(|e| format!("{name} = {s}: {e}"))?;
            Ok(match trial.text(id) {
                Some(t) => PartValue::Text(t.to_owned()),
                None => PartValue::Number(trial.value(id).unwrap_or(f64::NAN)),
            })
        }
        _ => Err(format!(
            "{name}: a value is a text, a number or an expression"
        )),
    }
}

fn value_json(value: &OptionValue) -> Value {
    match value {
        OptionValue::Number(n) => json!(n),
        OptionValue::Text(t) | OptionValue::Expression(t) => json!(t),
    }
}

fn domain_json(domain: &Domain, unit: Unit) -> Value {
    let text = |v: &Option<f64>| v.map(|v| format_value(v, unit, DEFAULT_DECIMALS));
    match domain {
        Domain::Free => json!({"kind": "free"}),
        Domain::List(values) => {
            let values: Vec<Value> = values
                .iter()
                .map(|v: &DomainValue| {
                    json!({"value": value_json(&v.value), "text": v.text,
                           "expression": v.expression})
                })
                .collect();
            json!({"kind": "list", "values": values})
        }
        Domain::Range { min, max, step } => json!({
            "kind": "range", "min": min, "max": max, "step": step,
            "min_text": text(min), "max_text": text(max), "step_text": text(step),
        }),
    }
}
