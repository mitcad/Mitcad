// SPDX-License-Identifier: MIT
//! Named parameters with expressions, evaluated in dependency order (the
//! table of the Change Parameters dialog).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use super::ast::Expr;
use super::eval::{EvalContext, EvalError, Lookup};
use super::format::{format_value, value_to_expression};
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

/// The two kinds of parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamKind {
    /// Created by the user in the parameters dialog.
    User,
    /// Created by a feature or a sketch dimension, such as `d1`.
    Model,
}

impl ParamKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Model => "model",
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
}

impl ParamSpec {
    pub fn new(kind: ParamKind, name: &str, expression: &str, unit: Unit) -> Self {
        Self {
            name: name.to_owned(),
            expression: expression.to_owned(),
            unit,
            comment: String::new(),
            kind,
        }
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
    dependencies: Vec<ParamId>,
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

    pub fn comment(&self) -> &str {
        &self.comment
    }

    pub fn kind(&self) -> ParamKind {
        self.kind
    }

    /// The value in canonical units (millimetres, radians).
    pub fn value(&self) -> f64 {
        self.value.value
    }

    pub fn quantity(&self) -> Quantity {
        self.value
    }

    /// The value in the parameter's own unit.
    pub fn value_in_unit(&self) -> f64 {
        self.unit.canonical_to_unit(self.value.value)
    }

    /// The value as text in the parameter's unit, such as `12.5 mm`.
    pub fn formatted_value(&self, max_decimals: usize) -> String {
        format_value(self.value.value, self.unit, max_decimals)
    }

    /// The parameters this one's expression references, each once.
    pub fn dependencies(&self) -> &[ParamId] {
        &self.dependencies
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
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownId(id) => write!(f, "no parameter with id {}", id.0),
            Self::InvalidName(name) => write!(
                f,
                "'{name}' is not a valid parameter name: use letters, digits and '_', starting with a letter"
            ),
            Self::ReservedName(name) => write!(
                f,
                "'{name}' is a unit, function or constant and cannot name a parameter"
            ),
            Self::DuplicateName(name) => write!(f, "parameter '{name}' already exists"),
            Self::Parse { parameter, error } => write!(f, "parameter '{parameter}': {error}"),
            Self::Eval { parameter, error } => write!(f, "parameter '{parameter}': {error}"),
            Self::Cycle { path } => write!(f, "circular reference: {}", path.join(" -> ")),
            Self::InUse { name, used_by } => {
                write!(f, "parameter '{name}' is used by {}", used_by.join(", "))
            }
            Self::IdOrder(id) => write!(f, "parameter ids must increase, found {}", id.0),
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
}

/// Values of the table with pending new values on top.
struct Overlay<'a> {
    table: &'a ParameterTable,
    values: &'a HashMap<ParamId, Quantity>,
}

impl Lookup for Overlay<'_> {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        let id = *self.table.by_name.get(name)?;
        self.values
            .get(&id)
            .copied()
            .or_else(|| self.table.get(id).map(|p| p.value))
    }
}

impl Lookup for ParameterTable {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self.find(name).and_then(|id| self.get(id)).map(|p| p.value)
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
        let mut table = Self::new(context);
        for (id, spec) in specs {
            if table.items.last().is_some_and(|p| p.id >= id) || id.0 == u32::MAX {
                return Err(TableError::IdOrder(id));
            }
            table.check_new_name(&spec.name)?;
            let expr = parse(&spec.name, &spec.expression)?;
            let expression = with_decimal_points(&spec.expression).into_owned();
            table.by_name.insert(spec.name.clone(), id);
            table.items.push(Parameter {
                id,
                name: spec.name,
                expression,
                expr,
                unit: spec.unit,
                comment: spec.comment,
                kind: spec.kind,
                value: Quantity::unitless(0.0),
                dependencies: Vec::new(),
            });
            table.next_id = id.0 + 1;
        }
        for i in 0..table.items.len() {
            let p = &table.items[i];
            let dependencies = table.resolve(&p.name, &p.expr)?;
            table.items[i].dependencies = dependencies;
        }
        let order = table
            .topological_order()
            .map_err(|cycle| table.cycle_error(&cycle))?;
        let no_overlay = HashMap::new();
        for id in order {
            let i = table.index(id).expect("ordered ids exist");
            let p = &table.items[i];
            let value = table.eval_param(&p.expr, p.unit, &p.name, &no_overlay)?;
            table.items[i].value = value;
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
        (1u64..)
            .map(|n| format!("{prefix}{n}"))
            .find(|name| !self.by_name.contains_key(name) && !is_reserved_name(name))
            .expect("an unused name exists")
    }

    /// Adds a parameter. Its expression may reference existing parameters.
    pub fn add(&mut self, spec: ParamSpec) -> Result<ParamId, TableError> {
        self.check_new_name(&spec.name)?;
        // Parsed as written, so that errors suggest what was meant; kept
        // with decimal points (the same expression, the same positions).
        let expr = parse(&spec.name, &spec.expression)?;
        let expression = with_decimal_points(&spec.expression).into_owned();
        if expr.references().contains(&spec.name.as_str()) {
            return Err(TableError::Cycle {
                path: vec![spec.name.clone(), spec.name],
            });
        }
        let dependencies = self.resolve(&spec.name, &expr)?;
        let value = self.eval_param(&expr, spec.unit, &spec.name, &HashMap::new())?;
        let id = ParamId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect("parameter ids left");
        self.by_name.insert(spec.name.clone(), id);
        self.items.push(Parameter {
            id,
            name: spec.name,
            expression,
            expr,
            unit: spec.unit,
            comment: spec.comment,
            kind: spec.kind,
            value,
            dependencies,
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
        let name = self.param(id)?.name.clone();
        let expr = parse(&name, expression)?;
        let expression = with_decimal_points(expression);
        let dependencies = self.resolve(&name, &expr)?;
        if let Some(cycle) = self.cycle_through(id, &dependencies) {
            return Err(self.cycle_error(&cycle));
        }
        let order = self.affected(id);
        let values = self.evaluate_pending(&order, Some((id, &expr, unit)))?;
        let i = self.index(id).expect("checked above");
        let p = &mut self.items[i];
        p.expression = expression.into_owned();
        p.expr = expr;
        p.unit = unit;
        p.dependencies = dependencies;
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
            rewrites.push((i, text, expr));
        }
        let rewritten = rewrites.iter().map(|(i, ..)| self.items[*i].id).collect();
        for (i, text, expr) in rewrites {
            self.items[i].expression = text;
            self.items[i].expr = expr;
        }
        self.by_name.remove(&old);
        self.by_name.insert(new_name.to_owned(), id);
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

    fn eval_param(
        &self,
        expr: &Expr,
        unit: Unit,
        name: &str,
        pending: &HashMap<ParamId, Quantity>,
    ) -> Result<Quantity, TableError> {
        let lookup = Overlay {
            table: self,
            values: pending,
        };
        expr.eval_as(&lookup, &self.context, unit)
            .map_err(|error| TableError::Eval {
                parameter: name.to_owned(),
                error,
            })
    }

    /// New values for `order`, without changing the table. `replace` gives
    /// a new expression and unit for one parameter.
    fn evaluate_pending(
        &self,
        order: &[ParamId],
        replace: Option<(ParamId, &Expr, Unit)>,
    ) -> Result<HashMap<ParamId, Quantity>, TableError> {
        let mut values = HashMap::new();
        for &id in order {
            let p = self.get(id).expect("ordered ids exist");
            let (expr, unit) = match replace {
                Some((replaced, expr, unit)) if replaced == id => (expr, unit),
                _ => (&p.expr, p.unit),
            };
            let value = self.eval_param(expr, unit, &p.name, &values)?;
            values.insert(id, value);
        }
        Ok(values)
    }

    /// Stores new values and returns the ids whose values changed.
    fn commit(&mut self, values: HashMap<ParamId, Quantity>) -> ChangeSet {
        let mut changed = ChangeSet::new();
        for (id, value) in values {
            let i = self.index(id).expect("evaluated ids exist");
            if self.items[i].value != value {
                self.items[i].value = value;
                changed.insert(id);
            }
        }
        changed
    }

    /// `id` and everything that depends on it, in evaluation order.
    fn affected(&self, id: ParamId) -> Vec<ParamId> {
        let mut dependents: HashMap<ParamId, Vec<ParamId>> = HashMap::new();
        for p in &self.items {
            for &dep in &p.dependencies {
                dependents.entry(dep).or_default().push(p.id);
            }
        }
        let mut affected = HashSet::from([id]);
        let mut work = vec![id];
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
}

fn parse(name: &str, expression: &str) -> Result<Expr, TableError> {
    Expr::parse(expression).map_err(|error| TableError::Parse {
        parameter: name.to_owned(),
        error,
    })
}

/// `text` with the references to `old` replaced by `new`.
fn replace_references(text: &str, expr: &Expr, old: &str, new: &str) -> String {
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
