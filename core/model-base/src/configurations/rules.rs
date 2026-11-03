// SPDX-License-Identifier: MIT
//! Configuration rules and the cascade (mitcad#799; `docs/configurations.md`,
//! "Rules" and "The cascade").
//!
//! Rules only narrow: an option's allowed values are its domain (its
//! parameter's `values`, else its `min`, `max` and `step`, else free)
//! intersected with every active restriction. Rules do not take part in
//! evaluation: they check the evaluated parameters and change only the
//! constants of options, so they add no dependency edges and cannot form a
//! cycle. After a change the cascade goes through the options in order and
//! moves each one that is not allowed to the nearest allowed value; an
//! option the change set, and a driven option, is never moved.

use std::collections::{BTreeMap, BTreeSet};

use super::from_tables::tables_of;
use super::{ConfigurationRule, Configurations, RuleAction};
use crate::expr::{
    DEFAULT_DECIMALS, ParamId, Unit, format_value, text_literal, value_to_expression,
};
use crate::parameters::Parameters;

/// A value of an option: a number in canonical units (millimetres,
/// radians) or a text; as input also a constant expression.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Number(f64),
    Text(String),
    /// A constant expression in the option's unit (`800 mm`, `'Black'`):
    /// input only.
    Expression(String),
}

/// An allowed or listed value of an option.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainValue {
    /// A number in canonical units, or a text.
    pub value: OptionValue,
    /// For people: `800 mm`, `Black`.
    pub text: String,
    /// The constant that chooses it: `800 mm`, `'Black'`.
    pub expression: String,
}

/// What an option can be: a list of values, a range (canonical units, a
/// bound left out when None), or anything.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Domain {
    #[default]
    Free,
    List(Vec<DomainValue>),
    Range {
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
    },
}

/// An option moved by the cascade, and the rule that moved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationMove {
    pub option: String,
    /// The values as text: `1500 mm`, `Black`.
    pub from: String,
    pub to: String,
    pub from_expression: String,
    pub to_expression: String,
    /// The rule (or `the configuration table` for a closed table's rows,
    /// or the option's own `values` limit).
    pub rule: String,
}

impl ConfigurationMove {
    /// `Length 1500 mm → 1000 mm: Black is short`.
    pub fn text(&self) -> String {
        format!("{} {} → {}: {}", self.option, self.from, self.to, self.rule)
    }
}

/// A rule the parameters break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleViolation {
    /// The rule's name (`the configuration table` for a closed table).
    pub rule: String,
    /// What is wrong: the rule's message, or what it does not allow
    /// (`Length = 1500 mm`).
    pub message: String,
}

impl RuleViolation {
    /// The refusal in words: `the rule Black is short: does not allow
    /// Length = 1500 mm`; an empty intersection's message, which names its
    /// rules, alone.
    pub fn text(&self) -> String {
        if self.message.starts_with(&format!("{} leave ", self.rule)) {
            self.message.clone()
        } else if self.rule == CLOSED_TABLE {
            format!("{CLOSED_TABLE}: {}", self.message)
        } else {
            format!("the rule {}: {}", self.rule, self.message)
        }
    }
}

/// A problem of a rule with its place in the rule (mitcad#1064), for the
/// block editor to mark the block that is wrong
/// ([`Configurations::placed_rule_problems`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleProblem {
    /// The rule's index in the table's rules.
    pub index: usize,
    /// The rule's name (empty for a rule without one).
    pub rule: String,
    /// Where in the rule: `when`; an action of its then or else as
    /// `then.0` or `else.1`, with the part of it when there is one:
    /// `then.0.option`, `then.0.values.2`, `then.0.exclude.0`, `then.0.min`,
    /// `then.0.max`, `then.0.step`, `then.0.require`. None for the rule
    /// itself (its name).
    pub place: Option<String>,
    /// A warning (a value outside its option's domain) does not refuse the
    /// table: it is none of [`Configurations::rule_problems`].
    pub warning: bool,
    /// The problem in words, as [`Configurations::rule_problems`] gives it.
    pub message: String,
}

/// The name a closed table's restriction goes by in moves, refusals and
/// `restricted_by`: the Rows tab's checkbox that closes it (mitcad#968).
pub const CLOSED_TABLE: &str = "Only these rows";

/// What the cascade did to the parameters: the moves, in option order,
/// and the rules still broken after them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cascade {
    pub moves: Vec<ConfigurationMove>,
    pub violations: Vec<RuleViolation>,
}

/// An option as the choosers show it (the `configure` query).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionReport {
    pub name: String,
    pub unit: Unit,
    /// Its value: a number in canonical units, or a text.
    pub value: OptionValue,
    pub text: String,
    pub expression: String,
    /// Its expression is a formula: it is not chosen and never moved.
    pub driven: bool,
    pub domain: Domain,
    /// Its domain narrowed by the rules, given the options before it.
    pub allowed: Domain,
    /// The rules that narrow it now.
    pub restricted_by: Vec<String>,
}

/// What `configure` did ([`ConfigureOutcome::refused`] set: nothing).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigureOutcome {
    /// Parameters whose values changed.
    pub changed: Vec<String>,
    pub moves: Vec<ConfigurationMove>,
    /// The combination's row.
    pub row: Option<String>,
    pub refused: Option<RuleViolation>,
}

/// The `configure` query: each option as the choosers show it after the
/// cascade of the values tried.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigureReport {
    pub options: Vec<OptionReport>,
    pub row: Option<String>,
    pub designation_values: BTreeMap<String, String>,
    pub moves: Vec<ConfigurationMove>,
    pub refused: Option<RuleViolation>,
    pub problems: Vec<String>,
}

/// Combinations of earlier options that leave an option no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyIntersection {
    pub option: String,
    pub rules: Vec<String>,
    /// The earlier options' values as text.
    pub values: BTreeMap<String, String>,
}

/// The report of Check Rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RulesReport {
    /// How many combinations the finite domains make; None when a domain
    /// is not finite.
    pub combinations: Option<u64>,
    pub checked: u64,
    pub sampled: bool,
    /// (option, value as text) the cascade never keeps.
    pub never_allowed: Vec<(String, String)>,
    pub never_applies: Vec<String>,
    pub empty: Vec<EmptyIntersection>,
    /// (row, the rule it breaks).
    pub rows: Vec<(String, RuleViolation)>,
    /// (the combination's values as text, the rule), at most 20.
    pub unrepairable: Vec<(BTreeMap<String, String>, RuleViolation)>,
    pub problems: Vec<String>,
    pub lines: Vec<String>,
}

/// Within rounding: a billionth, relative (at least 1e-9 in mm or rad).
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// An evaluated value of an option: a number in canonical units or a text.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    Num(f64),
    Text(String),
}

impl Val {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Num(a), Self::Num(b)) => near(*a, *b),
            (Self::Text(a), Self::Text(b)) => a == b,
            _ => false,
        }
    }

    /// For people: `800 mm`, `Black`.
    fn text(&self, unit: Unit) -> String {
        match self {
            Self::Num(v) => format_value(*v, unit, DEFAULT_DECIMALS),
            Self::Text(text) => text.clone(),
        }
    }

    /// The constant that chooses it: `800 mm`, `'Black'`.
    fn expression(&self, unit: Unit) -> String {
        match self {
            Self::Num(v) => value_to_expression(*v, unit),
            Self::Text(text) => text_literal(text),
        }
    }

    fn option_value(&self) -> OptionValue {
        match self {
            Self::Num(v) => OptionValue::Number(*v),
            Self::Text(text) => OptionValue::Text(text.clone()),
        }
    }

    fn domain_value(&self, unit: Unit) -> DomainValue {
        DomainValue {
            value: self.option_value(),
            text: self.text(unit),
            expression: self.expression(unit),
        }
    }
}

/// An option's value as the cascade compares it; None for an expression.
fn val_of(value: &OptionValue) -> Option<Val> {
    match value {
        OptionValue::Number(v) => Some(Val::Num(*v)),
        OptionValue::Text(text) => Some(Val::Text(text.clone())),
        OptionValue::Expression(_) => None,
    }
}

/// An option as the cascade sees it.
#[derive(Debug, Clone)]
struct Opt {
    name: String,
    id: ParamId,
    unit: Unit,
    text: bool,
    /// Its expression is a formula.
    driven: bool,
    value: Val,
}

fn option_info(parameters: &Parameters, name: &str) -> Result<Opt, String> {
    let id = parameters
        .find(name)
        .ok_or_else(|| format!("option {name} does not exist"))?;
    let p = parameters
        .get(id)
        .ok_or_else(|| format!("option {name} does not exist"))?;
    Ok(Opt {
        name: name.to_owned(),
        id,
        unit: p.unit(),
        text: p.is_text(),
        driven: !p.expr().is_constant(),
        value: match p.text() {
            Some(text) => Val::Text(text.to_owned()),
            None => Val::Num(p.value()),
        },
    })
}

/// Evaluates an expression of an option's value (a rule's value, a row's
/// constant) against the parameters, in the option's unit.
fn eval_val(parameters: &Parameters, opt: &Opt, expression: &str) -> Result<Val, String> {
    let table = parameters.table();
    if opt.text {
        table
            .evaluate_text(expression)
            .map(Val::Text)
            .map_err(|e| format!("{expression}: {e}"))
    } else {
        table
            .evaluate(expression, opt.unit)
            .map(|q| Val::Num(q.value))
            .map_err(|e| format!("{expression}: {e}"))
    }
}

/// What one source (a rule, a closed table) allows an option.
#[derive(Debug, Clone, Default)]
struct Restriction {
    rule: String,
    only: Option<Vec<Val>>,
    exclude: Vec<Val>,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
}

/// The values an option may take: a list, or a range with steps, less
/// excluded values. Steps are counted from the minimum (from zero when
/// there is none), as the limits' step rule counts them.
#[derive(Debug, Clone, Default)]
struct Set {
    list: Option<Vec<Val>>,
    min: Option<f64>,
    max: Option<f64>,
    steps: Vec<f64>,
    excluded: Vec<Val>,
}

/// The most values of a range a chooser lists or Check Rules goes through.
const MAX_RANGE_VALUES: usize = 10_000;

impl Set {
    /// The option's domain: its parameter's `values`, else its `min`,
    /// `max` and `step`, else anything.
    fn domain(parameters: &Parameters, opt: &Opt) -> Self {
        let Some(p) = parameters.get(opt.id) else {
            return Self::default();
        };
        let list = p.allowed_values().map(|values| {
            values
                .iter()
                .map(|v| match v {
                    crate::expr::Value::Number(q) => Val::Num(q.value),
                    crate::expr::Value::Text(t) => Val::Text(t.clone()),
                })
                .collect()
        });
        let bounds = p.bounds();
        Self {
            list,
            min: bounds.min,
            max: bounds.max,
            steps: bounds.step.into_iter().collect(),
            excluded: Vec::new(),
        }
    }

    fn narrow(&mut self, r: &Restriction) {
        if let Some(only) = &r.only {
            self.list = Some(match self.list.take() {
                Some(list) => list
                    .into_iter()
                    .filter(|v| only.iter().any(|o| o.same(v)))
                    .collect(),
                None => only.clone(),
            });
        }
        self.excluded.extend(r.exclude.iter().cloned());
        if let Some(min) = r.min {
            self.min = Some(self.min.map_or(min, |m| m.max(min)));
        }
        if let Some(max) = r.max {
            self.max = Some(self.max.map_or(max, |m| m.min(max)));
        }
        if let Some(step) = r.step {
            self.steps.push(step);
        }
    }

    fn base(&self) -> f64 {
        self.min.unwrap_or(0.0)
    }

    fn contains(&self, value: &Val) -> bool {
        if let Some(list) = &self.list
            && !list.iter().any(|v| v.same(value))
        {
            return false;
        }
        if self.excluded.iter().any(|v| v.same(value)) {
            return false;
        }
        let Val::Num(v) = value else {
            return true;
        };
        if self.min.is_some_and(|min| *v < min && !near(*v, min))
            || self.max.is_some_and(|max| *v > max && !near(*v, max))
        {
            return false;
        }
        let base = self.base();
        self.steps
            .iter()
            .all(|&step| step <= 0.0 || near(base + ((v - base) / step).round() * step, *v))
    }

    /// Whether nothing is allowed.
    fn is_empty(&self) -> bool {
        if let (Some(min), Some(max)) = (self.min, self.max)
            && min > max
            && !near(min, max)
        {
            return true;
        }
        self.members().is_some_and(|m| m.is_empty())
    }

    /// The allowed values when they are finitely many: the list's, or a
    /// range's steps between its bounds (at most [`MAX_RANGE_VALUES`]).
    fn members(&self) -> Option<Vec<Val>> {
        if let Some(list) = &self.list {
            return Some(list.iter().filter(|v| self.contains(v)).cloned().collect());
        }
        let (Some(min), Some(max), Some(&step)) = (
            self.min,
            self.max,
            self.steps.iter().max_by(|a, b| a.total_cmp(b)),
        ) else {
            return None;
        };
        if step <= 0.0 || max < min {
            return Some(Vec::new());
        }
        let count = ((max - min) / step + 1e-9).floor() as usize + 1;
        if count > MAX_RANGE_VALUES {
            return None;
        }
        Some(
            (0..count)
                .map(|k| Val::Num(min + k as f64 * step))
                .filter(|v| self.contains(v))
                .collect(),
        )
    }

    /// The allowed value nearest to `value`: of a list the nearest number
    /// (the lower one of two as near), or for a text `prefer` when it is
    /// allowed, else the first; of a range the nearest step. None when
    /// nothing is allowed or none can be found.
    fn nearest(&self, value: &Val, prefer: Option<&Val>) -> Option<Val> {
        if self.list.is_some() {
            let members = self.members()?;
            return match value {
                Val::Num(v) => members
                    .iter()
                    .filter_map(|m| match m {
                        Val::Num(n) => Some(*n),
                        Val::Text(_) => None,
                    })
                    .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()).then(a.total_cmp(b)))
                    .map(Val::Num),
                Val::Text(_) => prefer
                    .filter(|p| members.iter().any(|m| m.same(p)))
                    .cloned()
                    .or_else(|| members.first().cloned()),
            };
        }
        let Val::Num(v) = value else {
            return prefer.filter(|p| self.contains(p)).cloned();
        };
        if self.is_empty() {
            return None;
        }
        let mut target = *v;
        if let Some(min) = self.min {
            target = target.max(min);
        }
        if let Some(max) = self.max {
            target = target.min(max);
        }
        let Some(&step) = self.steps.iter().max_by(|a, b| a.total_cmp(b)) else {
            return self.contains(&Val::Num(target)).then_some(Val::Num(target));
        };
        let base = self.base();
        // Halfway goes up, as the limits' step rule.
        let k = ((target - base) / step + 0.5 + 1e-9).floor();
        for distance in 0..1000_i32 {
            for sign in [0.0, -1.0, 1.0] {
                if distance == 0 && sign != 0.0 {
                    continue;
                }
                let candidate = Val::Num(base + (k + sign * f64::from(distance)) * step);
                if self.contains(&candidate) {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// As the report shows it.
    fn report(&self, unit: Unit) -> Domain {
        if self.list.is_some() {
            Domain::List(
                self.members()
                    .unwrap_or_default()
                    .iter()
                    .map(|v| v.domain_value(unit))
                    .collect(),
            )
        } else if self.min.is_some() || self.max.is_some() || !self.steps.is_empty() {
            Domain::Range {
                min: self.min,
                max: self.max,
                step: self.steps.iter().copied().max_by(|a, b| a.total_cmp(b)),
            }
        } else {
            Domain::Free
        }
    }
}

/// A row's value of an option, when it has one (a constant).
fn row_val(parameters: &Parameters, opt: &Opt, row: &super::ConfigurationRow) -> Option<Val> {
    eval_val(parameters, opt, row.values.get(&opt.name)?).ok()
}

impl Configurations {
    /// Whether the table has anything the cascade enforces: rules, or a
    /// closed table's options.
    pub fn enforces(&self) -> bool {
        !self.rules.is_empty() || (self.closed && !self.options.is_empty() && !self.rows.is_empty())
    }

    fn option_infos(&self, parameters: &Parameters) -> Result<Vec<Opt>, String> {
        self.options
            .iter()
            .map(|name| option_info(parameters, name))
            .collect()
    }

    /// What restricts option `i` now: a closed table's rows that have the
    /// options before it (as `opts` has them), and the active rules.
    fn restrictions(
        &self,
        parameters: &Parameters,
        opts: &[Opt],
        i: usize,
    ) -> Result<Vec<Restriction>, String> {
        let mut out = Vec::new();
        if self.closed && !self.rows.is_empty() {
            let mut only: Vec<Val> = Vec::new();
            for row in &self.rows {
                let earlier = opts[..i]
                    .iter()
                    .all(|o| row_val(parameters, o, row).is_none_or(|v| v.same(&o.value)));
                if !earlier {
                    continue;
                }
                if let Some(v) = row_val(parameters, &opts[i], row)
                    && !only.iter().any(|o| o.same(&v))
                {
                    only.push(v);
                }
            }
            out.push(Restriction {
                rule: CLOSED_TABLE.to_owned(),
                only: Some(only),
                ..Restriction::default()
            });
        }
        out.extend(self.rule_restrictions(parameters, &opts[i])?);
        Ok(out)
    }

    /// The option's domain narrowed by `restrictions`.
    fn allowed_set(parameters: &Parameters, opt: &Opt, restrictions: &[Restriction]) -> Set {
        let mut set = Set::domain(parameters, opt);
        for r in restrictions {
            set.narrow(r);
        }
        set
    }

    /// The rules (and a closed table's combinations) the parameters break.
    /// A closed table whose combination is no row is broken as
    /// [`CLOSED_TABLE`].
    pub fn violations(&self, parameters: &Parameters) -> Result<Vec<RuleViolation>, String> {
        let mut out: Vec<RuleViolation> = Vec::new();
        if !self.enforces() {
            return Ok(out);
        }
        let opts = self.option_infos(parameters)?;
        for i in 0..opts.len() {
            let opt = &opts[i];
            let restrictions: Vec<Restriction> = self
                .restrictions(parameters, &opts, i)?
                .into_iter()
                .filter(|r| r.rule != CLOSED_TABLE)
                .collect();
            if restrictions.is_empty() {
                continue;
            }
            let set = Self::allowed_set(parameters, opt, &restrictions);
            if set.is_empty() {
                let mut rules: Vec<&str> = restrictions.iter().map(|r| r.rule.as_str()).collect();
                rules.dedup();
                out.push(RuleViolation {
                    rule: rules.join(" and "),
                    message: format!("{} leave {} no value", rules.join(" and "), opt.name),
                });
                continue;
            }
            for r in &restrictions {
                let mut alone = Set::default();
                alone.narrow(r);
                if !alone.contains(&opt.value) && !out.iter().any(|v| v.rule == r.rule) {
                    out.push(RuleViolation {
                        rule: r.rule.clone(),
                        message: format!(
                            "does not allow {} = {}",
                            opt.name,
                            opt.value.text(opt.unit)
                        ),
                    });
                }
            }
        }
        out.extend(self.rule_checks(parameters)?);
        if self.closed && !self.rows.is_empty() && !opts.is_empty() {
            let matches = self.rows.iter().any(|row| {
                opts.iter()
                    .all(|o| row_val(parameters, o, row).is_some_and(|v| v.same(&o.value)))
            });
            if !matches {
                let combination: Vec<String> = opts
                    .iter()
                    .map(|o| format!("{} {}", o.name, o.value.text(o.unit)))
                    .collect();
                out.push(RuleViolation {
                    rule: CLOSED_TABLE.to_owned(),
                    message: format!("{} is no row", combination.join(" and ")),
                });
            }
        }
        Ok(out)
    }

    /// Each option as the choosers show it, with its allowed values given
    /// the options before it: a list's values are each tried through the
    /// cascade (the options before it kept), and allowed when the cascade
    /// breaks no rule that the parameters do not break already.
    pub fn option_reports(&self, parameters: &Parameters) -> Result<Vec<OptionReport>, String> {
        let opts = self.option_infos(parameters)?;
        let broken: Vec<String> = self
            .violations(parameters)?
            .into_iter()
            .map(|v| v.rule)
            .collect();
        let mut out = Vec::with_capacity(opts.len());
        for (i, opt) in opts.iter().enumerate() {
            let restrictions = self.restrictions(parameters, &opts, i)?;
            let domain = Set::domain(parameters, opt);
            let set = Self::allowed_set(parameters, opt, &restrictions);
            let allowed = if set.list.is_some() && self.enforces() {
                let pinned: BTreeSet<String> = opts[..=i].iter().map(|o| o.name.clone()).collect();
                let mut values = Vec::new();
                for v in set.members().unwrap_or_default() {
                    let mut trial = parameters.clone();
                    if trial
                        .set_expression(opt.id, &v.expression(opt.unit))
                        .is_err()
                    {
                        continue;
                    }
                    let Ok(done) = cascade(&mut trial, self, &pinned) else {
                        continue;
                    };
                    if done.violations.iter().all(|x| broken.contains(&x.rule)) {
                        values.push(v.domain_value(opt.unit));
                    }
                }
                Domain::List(values)
            } else {
                set.report(opt.unit)
            };
            let mut restricted_by: Vec<String> = Vec::new();
            for r in &restrictions {
                if !restricted_by.contains(&r.rule) {
                    restricted_by.push(r.rule.clone());
                }
            }
            let p = parameters.get(opt.id);
            out.push(OptionReport {
                name: opt.name.clone(),
                unit: opt.unit,
                value: opt.value.option_value(),
                text: opt.value.text(opt.unit),
                expression: p.map(|p| p.expression().to_owned()).unwrap_or_default(),
                driven: opt.driven,
                domain: domain.report(opt.unit),
                allowed,
                restricted_by,
            });
        }
        Ok(out)
    }

    /// The value of an option in the default row, when it has one.
    fn default_val(&self, parameters: &Parameters, opt: &Opt) -> Option<Val> {
        row_val(parameters, opt, self.default_row()?)
    }
}

/// Runs the cascade on `parameters`: each option of `table` in order that
/// is not allowed moves to the nearest allowed value, except the `pinned`
/// options (those the change set), the options before the first of them,
/// and driven ones. Returns the moves and
/// the rules still broken; an evaluation that fails is an error.
pub fn cascade(
    parameters: &mut Parameters,
    table: &Configurations,
    pinned: &BTreeSet<String>,
) -> Result<Cascade, String> {
    let mut done = Cascade::default();
    if !table.enforces() {
        return Ok(done);
    }
    // Only the options after the first one the change set move; all may
    // when the change set none (another parameter, a table edit).
    let first = table
        .options
        .iter()
        .position(|name| pinned.contains(name))
        .unwrap_or(0);
    for i in first..table.options.len() {
        let opts = table.option_infos(parameters)?;
        let opt = &opts[i];
        if pinned.contains(&opt.name) || opt.driven {
            continue;
        }
        let restrictions = table.restrictions(parameters, &opts, i)?;
        let set = Configurations::allowed_set(parameters, opt, &restrictions);
        if set.contains(&opt.value) {
            continue;
        }
        let prefer = table.default_val(parameters, opt);
        let Some(target) = set.nearest(&opt.value, prefer.as_ref()) else {
            continue;
        };
        let rule = restrictions
            .iter()
            .find(|r| {
                let mut alone = Set::default();
                alone.narrow(r);
                !alone.contains(&opt.value)
            })
            .map_or_else(
                || format!("the allowed values of {}", opt.name),
                |r| r.rule.clone(),
            );
        let to_expression = target.expression(opt.unit);
        let from_expression = parameters
            .get(opt.id)
            .map(|p| p.expression().to_owned())
            .unwrap_or_default();
        parameters
            .set_expression(opt.id, &to_expression)
            .map_err(|e| format!("{}: {e}", opt.name))?;
        done.moves.push(ConfigurationMove {
            option: opt.name.clone(),
            from: opt.value.text(opt.unit),
            to: target.text(opt.unit),
            from_expression,
            to_expression,
            rule,
        });
    }
    done.violations = table.violations(parameters)?;
    Ok(done)
}

/// Whether a condition holds: it gives a number, and zero is false. It is
/// read in parentheses, so that it may compare with a single `=`
/// (`Color = 'Black'`).
fn holds(parameters: &Parameters, condition: &str) -> Result<bool, String> {
    match parameters
        .table()
        .evaluate_value(&format!("({condition})"))
        .map_err(|e| format!("{condition}: {e}"))?
    {
        crate::expr::Value::Number(q) => Ok(q.value != 0.0),
        crate::expr::Value::Text(_) => Err(format!(
            "{condition}: a condition gives a number (0 is false), not a text"
        )),
    }
}

/// The actions of a rule that apply now: those of `then` when it is
/// enabled and its `when` holds (or is left out), those of `else` when it
/// does not; none for a disabled rule.
fn active_actions<'a>(
    rule: &'a ConfigurationRule,
    parameters: &Parameters,
) -> Result<&'a [RuleAction], String> {
    if !rule.enabled {
        return Ok(Default::default());
    }
    let applies = match &rule.when {
        None => true,
        Some(when) => {
            holds(parameters, when).map_err(|e| format!("the rule {}: when: {e}", rule.name))?
        }
    };
    Ok(if applies {
        rule.then.as_slice()
    } else {
        rule.otherwise.as_slice()
    })
}

/// Whether an action checks the combination (`require`, `forbid`).
fn is_check(action: &RuleAction) -> bool {
    action.require.is_some() || action.forbid.is_some()
}

impl Configurations {
    /// The restrictions of the active rules on an option (mitcad#799): the
    /// `values`, `exclude`, `min`, `max` and `step` of the actions that
    /// name it, and what an action of another kind allows
    /// ([`Configurations::other_action_allowed`]).
    fn rule_restrictions(
        &self,
        parameters: &Parameters,
        opt: &Opt,
    ) -> Result<Vec<Restriction>, String> {
        let mut out = Vec::new();
        for rule in &self.rules {
            for action in active_actions(rule, parameters)? {
                let at = |e: String| format!("the rule {}: {e}", rule.name);
                if action.is_restriction() {
                    if action.option.as_deref() != Some(opt.name.as_str()) {
                        continue;
                    }
                    let list = |texts: &Option<Vec<String>>| -> Result<Option<Vec<Val>>, String> {
                        texts
                            .as_ref()
                            .map(|texts| {
                                // A table column (mitcad#806).
                                if let Some(values) = super::from_tables::list_values(
                                    parameters,
                                    &tables_of(parameters),
                                    &opt.name,
                                    texts,
                                ) {
                                    return values.map(|values| {
                                        values.iter().filter_map(|v| val_of(&v.value)).collect()
                                    });
                                }
                                texts
                                    .iter()
                                    .map(|t| eval_val(parameters, opt, t))
                                    .collect::<Result<Vec<_>, _>>()
                            })
                            .transpose()
                    };
                    let bound = |text: &Option<String>| -> Result<Option<f64>, String> {
                        let Some(text) = text else {
                            return Ok(None);
                        };
                        match eval_val(parameters, opt, text)? {
                            Val::Num(v) => Ok(Some(v)),
                            Val::Text(_) => Err(format!(
                                "{} is a text option: it has no minimum, maximum or step",
                                opt.name
                            )),
                        }
                    };
                    out.push(Restriction {
                        rule: rule.name.clone(),
                        only: list(&action.values).map_err(at)?,
                        exclude: list(&action.exclude).map_err(at)?.unwrap_or_default(),
                        min: bound(&action.min).map_err(at)?,
                        max: bound(&action.max).map_err(at)?,
                        step: bound(&action.step).map_err(at)?,
                    });
                } else if !is_check(action)
                    && let Some(only) = self
                        .other_action_allowed(rule, action, parameters, opt)
                        .map_err(at)?
                {
                    out.push(Restriction {
                        rule: rule.name.clone(),
                        only: Some(only),
                        ..Restriction::default()
                    });
                }
            }
        }
        Ok(out)
    }

    /// The `require` and `forbid` actions of the active rules that the
    /// parameters break (mitcad#799), and what an action of another kind
    /// says is broken ([`Configurations::other_action_violation`]).
    fn rule_checks(&self, parameters: &Parameters) -> Result<Vec<RuleViolation>, String> {
        let mut out = Vec::new();
        for rule in &self.rules {
            for action in active_actions(rule, parameters)? {
                if let Some(condition) = &action.require {
                    let ok = holds(parameters, condition)
                        .map_err(|e| format!("the rule {}: require: {e}", rule.name))?;
                    if !ok {
                        out.push(RuleViolation {
                            rule: rule.name.clone(),
                            message: action
                                .message
                                .clone()
                                .filter(|m| !m.trim().is_empty())
                                .unwrap_or_else(|| format!("{condition} does not hold")),
                        });
                    }
                } else if let Some(message) = &action.forbid {
                    out.push(RuleViolation {
                        rule: rule.name.clone(),
                        message: if message.trim().is_empty() {
                            "the combination is not allowed".to_owned()
                        } else {
                            message.clone()
                        },
                    });
                } else if !action.is_restriction()
                    && let Some(violation) = self
                        .other_action_violation(rule, action, parameters)
                        .map_err(|e| format!("the rule {}: {e}", rule.name))?
                {
                    out.push(violation);
                }
            }
        }
        Ok(out)
    }

    /// The rules whose `when` holds (or that have none) for the
    /// parameters: they apply their `then` (Check Rules' "never applies").
    fn applying_rules(&self, parameters: &Parameters) -> Vec<String> {
        self.rules
            .iter()
            .filter(|rule| {
                rule.enabled
                    && rule
                        .when
                        .as_deref()
                        .is_none_or(|when| holds(parameters, when).unwrap_or(false))
            })
            .map(|rule| rule.name.clone())
            .collect()
    }

    // Actions of other kinds than a restriction, `require` and `forbid`
    // (mitcad#806 adds `in_table`): one hook for what such an action
    // allows an option, one for what it says is broken, one for its
    // problems.

    /// The values an action of another kind allows `opt` when its rule is
    /// active, like a `values` restriction; None when it does not restrict
    /// the option.
    fn other_action_allowed(
        &self,
        rule: &ConfigurationRule,
        action: &RuleAction,
        parameters: &Parameters,
        opt: &Opt,
    ) -> Result<Option<Vec<Val>>, String> {
        let _ = rule;
        // `in_table` (mitcad#806): the option's cells in the rows that
        // match the mapped options before it.
        let Some(in_table) = action.as_in_table() else {
            return Ok(None);
        };
        let tables = tables_of(parameters);
        Ok(self
            .in_table_allowed(in_table, &opt.name, parameters, &tables)?
            .map(|values| values.iter().filter_map(|v| val_of(&v.value)).collect()))
    }

    /// What an action of another kind says is broken when its rule is
    /// active; None when the parameters meet it.
    fn other_action_violation(
        &self,
        rule: &ConfigurationRule,
        action: &RuleAction,
        parameters: &Parameters,
    ) -> Result<Option<RuleViolation>, String> {
        // `in_table` (mitcad#806): the combination must be a row.
        let Some(in_table) = action.as_in_table() else {
            return Ok(None);
        };
        let tables = tables_of(parameters);
        Ok(self
            .in_table_violation(in_table, parameters, &tables)?
            .map(|message| RuleViolation {
                rule: rule.name.clone(),
                message,
            }))
    }

    /// What is wrong with an action of another kind; any action that is no
    /// restriction, `require` or `forbid` is one of no kind so far.
    fn other_action_problem(&self, action: &RuleAction, parameters: &Parameters) -> Option<String> {
        // `in_table` (mitcad#806).
        if let Some(in_table) = action.as_in_table() {
            let problems = self.in_table_problems(in_table, parameters, &tables_of(parameters));
            return (!problems.is_empty()).then(|| problems.join("; "));
        }
        if let Some(table) = &action.in_table {
            return Some(format!(
                "in_table {table} maps no option to a column (columns)"
            ));
        }
        if action.columns.is_some() {
            return Some("columns belong to an in_table action: name its table".to_owned());
        }
        Some(
            "an action has no kind: give it an option with values, exclude, min, max or \
             step, require or forbid, or in_table"
                .to_owned(),
        )
    }

    /// What is wrong with the rules: names, actions, options, expressions
    /// that do not evaluate. Empty intersections depend on the values and
    /// are violations ([`Configurations::violations`]). The errors of
    /// [`Configurations::placed_rule_problems`], in its words.
    pub fn rule_problems(&self, parameters: &Parameters) -> Vec<String> {
        self.placed_rule_problems(parameters)
            .into_iter()
            .filter(|p| !p.warning)
            .map(|p| p.message)
            .collect()
    }

    /// What is wrong with the rules, each problem with its rule and its
    /// place in the rule (mitcad#1064): the errors of
    /// [`Configurations::rule_problems`], and as warnings the values of an
    /// action that are outside its option's domain (a value that is no
    /// value of the option, a bound beyond all of them).
    pub fn placed_rule_problems(&self, parameters: &Parameters) -> Vec<RuleProblem> {
        let mut out = Vec::new();
        let mut names = BTreeSet::new();
        for (i, rule) in self.rules.iter().enumerate() {
            let problem = |place: Option<String>, warning: bool, message: String| RuleProblem {
                index: i,
                rule: rule.name.clone(),
                place,
                warning,
                message,
            };
            let label = if rule.name.trim().is_empty() {
                out.push(problem(None, false, format!("rule {} has no name", i + 1)));
                format!("rule {}", i + 1)
            } else {
                format!("the rule {}", rule.name)
            };
            if !rule.name.trim().is_empty() && !names.insert(rule.name.as_str()) {
                out.push(problem(
                    None,
                    false,
                    format!("{label}: another rule has the same name"),
                ));
            }
            if let Some(when) = &rule.when
                && let Err(e) = holds(parameters, when)
            {
                out.push(problem(
                    Some("when".to_owned()),
                    false,
                    format!("{label}: when: {e}"),
                ));
            }
            let actions = rule
                .then
                .iter()
                .enumerate()
                .map(|(k, action)| (format!("then.{k}"), action))
                .chain(
                    rule.otherwise
                        .iter()
                        .enumerate()
                        .map(|(k, action)| (format!("else.{k}"), action)),
                );
            for (place, action) in actions {
                let at = |part: &str| Some(format!("{place}.{part}"));
                let other = action.in_table.is_some() || action.columns.is_some();
                let kinds = usize::from(action.is_restriction())
                    + usize::from(action.require.is_some())
                    + usize::from(action.forbid.is_some())
                    + usize::from(other);
                if kinds > 1 {
                    out.push(problem(
                        Some(place.clone()),
                        false,
                        format!(
                            "{label}: an action has two kinds: an option's values, require, \
                             forbid and in_table go in actions of their own"
                        ),
                    ));
                    continue;
                }
                if kinds == 0 || other {
                    if let Some(message) = self.other_action_problem(action, parameters) {
                        out.push(problem(
                            Some(place.clone()),
                            false,
                            format!("{label}: {message}"),
                        ));
                    }
                    continue;
                }
                if let Some(condition) = &action.require
                    && let Err(e) = holds(parameters, condition)
                {
                    out.push(problem(
                        at("require"),
                        false,
                        format!("{label}: require: {e}"),
                    ));
                }
                if !action.is_restriction() {
                    continue;
                }
                let Some(option) = action.option.as_deref() else {
                    out.push(problem(
                        Some(place.clone()),
                        false,
                        format!("{label}: an action restricts no option"),
                    ));
                    continue;
                };
                if !self.is_option(option) {
                    out.push(problem(
                        at("option"),
                        false,
                        format!("{label}: {option} is not an option of the configurations"),
                    ));
                    continue;
                }
                let Ok(opt) = option_info(parameters, option) else {
                    continue;
                };
                if opt.text {
                    let bound = [
                        ("min", &action.min),
                        ("max", &action.max),
                        ("step", &action.step),
                    ]
                    .into_iter()
                    .find(|(_, text)| text.is_some());
                    if let Some((what, _)) = bound {
                        out.push(problem(
                            at(what),
                            false,
                            format!(
                                "{label}: {option} is a text option: it has no minimum, maximum \
                                 or step"
                            ),
                        ));
                        continue;
                    }
                }
                // The action's expressions as `expressions` gives them, each
                // with its place: a list's by its index.
                let mut parts: Vec<(&str, String, &str)> = Vec::new();
                for (what, list) in [("values", &action.values), ("exclude", &action.exclude)] {
                    for (k, text) in list.iter().flatten().enumerate() {
                        parts.push((what, format!("{what}.{k}"), text.as_str()));
                    }
                }
                for (what, text) in [
                    ("min", &action.min),
                    ("max", &action.max),
                    ("step", &action.step),
                ] {
                    if let Some(text) = text {
                        parts.push((what, what.to_owned(), text.as_str()));
                    }
                }
                let domain = Set::domain(parameters, &opt);
                for (what, part, text) in parts {
                    // A list that is a table column (mitcad#806).
                    if matches!(what, "values" | "exclude")
                        && let Some(Err(e)) = super::from_tables::list_values(
                            parameters,
                            &tables_of(parameters),
                            option,
                            std::slice::from_ref(&text.to_owned()),
                        )
                    {
                        out.push(problem(at(&part), false, format!("{label}: {what}: {e}")));
                        continue;
                    }
                    if super::from_tables::column_reference(text).is_some() {
                        continue;
                    }
                    match eval_val(parameters, &opt, text) {
                        Err(e) => {
                            out.push(problem(at(&part), false, format!("{label}: {what}: {e}")));
                        }
                        Ok(value) => {
                            if let Some(outside) = outside_domain(&domain, what, &value, &opt) {
                                out.push(problem(
                                    at(&part),
                                    true,
                                    format!("{label}: {what}: {outside}"),
                                ));
                            }
                        }
                    }
                }
                if let Some(step) = &action.step
                    && let Ok(Val::Num(v)) = eval_val(parameters, &opt, step)
                    && v <= 0.0
                {
                    out.push(problem(
                        at("step"),
                        false,
                        format!("{label}: the step must be positive"),
                    ));
                }
            }
        }
        out
    }
}

/// Why a value of a rule's action is outside its option's domain
/// (mitcad#1064): a value listed in `values` or `exclude` that is no value
/// of the option, a `min` above all of its values, a `max` below all of
/// them. None when it is inside, and for a domain that is free.
fn outside_domain(domain: &Set, what: &str, value: &Val, opt: &Opt) -> Option<String> {
    let text = value.text(opt.unit);
    match what {
        "values" | "exclude" => (!domain.contains(value))
            .then(|| format!("{text} is not one of the values of {}", opt.name)),
        "min" | "max" => {
            let Val::Num(v) = value else {
                return None;
            };
            // The domain's smallest and largest value: a list's numbers, or
            // the range's bounds.
            let (low, high) = match &domain.list {
                Some(list) => {
                    let numbers: Vec<f64> = list
                        .iter()
                        .filter_map(|v| match v {
                            Val::Num(n) => Some(*n),
                            Val::Text(_) => None,
                        })
                        .collect();
                    (
                        numbers.iter().copied().reduce(f64::min),
                        numbers.iter().copied().reduce(f64::max),
                    )
                }
                None => (domain.min, domain.max),
            };
            if what == "min"
                && let Some(high) = high
                && *v > high
                && !near(*v, high)
            {
                return Some(format!(
                    "{text} is above the largest value of {}, {}",
                    opt.name,
                    Val::Num(high).text(opt.unit)
                ));
            }
            if what == "max"
                && let Some(low) = low
                && *v < low
                && !near(*v, low)
            {
                return Some(format!(
                    "{text} is below the smallest value of {}, {}",
                    opt.name,
                    Val::Num(low).text(opt.unit)
                ));
            }
            None
        }
        _ => None,
    }
}

/// The most unrepairable combinations Check Rules lists.
const MAX_UNREPAIRABLE: usize = 20;

impl Configurations {
    /// Check Rules: goes through every combination of the options' finite
    /// domains (lists, and ranges with a minimum, a maximum and a step), up
    /// to `limit` of them, else `limit` combinations spread evenly. Driven
    /// options and options with no finite domain keep their values.
    pub fn check_rules(&self, parameters: &Parameters, limit: u64) -> RulesReport {
        let mut report = RulesReport {
            problems: self.problems(parameters),
            ..RulesReport::default()
        };
        let opts = match self.option_infos(parameters) {
            Ok(opts) => opts,
            Err(e) => {
                report.problems.push(e);
                return report;
            }
        };
        // The finite domains of the options that are chosen.
        let mut axes: Vec<(usize, Vec<Val>)> = Vec::new();
        let mut finite = true;
        for (i, opt) in opts.iter().enumerate() {
            if opt.driven {
                continue;
            }
            let mut domain = Set::domain(parameters, opt);
            // A closed table's values are its rows'.
            if self.closed {
                let rows: Vec<Val> = self
                    .rows
                    .iter()
                    .filter_map(|row| row_val(parameters, opt, row))
                    .fold(Vec::new(), |mut all, v| {
                        if !all.iter().any(|a: &Val| a.same(&v)) {
                            all.push(v);
                        }
                        all
                    });
                domain.narrow(&Restriction {
                    rule: CLOSED_TABLE.to_owned(),
                    only: Some(rows),
                    ..Restriction::default()
                });
            }
            match domain.members() {
                Some(values) if !values.is_empty() => axes.push((i, values)),
                _ => finite = false,
            }
        }
        let total = axes
            .iter()
            .try_fold(1_u64, |n, (_, values)| n.checked_mul(values.len() as u64));
        report.combinations = if finite { total } else { None };
        let total = total.unwrap_or(u64::MAX);
        let limit = limit.max(1);
        report.sampled = total > limit;
        let count = total.min(limit);
        let mut kept: Vec<Vec<bool>> = axes.iter().map(|(_, v)| vec![false; v.len()]).collect();
        let mut applied: BTreeSet<String> = BTreeSet::new();
        let mut empty_seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
        for n in 0..count {
            // Spread evenly over all combinations when sampled.
            let mut index = if report.sampled {
                ((u128::from(n) * u128::from(total)) / u128::from(count)) as u64
            } else {
                n
            };
            let mut trial = parameters.clone();
            let mut texts = BTreeMap::new();
            let mut ok = true;
            for (i, values) in &axes {
                let len = values.len() as u64;
                let v = &values[(index % len) as usize];
                index /= len;
                let opt = &opts[*i];
                texts.insert(opt.name.clone(), v.text(opt.unit));
                ok &= trial
                    .set_expression(opt.id, &v.expression(opt.unit))
                    .is_ok();
            }
            if !ok {
                continue;
            }
            report.checked += 1;
            applied.extend(self.applying_rules(&trial));
            // Options left no value, given the options before them.
            if let Ok(trial_opts) = self.option_infos(&trial) {
                for i in 0..trial_opts.len() {
                    let Ok(restrictions) = self.restrictions(&trial, &trial_opts, i) else {
                        continue;
                    };
                    let rules: Vec<Restriction> = restrictions
                        .into_iter()
                        .filter(|r| r.rule != CLOSED_TABLE)
                        .collect();
                    if rules.is_empty()
                        || !Self::allowed_set(&trial, &trial_opts[i], &rules).is_empty()
                    {
                        continue;
                    }
                    let earlier: BTreeMap<String, String> = trial_opts[..i]
                        .iter()
                        .map(|o| (o.name.clone(), o.value.text(o.unit)))
                        .collect();
                    let key = (
                        trial_opts[i].name.clone(),
                        earlier.values().cloned().collect::<Vec<_>>(),
                    );
                    if empty_seen.insert(key) && report.empty.len() < MAX_UNREPAIRABLE {
                        let mut names: Vec<String> = rules.iter().map(|r| r.rule.clone()).collect();
                        names.dedup();
                        report.empty.push(EmptyIntersection {
                            option: trial_opts[i].name.clone(),
                            rules: names,
                            values: earlier,
                        });
                    }
                }
            }
            match cascade(&mut trial, self, &BTreeSet::new()) {
                Ok(done) if done.violations.is_empty() => {
                    for (k, (i, values)) in axes.iter().enumerate() {
                        let opt = &opts[*i];
                        if let Ok(now) = option_info(&trial, &opt.name)
                            && let Some(at) = values.iter().position(|v| v.same(&now.value))
                        {
                            kept[k][at] = true;
                        }
                    }
                }
                Ok(done) => {
                    if report.unrepairable.len() < MAX_UNREPAIRABLE {
                        report
                            .unrepairable
                            .push((texts, done.violations[0].clone()));
                    }
                }
                Err(e) => {
                    if !report.problems.contains(&e) {
                        report.problems.push(e);
                    }
                }
            }
        }
        for (k, (i, values)) in axes.iter().enumerate() {
            let opt = &opts[*i];
            for (at, v) in values.iter().enumerate() {
                if !kept[k][at] {
                    report
                        .never_allowed
                        .push((opt.name.clone(), v.text(opt.unit)));
                }
            }
        }
        report.never_applies = self
            .rules
            .iter()
            .filter(|r| r.enabled && !applied.contains(&r.name))
            .map(|r| r.name.clone())
            .collect();
        // Rows that break a rule.
        for row in &self.rows {
            let mut trial = parameters.clone();
            if super::apply_values(&mut trial, row).is_err() {
                continue;
            }
            if let Ok(broken) = self.violations(&trial)
                && let Some(v) = broken.into_iter().next()
            {
                report.rows.push((row.name.clone(), v));
            }
        }
        report.lines = report_lines(&report);
        report
    }
}

/// The report of Check Rules in words.
fn report_lines(report: &RulesReport) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match (report.combinations, report.sampled) {
        (Some(n), false) => format!("{} of {n} combinations checked", report.checked),
        (Some(n), true) => format!(
            "{} of {n} combinations checked, sampled evenly",
            report.checked
        ),
        (None, _) => format!(
            "{} combinations checked; some options have no finite list of values",
            report.checked
        ),
    });
    for (option, text) in &report.never_allowed {
        lines.push(format!("{option} {text} is never allowed"));
    }
    for rule in &report.never_applies {
        lines.push(format!("the rule {rule} never applies"));
    }
    for e in &report.empty {
        let values: Vec<String> = e.values.iter().map(|(k, v)| format!("{k} {v}")).collect();
        lines.push(format!(
            "{} leave {} no value{}",
            e.rules.join(" and "),
            e.option,
            if values.is_empty() {
                String::new()
            } else {
                format!(" when {}", values.join(" and "))
            }
        ));
    }
    for (row, v) in &report.rows {
        lines.push(format!("row {row}: {}", v.text()));
    }
    for (values, v) in &report.unrepairable {
        let values: Vec<String> = values.iter().map(|(k, x)| format!("{k} {x}")).collect();
        lines.push(format!(
            "{} cannot be repaired: {}",
            values.join(", "),
            v.text()
        ));
    }
    if lines.len() == 1 && report.problems.is_empty() {
        lines.push("no problems found".to_owned());
    }
    lines
}
