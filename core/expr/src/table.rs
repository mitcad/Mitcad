// SPDX-License-Identifier: MIT
//! Named parameters with expressions, evaluated in dependency order (the
//! table of the Change Parameters dialog).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use super::ast::Expr;
use super::eval::{EvalContext, EvalError, Lookup, Value};
use super::format::{format_value, value_to_expression};
use super::limits::{self, Bounds, LimitError, LimitExprs, Limited, ParamLimits};
use super::lookups::{ColumnError, TableColumn};
use super::parser::ParseError;
use super::units::{Quantity, Unit};
use super::{ExprError, is_name_char, is_name_start, is_reserved_name, with_decimal_points};

/// Stable handle to a parameter in a [`ParameterTable`]. A table never
/// hands out the same id twice, and ids increase in creation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamId(u32);

impl ParamId {
    /// An id as saved in a file.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// The kinds of parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamKind {
    /// Created by the user in the parameters dialog.
    User,
    /// Created by a feature or a sketch dimension, such as `d1`.
    Model,
    /// The read-only value a driven sketch dimension measures (mitcad#231):
    /// it can be renamed and used in expressions, never set.
    Reference,
}

impl ParamKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Model => "model",
            Self::Reference => "reference",
        }
    }
}

impl fmt::Display for ParamKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Definition of a parameter to add or load.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamSpec {
    pub name: String,
    pub expression: String,
    pub unit: Unit,
    pub comment: String,
    pub kind: ParamKind,
    /// Default units retained by an expression copied from another
    /// document. None follows the table's defaults.
    pub expression_context: Option<EvalContext>,
    /// Minimum, maximum, step and strictness (mitcad#665); none by default.
    pub limits: ParamLimits,
}

impl ParamSpec {
    pub fn new(kind: ParamKind, name: &str, expression: &str, unit: Unit) -> Self {
        Self {
            name: name.to_owned(),
            expression: expression.to_owned(),
            unit,
            comment: String::new(),
            kind,
            expression_context: None,
            limits: ParamLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: ParamLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn user(name: &str, expression: &str, unit: Unit) -> Self {
        Self::new(ParamKind::User, name, expression, unit)
    }

    pub fn model(name: &str, expression: &str, unit: Unit) -> Self {
        Self::new(ParamKind::Model, name, expression, unit)
    }

    /// A parameter whose expression is the canonical `value` written in
    /// `unit`, such as `12.5 mm`.
    pub fn from_value(kind: ParamKind, name: &str, value: f64, unit: Unit) -> Self {
        Self::new(kind, name, &value_to_expression(value, unit), unit)
    }

    pub fn with_comment(mut self, comment: &str) -> Self {
        comment.clone_into(&mut self.comment);
        self
    }

    pub fn with_expression_context(mut self, context: Option<EvalContext>) -> Self {
        self.expression_context = context;
        self
    }
}

/// Parameters whose values changed, in creation order.
pub type ChangeSet = BTreeSet<ParamId>;

/// A parameter in a [`ParameterTable`]. Its value is always the current
/// evaluation of its expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    id: ParamId,
    name: String,
    expression: String,
    expr: Expr,
    unit: Unit,
    comment: String,
    kind: ParamKind,
    value: Quantity,
    /// The value of a text parameter (unit [`Unit::TEXT`], mitcad#664);
    /// its `value` is then 0.
    text: Option<String>,
    dependencies: Vec<ParamId>,
    expression_context: Option<EvalContext>,
    /// Limits as written, parsed and evaluated, the result before them
    /// and which one took effect (mitcad#665).
    limits: ParamLimits,
    limit_exprs: LimitExprs,
    bounds: Bounds,
    unclamped: Quantity,
    limited: Option<Limited>,
    /// Its allowed values evaluated (mitcad#798): numbers in canonical
    /// units or texts; None when it has no `values`.
    allowed: Option<Vec<Value>>,
}

impl Parameter {
    pub fn id(&self) -> ParamId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The expression text as entered, with decimal points: `1,5 mm` is
    /// kept as `1.5 mm` ([`with_decimal_points`]).
    pub fn expression(&self) -> &str {
        &self.expression
    }

    pub fn expr(&self) -> &Expr {
        &self.expr
    }

    pub fn unit(&self) -> Unit {
        self.unit
    }

    /// Explicit defaults of a copied expression; None uses its document's
    /// defaults. The parameter's own unit still takes precedence.
    pub fn expression_context(&self) -> Option<EvalContext> {
        self.expression_context
    }

    pub fn comment(&self) -> &str {
        &self.comment
    }

    pub fn kind(&self) -> ParamKind {
        self.kind
    }

    /// The value in canonical units (millimetres, radians); 0 for a text
    /// parameter.
    pub fn value(&self) -> f64 {
        self.value.value
    }

    pub fn quantity(&self) -> Quantity {
        self.value
    }

    /// Whether it is a text parameter (unit `Text`, mitcad#664).
    pub fn is_text(&self) -> bool {
        self.unit.is_text()
    }

    /// The value of a text parameter (mitcad#664); None for a number.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The value in the parameter's own unit.
    pub fn value_in_unit(&self) -> f64 {
        self.unit.canonical_to_unit(self.value.value)
    }

    /// The value as text in the parameter's unit, such as `12.5 mm`; a text
    /// parameter's text as it is.
    pub fn formatted_value(&self, max_decimals: usize) -> String {
        match &self.text {
            Some(text) => text.clone(),
            None => format_value(self.value.value, self.unit, max_decimals),
        }
    }

    /// The parameters this one's expression and limits reference, each
    /// once.
    pub fn dependencies(&self) -> &[ParamId] {
        &self.dependencies
    }

    /// Its limits as written (mitcad#665).
    pub fn limits(&self) -> &ParamLimits {
        &self.limits
    }

    /// Its limits evaluated, in canonical units.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The expression's result before the limits, in canonical units.
    pub fn unclamped(&self) -> f64 {
        self.unclamped.value
    }

    /// Which limit changed the value, if one did.
    pub fn limited(&self) -> Option<Limited> {
        self.limited
    }

    /// Its allowed values evaluated (mitcad#798): numbers in canonical
    /// units or texts, in the order written; None without `values`.
    pub fn allowed_values(&self) -> Option<&[Value]> {
        self.allowed.as_deref()
    }

    /// The names its limit expressions reference, each once.
    pub fn limit_references(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for (_, expr) in self.limit_exprs.all() {
            for name in expr.references() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// The table columns its expression and limits look values up in
    /// (mitcad#667), as `(table, column)` pairs, each once: the
    /// expression's first.
    pub fn table_references(&self) -> Vec<(&str, &str)> {
        let mut pairs = self.expr.table_references();
        for (_, expr) in self.limit_exprs.all() {
            for pair in expr.table_references() {
                if !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
        }
        // Allowed values that are a column (mitcad#806).
        if let Some(pair) = self.limit_exprs.values_column()
            && !pairs.contains(&pair)
        {
            pairs.push(pair);
        }
        pairs
    }
}

/// A parameter's value with its limits applied.
#[derive(Debug, Clone)]
struct Evaluated {
    value: Quantity,
    unclamped: Quantity,
    bounds: Bounds,
    limited: Option<Limited>,
    /// A text parameter's text (mitcad#664).
    text: Option<String>,
    /// The allowed values evaluated (mitcad#798).
    allowed: Option<Vec<Value>>,
}

impl Evaluated {
    fn unlimited(value: Quantity) -> Self {
        Self {
            value,
            unclamped: value,
            bounds: Bounds::default(),
            limited: None,
            text: None,
            allowed: None,
        }
    }

    /// A text parameter's value.
    fn text(text: String) -> Self {
        Self {
            text: Some(text),
            ..Self::unlimited(Quantity::unitless(0.0))
        }
    }
}

/// What a parameter is evaluated from: its expression, unit and limits,
/// either its own or the ones an edit gives it.
#[derive(Clone, Copy)]
struct Definition<'a> {
    name: &'a str,
    expr: &'a Expr,
    unit: Unit,
    context: Option<EvalContext>,
    limits: &'a LimitExprs,
    strict: bool,
}

impl Parameter {
    fn store(&mut self, evaluated: Evaluated) {
        self.value = evaluated.value;
        self.unclamped = evaluated.unclamped;
        self.bounds = evaluated.bounds;
        self.limited = evaluated.limited;
        self.text = evaluated.text;
        self.allowed = evaluated.allowed;
    }

    fn definition(&self) -> Definition<'_> {
        Definition {
            name: &self.name,
            expr: &self.expr,
            unit: self.unit,
            context: self.expression_context,
            limits: &self.limit_exprs,
            strict: self.limits.strict,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableError {
    UnknownId(ParamId),
    InvalidName(String),
    /// A unit, function or constant name.
    ReservedName(String),
    DuplicateName(String),
    Parse {
        parameter: String,
        error: ParseError,
    },
    /// Evaluation failed, including references to unknown names.
    Eval {
        parameter: String,
        error: EvalError,
    },
    /// Circular references: each parameter references the next, and the
    /// last is the first again.
    Cycle {
        path: Vec<String>,
    },
    /// The parameter is referenced by others and cannot be removed.
    InUse {
        name: String,
        used_by: Vec<String>,
    },
    /// Loaded ids must increase.
    IdOrder(ParamId),
    /// A limit that does not parse or evaluate, a minimum above the
    /// maximum, a step that is not positive, or a strict parameter's result
    /// outside its limits (mitcad#665).
    Limit {
        parameter: String,
        error: LimitError,
    },
    /// The name of a table (mitcad#667), which tables and parameters
    /// share.
    TableName(String),
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownId(id) => write!(f, "no parameter with id {}", id.0),
            Self::InvalidName(name) => write!(
                f,
                "{name} is not a valid parameter name: use letters, digits and '_', starting with a letter"
            ),
            Self::ReservedName(name) => write!(
                f,
                "{name} is a unit, function or constant and cannot name a parameter"
            ),
            Self::DuplicateName(name) => write!(f, "parameter {name} already exists"),
            Self::Parse { parameter, error } => write!(f, "parameter {parameter}: {error}"),
            Self::Eval { parameter, error } => write!(f, "parameter {parameter}: {error}"),
            Self::Cycle { path } => write!(f, "circular reference: {}", path.join(" -> ")),
            Self::InUse { name, used_by } => {
                write!(f, "parameter {name} is used by {}", used_by.join(", "))
            }
            Self::IdOrder(id) => write!(f, "parameter ids must increase, found {}", id.0),
            Self::Limit { parameter, error } => write!(f, "parameter {parameter}: {error}"),
            Self::TableName(name) => write!(f, "{name} is the name of a table"),
        }
    }
}

impl std::error::Error for TableError {}

/// Named parameters in creation order. Every parameter's value is the
/// evaluation of its expression; edits that would break this (syntax
/// errors, unknown names, cycles, unit mismatches) are refused and leave
/// the table unchanged.
#[derive(Debug, Clone, Default)]
pub struct ParameterTable {
    context: EvalContext,
    /// Sorted by id, which is creation order.
    items: Vec<Parameter>,
    by_name: HashMap<String, ParamId>,
    next_id: u32,
    /// Where [`ParameterTable::unused_name`] starts looking for a free
    /// dimension name: every `d<n>` below it is taken or reserved
    /// (mitcad#413: a search from `d1` each time made adding many
    /// dimensions quadratic).
    dimension_names_from: u64,
    /// The document's tables, which `Table.Column` references look values
    /// up in (mitcad#667).
    tables: Option<Arc<dyn TableSource>>,
}

/// The tables of a document that expressions look values up in with
/// `Table.Column` (mitcad#667): [`Lookup::column`] gives a column's cells.
/// Tables share the parameters' namespace, so a parameter cannot take a
/// table's name.
pub trait TableSource: Lookup + fmt::Debug + Send + Sync {
    /// Whether a table of this name exists.
    fn has_table(&self, name: &str) -> bool;
}

/// The prefix of dimension parameters' names.
const DIMENSION_PREFIX: &str = "d";

/// Values of the table with pending new values on top.
struct Overlay<'a> {
    table: &'a ParameterTable,
    values: &'a HashMap<ParamId, Evaluated>,
}

impl Overlay<'_> {
    /// The pending or current value of `name`: its number and its text
    /// (a text parameter's, mitcad#664).
    fn current(&self, name: &str) -> Option<(Quantity, Option<String>)> {
        let id = *self.table.by_name.get(name)?;
        match self.values.get(&id) {
            Some(e) => Some((e.value, e.text.clone())),
            None => self.table.get(id).map(|p| (p.value, p.text.clone())),
        }
    }
}

impl Lookup for Overlay<'_> {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        match self.current(name)? {
            (q, None) => Some(q),
            (_, Some(_)) => None,
        }
    }

    fn lookup_text(&self, name: &str) -> Option<String> {
        self.current(name)?.1
    }

    // A table's column (mitcad#667): the document's tables.
    fn column(&self, table: &str, column: &str) -> Result<TableColumn<'_>, ColumnError> {
        self.table.column(table, column)
    }
}

impl Lookup for ParameterTable {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self.find(name)
            .and_then(|id| self.get(id))
            .filter(|p| p.text.is_none())
            .map(|p| p.value)
    }

    fn lookup_text(&self, name: &str) -> Option<String> {
        self.find(name)
            .and_then(|id| self.get(id))
            .and_then(|p| p.text.clone())
    }

    // A table's column (mitcad#667): the document's tables.
    fn column(&self, table: &str, column: &str) -> Result<TableColumn<'_>, ColumnError> {
        match self.tables.as_deref() {
            Some(tables) => tables.column(table, column),
            None => Err(ColumnError::NoTable),
        }
    }
}

impl ParameterTable {
    /// An empty table. `context` gives the document's default units.
    pub fn new(context: EvalContext) -> Self {
        Self {
            context,
            ..Self::default()
        }
    }

    /// A table of `specs` with ids 0, 1, 2, ... Expressions may reference
    /// parameters later in the list.
    pub fn build(
        context: EvalContext,
        specs: impl IntoIterator<Item = ParamSpec>,
    ) -> Result<Self, TableError> {
        let specs = specs.into_iter().enumerate().map(|(i, spec)| {
            let raw = u32::try_from(i).expect("fewer than 2^32 parameters");
            (ParamId(raw), spec)
        });
        Self::build_with_ids(context, specs)
    }

    /// A table of parameters with saved ids, which must increase.
    pub fn build_with_ids(
        context: EvalContext,
        specs: impl IntoIterator<Item = (ParamId, ParamSpec)>,
    ) -> Result<Self, TableError> {
        Self::build_with_tables(context, specs, None)
    }

    /// [`ParameterTable::build_with_ids`] whose expressions look values up
    /// in `tables` (mitcad#667).
    pub fn build_with_tables(
        context: EvalContext,
        specs: impl IntoIterator<Item = (ParamId, ParamSpec)>,
        tables: Option<Arc<dyn TableSource>>,
    ) -> Result<Self, TableError> {
        let mut table = Self::new(context);
        table.tables = tables;
        for (id, spec) in specs {
            if table.items.last().is_some_and(|p| p.id >= id) || id.0 == u32::MAX {
                return Err(TableError::IdOrder(id));
            }
            table.check_new_name(&spec.name)?;
            let expr = parse(&spec.name, &spec.expression)?;
            let expression = with_decimal_points(&spec.expression).into_owned();
            let (limits, limit_exprs) = parse_limits(&spec.name, spec.limits)?;
            table.by_name.insert(spec.name.clone(), id);
            table.name_taken(&spec.name);
            table.items.push(Parameter {
                id,
                name: spec.name,
                expression,
                expr,
                unit: spec.unit,
                comment: spec.comment,
                kind: spec.kind,
                value: Quantity::unitless(0.0),
                text: None,
                dependencies: Vec::new(),
                expression_context: spec.expression_context,
                limits,
                limit_exprs,
                bounds: Bounds::default(),
                unclamped: Quantity::unitless(0.0),
                limited: None,
                allowed: None,
            });
            table.next_id = id.0 + 1;
        }
        for i in 0..table.items.len() {
            let p = &table.items[i];
            let dependencies = table.resolve_all(&p.name, &p.expr, &p.limit_exprs)?;
            table.items[i].dependencies = dependencies;
        }
        let order = table
            .topological_order()
            .map_err(|cycle| table.cycle_error(&cycle))?;
        let no_overlay = HashMap::new();
        for id in order {
            let i = table.index(id).expect("ordered ids exist");
            let evaluated = table.eval_param(table.items[i].definition(), &no_overlay)?;
            table.items[i].store(evaluated);
        }
        Ok(table)
    }

    /// The document's default units.
    pub fn context(&self) -> EvalContext {
        self.context
    }

    /// Changes the document's default units and re-evaluates everything.
    pub fn set_context(&mut self, context: EvalContext) -> Result<ChangeSet, TableError> {
        let old = std::mem::replace(&mut self.context, context);
        let order = self.evaluation_order();
        match self.evaluate_pending(&order, None) {
            Ok(values) => Ok(self.commit(values)),
            Err(e) => {
                self.context = old;
                Err(e)
            }
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Parameters in creation order.
    pub fn iter(&self) -> impl Iterator<Item = (ParamId, &Parameter)> {
        self.items.iter().map(|p| (p.id, p))
    }

    pub fn find(&self, name: &str) -> Option<ParamId> {
        self.by_name.get(name).copied()
    }

    pub fn get(&self, id: ParamId) -> Option<&Parameter> {
        self.index(id).map(|i| &self.items[i])
    }

    /// The value in canonical units (millimetres, radians).
    ///
    /// # Panics
    ///
    /// If `id` is not in the table.
    pub fn value(&self, id: ParamId) -> f64 {
        self.quantity(id).value
    }

    /// # Panics
    ///
    /// If `id` is not in the table.
    pub fn quantity(&self, id: ParamId) -> Quantity {
        self.get(id).expect("parameter id is in the table").value
    }

    /// The first free name `<prefix>1`, `<prefix>2`, ..., such as `d1`,
    /// `d2` for dimensions.
    pub fn unused_name(&self, prefix: &str) -> String {
        let from = if prefix == DIMENSION_PREFIX {
            self.dimension_names_from.max(1)
        } else {
            1
        };
        (from..)
            .map(|n| format!("{prefix}{n}"))
            .find(|name| !self.by_name.contains_key(name) && !is_reserved_name(name))
            .expect("an unused name exists")
    }

    /// Keeps [`ParameterTable::dimension_names_from`] after `name` was
    /// taken: past the names taken from it on.
    fn name_taken(&mut self, name: &str) {
        let from = self.dimension_names_from.max(1);
        if name == format!("{DIMENSION_PREFIX}{from}") {
            self.dimension_names_from = (from..)
                .find(|n| {
                    let name = format!("{DIMENSION_PREFIX}{n}");
                    !self.by_name.contains_key(&name) && !is_reserved_name(&name)
                })
                .expect("an unused name exists");
        }
    }

    /// Keeps [`ParameterTable::dimension_names_from`] after `name` was
    /// freed: a dimension name below it is free again.
    fn name_freed(&mut self, name: &str) {
        if let Some(n) = name
            .strip_prefix(DIMENSION_PREFIX)
            .filter(|digits| !digits.starts_with('0'))
            .and_then(|digits| digits.parse::<u64>().ok())
        {
            self.dimension_names_from = self.dimension_names_from.min(n);
        }
    }

    /// Adds a parameter. Its expression may reference existing parameters.
    pub fn add(&mut self, spec: ParamSpec) -> Result<ParamId, TableError> {
        self.check_new_name(&spec.name)?;
        // Parsed as written, so that errors suggest what was meant; kept
        // with decimal points (the same expression, the same positions).
        let expr = parse(&spec.name, &spec.expression)?;
        let expression = with_decimal_points(&spec.expression).into_owned();
        let (limits, limit_exprs) = parse_limits(&spec.name, spec.limits)?;
        let refers_to_itself = expr.references().contains(&spec.name.as_str())
            || limit_exprs
                .all()
                .any(|(_, e)| e.references().contains(&spec.name.as_str()));
        if refers_to_itself {
            return Err(TableError::Cycle {
                path: vec![spec.name.clone(), spec.name],
            });
        }
        let dependencies = self.resolve_all(&spec.name, &expr, &limit_exprs)?;
        let evaluated = self.eval_param(
            Definition {
                name: &spec.name,
                expr: &expr,
                unit: spec.unit,
                context: spec.expression_context,
                limits: &limit_exprs,
                strict: limits.strict,
            },
            &HashMap::new(),
        )?;
        let id = ParamId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect("parameter ids left");
        self.by_name.insert(spec.name.clone(), id);
        self.name_taken(&spec.name);
        self.items.push(Parameter {
            id,
            name: spec.name,
            expression,
            expr,
            unit: spec.unit,
            comment: spec.comment,
            kind: spec.kind,
            value: evaluated.value,
            text: evaluated.text,
            dependencies,
            expression_context: spec.expression_context,
            limits,
            limit_exprs,
            bounds: evaluated.bounds,
            unclamped: evaluated.unclamped,
            limited: evaluated.limited,
            allowed: evaluated.allowed,
        });
        Ok(id)
    }

    /// Changes a parameter's expression and re-evaluates it and its
    /// dependents. Returns the parameters whose values changed.
    pub fn set_expression(
        &mut self,
        id: ParamId,
        expression: &str,
    ) -> Result<ChangeSet, TableError> {
        let unit = self.param(id)?.unit;
        self.update(id, expression, unit)
    }

    /// Changes several expressions together, keeping their units. Checks
    /// and evaluates the final dependency graph, so no intermediate state
    /// can reject an otherwise valid set of expressions. On failure the
    /// table is unchanged; on success returns the parameters whose values
    /// changed.
    pub fn set_expressions(
        &mut self,
        expressions: &[(ParamId, &str)],
    ) -> Result<ChangeSet, TableError> {
        if expressions.is_empty() {
            return Ok(ChangeSet::new());
        }
        let mut candidate = self.clone();
        for &(id, expression) in expressions {
            let i = candidate.index(id).ok_or(TableError::UnknownId(id))?;
            let name = &candidate.items[i].name;
            let expr = parse(name, expression)?;
            let dependencies =
                candidate.resolve_all(name, &expr, &candidate.items[i].limit_exprs)?;
            let p = &mut candidate.items[i];
            p.expression = with_decimal_points(expression).into_owned();
            p.expr = expr;
            p.dependencies = dependencies;
        }
        let order = candidate
            .topological_order()
            .map_err(|cycle| candidate.cycle_error(&cycle))?;
        let values = candidate.evaluate_pending(&order, None)?;
        let changed = candidate.commit(values);
        *self = candidate;
        Ok(changed)
    }

    /// Changes a parameter's unit; bare numbers in its expression are then
    /// read in the new unit.
    pub fn set_unit(&mut self, id: ParamId, unit: Unit) -> Result<ChangeSet, TableError> {
        let expression = self.param(id)?.expression.clone();
        self.update(id, &expression, unit)
    }

    /// Sets the expression to a plain value: the canonical `value` written
    /// in the parameter's unit.
    pub fn set_value(&mut self, id: ParamId, value: f64) -> Result<ChangeSet, TableError> {
        let unit = self.param(id)?.unit;
        self.update(id, &value_to_expression(value, unit), unit)
    }

    /// Changes the expression and the unit together.
    pub fn update(
        &mut self,
        id: ParamId,
        expression: &str,
        unit: Unit,
    ) -> Result<ChangeSet, TableError> {
        self.redefine(id, expression, unit, None)
    }

    /// Sets a parameter's limits (mitcad#665): `min` and `max` expressions
    /// in its unit that may reference other parameters, a positive `step`,
    /// and whether a result outside them is refused (`strict`) instead of
    /// clamped and snapped. Re-evaluates it and its dependents; returns the
    /// parameters whose values changed. On failure the table is unchanged.
    pub fn set_limits(
        &mut self,
        id: ParamId,
        limits: ParamLimits,
    ) -> Result<ChangeSet, TableError> {
        let p = self.param(id)?;
        let (expression, unit) = (p.expression.clone(), p.unit);
        self.redefine(id, &expression, unit, Some(limits))
    }

    /// Changes the expression, the unit and, when given, the limits
    /// together, so that a new expression can be checked against new
    /// limits. Returns the parameters whose values changed; on failure the
    /// table is unchanged.
    pub fn redefine(
        &mut self,
        id: ParamId,
        expression: &str,
        unit: Unit,
        limits: Option<ParamLimits>,
    ) -> Result<ChangeSet, TableError> {
        let p = self.param(id)?;
        let name = p.name.clone();
        let expr = parse(&name, expression)?;
        let expression = with_decimal_points(expression);
        let new_limits = limits.map(|l| parse_limits(&name, l)).transpose()?;
        let (limit_exprs, strict) = match &new_limits {
            Some((limits, exprs)) => (exprs, limits.strict),
            None => (&p.limit_exprs, p.limits.strict),
        };
        // A reference to itself, in the expression or a limit, is a cycle.
        let dependencies = self.resolve_all(&name, &expr, limit_exprs)?;
        if let Some(cycle) = self.cycle_through(id, &dependencies) {
            return Err(self.cycle_error(&cycle));
        }
        let order = self.affected(id);
        let replace = Definition {
            expr: &expr,
            unit,
            limits: limit_exprs,
            strict,
            ..p.definition()
        };
        let values = self.evaluate_pending(&order, Some((id, replace)))?;
        let i = self.index(id).expect("checked above");
        let p = &mut self.items[i];
        p.expression = expression.into_owned();
        p.expr = expr;
        p.unit = unit;
        p.dependencies = dependencies;
        if let Some((limits, limit_exprs)) = new_limits {
            p.limits = limits;
            p.limit_exprs = limit_exprs;
        }
        Ok(self.commit(values))
    }

    pub fn set_comment(&mut self, id: ParamId, comment: &str) -> Result<(), TableError> {
        let i = self.index(id).ok_or(TableError::UnknownId(id))?;
        comment.clone_into(&mut self.items[i].comment);
        Ok(())
    }

    /// Changes a parameter's kind, as when the feature that made a model
    /// parameter is deleted while other features still use it.
    pub fn set_kind(&mut self, id: ParamId, kind: ParamKind) -> Result<(), TableError> {
        let i = self.index(id).ok_or(TableError::UnknownId(id))?;
        self.items[i].kind = kind;
        Ok(())
    }

    /// Renames a parameter and rewrites the references in the expressions
    /// that use it, keeping their other text as it was. Returns the
    /// parameters whose expressions were rewritten. Values do not change.
    pub fn rename(&mut self, id: ParamId, new_name: &str) -> Result<Vec<ParamId>, TableError> {
        let old = self.param(id)?.name.clone();
        if old == new_name {
            return Ok(Vec::new());
        }
        self.check_new_name(new_name)?;
        let mut rewrites = Vec::new();
        for (i, p) in self.items.iter().enumerate() {
            if !p.dependencies.contains(&id) {
                continue;
            }
            let text = replace_references(&p.expression, &p.expr, &old, new_name);
            let expr = parse(&p.name, &text)?;
            // Limits that reference it follow too (mitcad#665).
            let mut limits = p.limits.clone();
            for (which, limit) in p.limit_exprs.iter() {
                if let Some(written) = limits.text_mut(which) {
                    *written = replace_references(written, limit, &old, new_name);
                }
            }
            // So do its allowed values (mitcad#798).
            if let Some(values) = limits.values.as_mut() {
                for (written, limit) in values.iter_mut().zip(p.limit_exprs.values()) {
                    *written = replace_references(written, limit, &old, new_name);
                }
            }
            let (limits, limit_exprs) = parse_limits(&p.name, limits)?;
            rewrites.push((i, text, expr, limits, limit_exprs));
        }
        let rewritten = rewrites.iter().map(|(i, ..)| self.items[*i].id).collect();
        for (i, text, expr, limits, limit_exprs) in rewrites {
            self.items[i].expression = text;
            self.items[i].expr = expr;
            self.items[i].limits = limits;
            self.items[i].limit_exprs = limit_exprs;
        }
        self.by_name.remove(&old);
        self.name_freed(&old);
        self.by_name.insert(new_name.to_owned(), id);
        self.name_taken(new_name);
        let i = self.index(id).expect("checked above");
        new_name.clone_into(&mut self.items[i].name);
        Ok(rewritten)
    }

    /// Removes a parameter that no other parameter references.
    pub fn remove(&mut self, id: ParamId) -> Result<Parameter, TableError> {
        let name = self.param(id)?.name.clone();
        let used_by: Vec<String> = self
            .items
            .iter()
            .filter(|p| p.dependencies.contains(&id))
            .map(|p| p.name.clone())
            .collect();
        if !used_by.is_empty() {
            return Err(TableError::InUse { name, used_by });
        }
        let i = self.index(id).expect("checked above");
        let removed = self.items.remove(i);
        self.by_name.remove(&removed.name);
        self.name_freed(&removed.name);
        Ok(removed)
    }

    /// Parameters whose expressions reference `id` directly, in creation
    /// order.
    pub fn dependents(&self, id: ParamId) -> Vec<ParamId> {
        self.items
            .iter()
            .filter(|p| p.dependencies.contains(&id))
            .map(|p| p.id)
            .collect()
    }

    /// All parameters, each after the ones it references.
    pub fn evaluation_order(&self) -> Vec<ParamId> {
        self.topological_order()
            .expect("the table has no circular references")
    }

    /// Evaluates an expression that is not a parameter, such as a feature
    /// input, against the table's values.
    pub fn evaluate(&self, expression: &str, unit: Unit) -> Result<Quantity, ExprError> {
        Ok(Expr::parse(expression)?.eval_as(self, &self.context, unit)?)
    }

    /// Evaluates a text expression that is not a parameter, such as a
    /// sketch text's (mitcad#664), against the table's values.
    pub fn evaluate_text(&self, expression: &str) -> Result<String, ExprError> {
        Ok(Expr::parse(expression)?.eval_text(self, &self.context)?)
    }

    /// The value of an expression, a number or a text, against the table's
    /// values.
    pub fn evaluate_value(&self, expression: &str) -> Result<super::Value, ExprError> {
        Ok(Expr::parse(expression)?.eval_value(self, &self.context)?)
    }

    fn index(&self, id: ParamId) -> Option<usize> {
        self.items.binary_search_by_key(&id, |p| p.id).ok()
    }

    fn param(&self, id: ParamId) -> Result<&Parameter, TableError> {
        self.get(id).ok_or(TableError::UnknownId(id))
    }

    fn check_new_name(&self, name: &str) -> Result<(), TableError> {
        let mut chars = name.chars();
        let well_formed = chars.next().is_some_and(is_name_start) && chars.all(is_name_char);
        if !well_formed {
            Err(TableError::InvalidName(name.to_owned()))
        } else if is_reserved_name(name) {
            Err(TableError::ReservedName(name.to_owned()))
        } else if self.by_name.contains_key(name) {
            Err(TableError::DuplicateName(name.to_owned()))
        } else if self.tables.as_ref().is_some_and(|t| t.has_table(name)) {
            Err(TableError::TableName(name.to_owned()))
        } else {
            Ok(())
        }
    }

    /// The referenced parameters, each once, in order of appearance.
    fn resolve(&self, owner: &str, expr: &Expr) -> Result<Vec<ParamId>, TableError> {
        let mut ids = Vec::new();
        for (name, span) in expr.reference_spans() {
            let Some(&id) = self.by_name.get(name) else {
                return Err(TableError::Eval {
                    parameter: owner.to_owned(),
                    error: EvalError::UnknownReference {
                        name: name.to_owned(),
                        span,
                    },
                });
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// The parameters an expression and its limits reference, each once:
    /// the expression's first, in order of appearance.
    fn resolve_all(
        &self,
        owner: &str,
        expr: &Expr,
        limits: &LimitExprs,
    ) -> Result<Vec<ParamId>, TableError> {
        let mut ids = self.resolve(owner, expr)?;
        for (which, limit) in limits.all() {
            let found = self.resolve(owner, limit).map_err(|e| match e {
                TableError::Eval { parameter, error } => TableError::Limit {
                    parameter,
                    error: LimitError::Eval {
                        limit: which,
                        error,
                    },
                },
                other => other,
            })?;
            for id in found {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    /// A parameter's value: its expression's result with its limits
    /// applied (mitcad#665).
    fn eval_param(
        &self,
        def: Definition<'_>,
        pending: &HashMap<ParamId, Evaluated>,
    ) -> Result<Evaluated, TableError> {
        let lookup = Overlay {
            table: self,
            values: pending,
        };
        let context = def.context.unwrap_or(self.context);
        let limit_error = |error| TableError::Limit {
            parameter: def.name.to_owned(),
            error,
        };
        // A text parameter (mitcad#664): its expression's text, no limits
        // but allowed values (mitcad#798).
        if def.unit.is_text() {
            if !def.limits.has_no_bounds() {
                return Err(limit_error(LimitError::Text));
            }
            let text = def
                .expr
                .eval_text(&lookup, &context)
                .map_err(|error| TableError::Eval {
                    parameter: def.name.to_owned(),
                    error,
                })?;
            if !def.limits.has_values() {
                return Ok(Evaluated::text(text));
            }
            let mut allowed = Vec::with_capacity(def.limits.values().len());
            // A table column (mitcad#806): its texts.
            if let Some((table, column)) = def.limits.values_column() {
                for value in column_values(&lookup, table, column, def.unit).map_err(limit_error)? {
                    if let Value::Text(t) = value {
                        allowed.push(t);
                    }
                }
            }
            for expr in def.limits.values() {
                let value = expr.eval_text(&lookup, &context).map_err(|error| {
                    limit_error(LimitError::Eval {
                        limit: Limited::Values,
                        error,
                    })
                })?;
                allowed.push(value);
            }
            if !allowed.contains(&text) {
                return Err(limit_error(LimitError::NotAllowed {
                    value: super::text_literal(&text),
                    values: allowed
                        .iter()
                        .map(|t| super::text_literal(t))
                        .collect::<Vec<_>>()
                        .join(", "),
                }));
            }
            return Ok(Evaluated {
                allowed: Some(allowed.into_iter().map(Value::Text).collect()),
                ..Evaluated::text(text)
            });
        }
        let unclamped = def
            .expr
            .eval_as(&lookup, &context, def.unit)
            .map_err(|error| TableError::Eval {
                parameter: def.name.to_owned(),
                error,
            })?;
        if def.limits.is_empty() {
            return Ok(Evaluated::unlimited(unclamped));
        }
        let mut bounds = Bounds::default();
        for (which, expr) in def.limits.iter() {
            let bound = expr.eval_as(&lookup, &context, def.unit).map_err(|error| {
                limit_error(LimitError::Eval {
                    limit: which,
                    error,
                })
            })?;
            bounds.set(which, bound.value);
        }
        let (mut value, mut limited) =
            limits::apply(unclamped.value, &bounds, def.strict, def.unit).map_err(limit_error)?;
        // The allowed values (mitcad#798): the nearest, or refused when
        // strict.
        let mut allowed = None;
        if def.limits.has_values() {
            let mut list = Vec::with_capacity(def.limits.values().len());
            // A table column (mitcad#806): its numbers.
            if let Some((table, column)) = def.limits.values_column() {
                for value in column_values(&lookup, table, column, def.unit).map_err(limit_error)? {
                    if let Value::Number(q) = value {
                        list.push(q.value);
                    }
                }
            }
            for expr in def.limits.values() {
                let listed = expr.eval_as(&lookup, &context, def.unit).map_err(|error| {
                    limit_error(LimitError::Eval {
                        limit: Limited::Values,
                        error,
                    })
                })?;
                if !listed.value.is_finite() {
                    return Err(limit_error(LimitError::NotFinite(Limited::Values)));
                }
                list.push(listed.value);
            }
            let (snapped, moved) =
                limits::snap_to_values(value, &list, def.strict, def.unit).map_err(limit_error)?;
            if moved {
                value = snapped;
                if !matches!(limited, Some(Limited::Min | Limited::Max)) {
                    limited = Some(Limited::Values);
                }
            }
            allowed = Some(
                list.into_iter()
                    .map(|v| Value::Number(Quantity::new(v, unclamped.dims)))
                    .collect(),
            );
        }
        Ok(Evaluated {
            value: Quantity::new(value, unclamped.dims),
            unclamped,
            bounds,
            limited,
            text: None,
            allowed,
        })
    }

    /// New values for `order`, without changing the table. `replace` gives
    /// a new expression, unit or limits for one parameter.
    fn evaluate_pending(
        &self,
        order: &[ParamId],
        replace: Option<(ParamId, Definition<'_>)>,
    ) -> Result<HashMap<ParamId, Evaluated>, TableError> {
        let mut values = HashMap::new();
        for &id in order {
            let p = self.get(id).expect("ordered ids exist");
            let def = match replace {
                Some((replaced, def)) if replaced == id => def,
                _ => p.definition(),
            };
            let value = self.eval_param(def, &values)?;
            values.insert(id, value);
        }
        Ok(values)
    }

    /// Stores new values and returns the ids whose values changed. The
    /// result before the limits and which limit took effect are stored
    /// too; they alone are no change.
    fn commit(&mut self, values: HashMap<ParamId, Evaluated>) -> ChangeSet {
        let mut changed = ChangeSet::new();
        for (id, evaluated) in values {
            let i = self.index(id).expect("evaluated ids exist");
            if self.items[i].value != evaluated.value || self.items[i].text != evaluated.text {
                changed.insert(id);
            }
            self.items[i].store(evaluated);
        }
        changed
    }

    /// What the limits of `id` would make of `expression` as its new
    /// expression, without changing the table: the value, the result
    /// before the limits and which limit took effect, or why a strict
    /// parameter would refuse it (the `evaluate` query, mitcad#665).
    pub fn try_expression(
        &self,
        id: ParamId,
        expression: &str,
    ) -> Result<(Quantity, Quantity, Option<Limited>), TableError> {
        let p = self.param(id)?;
        let expr = parse(&p.name, expression)?;
        let evaluated = self.eval_param(
            Definition {
                expr: &expr,
                ..p.definition()
            },
            &HashMap::new(),
        )?;
        Ok((evaluated.value, evaluated.unclamped, evaluated.limited))
    }

    /// `id` and everything that depends on it, in evaluation order.
    fn affected(&self, id: ParamId) -> Vec<ParamId> {
        self.affected_all(&[id])
    }

    /// `ids` and everything that depends on them, in evaluation order.
    fn affected_all(&self, ids: &[ParamId]) -> Vec<ParamId> {
        let mut dependents: HashMap<ParamId, Vec<ParamId>> = HashMap::new();
        for p in &self.items {
            for &dep in &p.dependencies {
                dependents.entry(dep).or_default().push(p.id);
            }
        }
        let mut affected: HashSet<ParamId> = ids.iter().copied().collect();
        let mut work = ids.to_vec();
        while let Some(next) = work.pop() {
            for &d in dependents.get(&next).into_iter().flatten() {
                if affected.insert(d) {
                    work.push(d);
                }
            }
        }
        // The current order puts `id` before its dependents; its new
        // references cannot be among them, as that would be a cycle.
        self.evaluation_order()
            .into_iter()
            .filter(|p| affected.contains(p))
            .collect()
    }

    /// The cycle that giving `id` the references `dependencies` would
    /// close, starting and ending at `id`.
    fn cycle_through(&self, id: ParamId, dependencies: &[ParamId]) -> Option<Vec<ParamId>> {
        dependencies.iter().find_map(|&dep| {
            let path = self.path(dep, id)?;
            Some(std::iter::once(id).chain(path).collect())
        })
    }

    /// A chain of references from `from` to `to`, both included.
    fn path(&self, from: ParamId, to: ParamId) -> Option<Vec<ParamId>> {
        if from == to {
            return Some(vec![to]);
        }
        let mut visited = HashSet::from([from]);
        let mut stack = vec![(from, 0)];
        while let Some(&(node, next)) = stack.last() {
            let deps = &self.get(node)?.dependencies;
            let Some(&dep) = deps.get(next) else {
                stack.pop();
                continue;
            };
            stack.last_mut()?.1 += 1;
            if dep == to {
                return Some(stack.iter().map(|&(n, _)| n).chain([to]).collect());
            }
            if visited.insert(dep) {
                stack.push((dep, 0));
            }
        }
        None
    }

    /// Parameters ordered so that each comes after the ones it references,
    /// otherwise in creation order; or a cycle.
    fn topological_order(&self) -> Result<Vec<ParamId>, Vec<ParamId>> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            Active,
            Done,
        }
        let mut marks = vec![Mark::New; self.items.len()];
        let mut order = Vec::with_capacity(self.items.len());
        for start in 0..self.items.len() {
            if marks[start] != Mark::New {
                continue;
            }
            marks[start] = Mark::Active;
            let mut stack = vec![(start, 0)];
            while let Some(&(node, next)) = stack.last() {
                let Some(&dep) = self.items[node].dependencies.get(next) else {
                    marks[node] = Mark::Done;
                    order.push(self.items[node].id);
                    stack.pop();
                    continue;
                };
                stack.last_mut().expect("not empty").1 += 1;
                let dep = self.index(dep).expect("dependencies exist");
                match marks[dep] {
                    Mark::New => {
                        marks[dep] = Mark::Active;
                        stack.push((dep, 0));
                    }
                    Mark::Active => {
                        let at = stack
                            .iter()
                            .position(|&(n, _)| n == dep)
                            .expect("active nodes are on the stack");
                        let cycle = stack[at..]
                            .iter()
                            .map(|&(n, _)| self.items[n].id)
                            .chain([self.items[dep].id])
                            .collect();
                        return Err(cycle);
                    }
                    Mark::Done => {}
                }
            }
        }
        Ok(order)
    }

    fn cycle_error(&self, cycle: &[ParamId]) -> TableError {
        TableError::Cycle {
            path: cycle
                .iter()
                .map(|&id| self.get(id).map_or_else(String::new, |p| p.name.clone()))
                .collect(),
        }
    }

    // Tables (mitcad#667): the document's tables, which `Table.Column`
    // references look values up in. A table depends on nothing, so no
    // cycle passes through one.

    /// The tables lookups read.
    pub fn tables(&self) -> Option<&Arc<dyn TableSource>> {
        self.tables.as_ref()
    }

    /// Parameters whose expression or limits name the table `name`, in
    /// creation order.
    pub fn table_users(&self, name: &str) -> Vec<ParamId> {
        self.items
            .iter()
            .filter(|p| p.table_references().iter().any(|(t, _)| *t == name))
            .map(|p| p.id)
            .collect()
    }

    /// Replaces the tables and re-evaluates, in one step, the parameters
    /// that name one of the tables `changed` and the parameters that depend
    /// on them. On failure (a key no longer found, a column gone or of
    /// another kind) the table is unchanged. Returns the parameters whose
    /// values changed.
    pub fn set_tables(
        &mut self,
        tables: Option<Arc<dyn TableSource>>,
        changed: &[&str],
    ) -> Result<ChangeSet, TableError> {
        if let Some(p) = self
            .items
            .iter()
            .find(|p| tables.as_ref().is_some_and(|t| t.has_table(&p.name)))
        {
            return Err(TableError::DuplicateName(p.name.clone()));
        }
        let users: Vec<ParamId> = self
            .items
            .iter()
            .filter(|p| {
                p.table_references()
                    .iter()
                    .any(|(t, _)| changed.contains(t))
            })
            .map(|p| p.id)
            .collect();
        let old = std::mem::replace(&mut self.tables, tables);
        let order = self.affected_all(&users);
        match self.evaluate_pending(&order, None) {
            Ok(values) => Ok(self.commit(values)),
            Err(error) => {
                self.tables = old;
                Err(error)
            }
        }
    }

    /// Renames the table `table` to `new_name` (`column` None), or its
    /// column `column`, in every expression and limit that names it, as
    /// [`ParameterTable::rename`] does for parameters; `tables` are the
    /// tables after the rename. Values do not change; on failure the table
    /// is unchanged. Returns the parameters that were rewritten.
    pub fn rename_table_references(
        &mut self,
        tables: Option<Arc<dyn TableSource>>,
        table: &str,
        column: Option<&str>,
        new_name: &str,
    ) -> Result<Vec<ParamId>, TableError> {
        let mut candidate = self.clone();
        candidate.tables = tables;
        let mut rewritten = Vec::new();
        for p in &mut candidate.items {
            let named = p
                .table_references()
                .iter()
                .any(|&(t, c)| t == table && column.is_none_or(|column| column == c));
            if !named {
                continue;
            }
            let text = replace_table_references(&p.expression, &p.expr, table, column, new_name);
            let expr = parse(&p.name, &text)?;
            let mut limits = p.limits.clone();
            for (which, limit) in p.limit_exprs.iter() {
                if let Some(written) = limits.text_mut(which) {
                    *written = replace_table_references(written, limit, table, column, new_name);
                }
            }
            // The allowed values (mitcad#798), and a column as them
            // (mitcad#806).
            if let Some(values) = limits.values.as_mut() {
                if let Some((t, c)) = p.limit_exprs.values_column() {
                    if t == table && column.is_none() {
                        values[0] = format!("{new_name}.{c}");
                    } else if t == table && column == Some(c) {
                        values[0] = format!("{t}.{new_name}");
                    }
                } else {
                    for (written, limit) in values.iter_mut().zip(p.limit_exprs.values()) {
                        *written =
                            replace_table_references(written, limit, table, column, new_name);
                    }
                }
            }
            let (limits, limit_exprs) = parse_limits(&p.name, limits)?;
            p.expression = text;
            p.expr = expr;
            p.limits = limits;
            p.limit_exprs = limit_exprs;
            rewritten.push(p.id);
        }
        let order = candidate.affected_all(&rewritten);
        let values = candidate.evaluate_pending(&order, None)?;
        candidate.commit(values);
        *self = candidate;
        Ok(rewritten)
    }
}

/// The allowed values a table column gives (mitcad#806): its distinct
/// non-empty cells, in the table's order; texts for a text parameter, else
/// numbers in canonical units (a unitless column's numbers read in the
/// parameter's unit). A column of the other kind or of other dimensions
/// is an error.
fn column_values(
    lookup: &dyn Lookup,
    table: &str,
    column: &str,
    unit: Unit,
) -> Result<Vec<Value>, LimitError> {
    let error = |error| LimitError::Eval {
        limit: Limited::Values,
        error,
    };
    let span = super::Span::new(0, table.len() + 1 + column.len());
    let data = lookup.column(table, column).map_err(|e| {
        error(match e {
            ColumnError::NoTable => EvalError::UnknownTable {
                name: table.to_owned(),
                span: super::Span::new(0, table.len()),
            },
            ColumnError::NoColumn => EvalError::UnknownColumn {
                table: table.to_owned(),
                column: column.to_owned(),
                span: super::Span::new(table.len() + 1, span.end),
            },
        })
    })?;
    if unit.is_text() != data.unit.is_text() {
        return Err(error(if unit.is_text() {
            EvalError::NotText { span }
        } else {
            EvalError::NotNumber { span }
        }));
    }
    let promoted = !unit.is_text() && data.unit.is_unitless() && !unit.is_unitless();
    if !unit.is_text() && !promoted && data.unit.dims() != unit.dims() {
        return Err(error(EvalError::WrongDimension {
            expected: unit.dims(),
            found: data.unit.dims(),
        }));
    }
    let mut out: Vec<Value> = Vec::new();
    for cell in &data.cells {
        let value = match *cell {
            super::TableCell::Empty => continue,
            super::TableCell::Text(text) => Value::Text(text.to_owned()),
            super::TableCell::Number(n) if promoted => {
                Value::Number(Quantity::new(unit.to_canonical(n), unit.dims()))
            }
            super::TableCell::Number(n) => {
                Value::Number(Quantity::new(data.unit.to_canonical(n), data.unit.dims()))
            }
        };
        let seen = out.iter().any(|v| match (v, &value) {
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                a.value == b.value
                    || (a.value - b.value).abs() <= 1e-9 * a.value.abs().max(b.value.abs())
            }
            _ => false,
        });
        if !seen {
            out.push(value);
        }
    }
    Ok(out)
}

/// `text` with the references to the table `table` renamed to `new`
/// (`column` None), or those to its column `column` (mitcad#667). `expr` is
/// `text` parsed.
pub fn replace_table_references(
    text: &str,
    expr: &Expr,
    table: &str,
    column: Option<&str>,
    new: &str,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (t, c, span) in expr.column_reference_spans() {
        if t != table {
            continue;
        }
        // No spaces stand around the dot: the table's name starts the
        // span, the column's ends it.
        let (start, end) = match column {
            None => (span.start, span.start + t.len()),
            Some(column) if column == c => (span.end - c.len(), span.end),
            Some(_) => continue,
        };
        out.push_str(&text[copied..start]);
        out.push_str(new);
        copied = end;
    }
    out.push_str(&text[copied..]);
    out
}

fn parse(name: &str, expression: &str) -> Result<Expr, TableError> {
    Expr::parse(expression).map_err(|error| TableError::Parse {
        parameter: name.to_owned(),
        error,
    })
}

/// Limits as kept (blank texts are no limit, decimal commas become points)
/// and parsed.
fn parse_limits(
    name: &str,
    mut limits: ParamLimits,
) -> Result<(ParamLimits, LimitExprs), TableError> {
    for which in limits::ALL_LIMITS {
        let text = limits.text_mut(which);
        *text = text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| with_decimal_points(t).into_owned());
    }
    // Allowed values (mitcad#798): trimmed, with decimal points; an empty
    // list is none.
    limits.values = limits
        .values
        .take()
        .map(|values| {
            values
                .iter()
                .map(|t| with_decimal_points(t.trim()).into_owned())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty());
    let exprs = LimitExprs::parse(&limits).map_err(|error| TableError::Limit {
        parameter: name.to_owned(),
        error,
    })?;
    Ok((limits, exprs))
}

/// `text` with the references to `old` replaced by `new`.
pub fn replace_references(text: &str, expr: &Expr, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (name, span) in expr.reference_spans() {
        if name == old {
            out.push_str(&text[copied..span.start]);
            out.push_str(new);
            copied = span.end;
        }
    }
    out.push_str(&text[copied..]);
    out
}
