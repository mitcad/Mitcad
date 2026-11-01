// SPDX-License-Identifier: MIT
//! Evaluation of expressions to quantities, with the unit rules of the
//! module documentation.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;

use super::Span;
use super::ast::{BinaryOp, ColumnPlace, Expr, Function, Node, NodeKind};
use super::format::{DEFAULT_DECIMALS, format_number, format_value};
use super::lookups::{ColumnError, MatchMode, TableColumn};
use super::units::{AngleUnit, Dims, LengthUnit, Quantity, Ratio, Unit};

/// Source of parameter values for evaluation.
pub trait Lookup {
    /// The value of the parameter `name`, or `None` if there is none (or
    /// it is a text parameter).
    fn lookup(&self, name: &str) -> Option<Quantity>;

    /// The text of the text parameter `name` (mitcad#664), or `None` if
    /// there is none. A name has a number or a text, not both.
    fn lookup_text(&self, _name: &str) -> Option<String> {
        None
    }

    /// The column `column` of the table `table` (mitcad#667), for a column
    /// reference `table.column` in the lookup functions (`xlookup`, `match`,
    /// `index`) and the aggregates: its unit and its cells from the first
    /// row down, borrowed from `self`.
    ///
    /// `Err(ColumnError::NoTable)` when there is no such table (the
    /// default, so lookups without tables need not implement it), and
    /// `Err(ColumnError::NoColumn)` when the table has no such column.
    /// Names are case-sensitive.
    fn column(&self, _table: &str, _column: &str) -> Result<TableColumn<'_>, ColumnError> {
        Err(ColumnError::NoTable)
    }
}

/// The value of an expression: a number with its dimensions or, since
/// mitcad#664, a text.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(Quantity),
    Text(String),
}

impl Value {
    /// The number, if it is one.
    pub fn as_number(&self) -> Option<Quantity> {
        match self {
            Self::Number(q) => Some(*q),
            Self::Text(_) => None,
        }
    }

    /// The text, if it is one.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Number(_) => None,
            Self::Text(text) => Some(text),
        }
    }
}

/// The longest text an expression may make, in bytes: texts that join
/// themselves through parameters would otherwise grow without end.
pub const MAX_TEXT_BYTES: usize = 1 << 20;

/// No parameters.
impl Lookup for () {
    fn lookup(&self, _name: &str) -> Option<Quantity> {
        None
    }
}

impl<S: BuildHasher> Lookup for HashMap<String, Quantity, S> {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self.get(name).copied()
    }
}

impl Lookup for BTreeMap<String, Quantity> {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self.get(name).copied()
    }
}

impl<F: Fn(&str) -> Option<Quantity>> Lookup for F {
    fn lookup(&self, name: &str) -> Option<Quantity> {
        self(name)
    }
}

/// Default units for dimensionless values that need a dimension (see the
/// module documentation, rules 3 to 5 and 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvalContext {
    pub default_length_unit: LengthUnit,
    pub default_angle_unit: AngleUnit,
}

impl Default for EvalContext {
    /// Millimetres and degrees.
    fn default() -> Self {
        Self::new(LengthUnit::Millimetre, AngleUnit::Degree)
    }
}

impl EvalContext {
    pub const fn new(default_length_unit: LengthUnit, default_angle_unit: AngleUnit) -> Self {
        Self {
            default_length_unit,
            default_angle_unit,
        }
    }

    /// The context for a parameter in `unit`: its length and angle units,
    /// if it has them, replace the defaults.
    pub fn for_unit(self, unit: Unit) -> Self {
        Self {
            default_length_unit: unit.length().map_or(self.default_length_unit, |(u, _)| u),
            default_angle_unit: unit.angle().map_or(self.default_angle_unit, |(u, _)| u),
        }
    }

    /// Size in canonical units of the default unit for `dims`, such as
    /// 25.4 for length in an inch context.
    pub fn default_factor(&self, dims: Dims) -> f64 {
        fn pow(base: f64, exp: Ratio) -> f64 {
            if exp.is_integer() {
                base.powi(exp.numer())
            } else {
                base.powf(exp.to_f64())
            }
        }
        pow(self.default_length_unit.millimetres(), dims.length)
            * pow(self.default_angle_unit.radians(), dims.angle)
    }
}

/// Evaluation error. Spans are byte ranges in the expression's source.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    UnknownReference {
        name: String,
        span: Span,
    },
    /// Operands of `+`, `-`, a comparison or similar have different
    /// dimensions.
    DimensionMismatch {
        operation: &'static str,
        left: Dims,
        right: Dims,
        span: Span,
    },
    /// An exponent, condition or function argument must be unitless.
    NotDimensionless {
        operation: &'static str,
        found: Dims,
        span: Span,
    },
    /// A trigonometric function got something else than an angle or a
    /// unitless value.
    NotAngle {
        function: &'static str,
        found: Dims,
        span: Span,
    },
    /// A value with units raised to a power that is not a simple fraction.
    InvalidExponent {
        exponent: f64,
        span: Span,
    },
    DimensionOverflow {
        span: Span,
    },
    DivisionByZero {
        span: Span,
    },
    /// Argument outside a function's domain, such as `sqrt(-1)`.
    Domain {
        operation: &'static str,
        span: Span,
    },
    NotFinite {
        span: Span,
    },
    /// The result does not have the expected unit's dimensions.
    WrongDimension {
        expected: Dims,
        found: Dims,
    },
    /// A unitless value (a count, factor or fraction) came out a length or
    /// an angle (mitcad#662); `hint` gives the value and the expression
    /// that gives a number.
    NotUnitless {
        found: Dims,
        hint: String,
    },
    /// A `let` name that a parameter has too.
    LocalShadowsParameter {
        name: String,
        span: Span,
    },
    /// `ifs` with no true condition, or `switch` with no matching value
    /// and no default.
    NoMatch {
        function: &'static str,
        span: Span,
    },
    /// Text where a number is expected (mitcad#664).
    NotNumber {
        span: Span,
    },
    /// A number where text is expected (mitcad#664).
    NotText {
        span: Span,
    },
    /// The values of a choice (`if`, `switch`, ...) or the operands of a
    /// comparison are text and numbers mixed (mitcad#664).
    MixedValues {
        operation: &'static str,
        span: Span,
    },
    /// The format of `text(x; format)` is not a number format.
    InvalidFormat {
        format: String,
        span: Span,
    },
    /// A text longer than [`MAX_TEXT_BYTES`].
    TextTooLong {
        span: Span,
    },
    // Table lookups (mitcad#667).
    /// A column reference, `Screws.dk`, where a value is expected (the
    /// parser refuses it first; only a tree built by hand gets here).
    ColumnOutsideLookup {
        table: String,
        column: String,
        span: Span,
    },
    /// A value where a lookup function needs a column (only a tree built
    /// by hand; the parser refuses it).
    NotColumn {
        function: &'static str,
        span: Span,
    },
    UnknownTable {
        name: String,
        span: Span,
    },
    UnknownColumn {
        table: String,
        column: String,
        span: Span,
    },
    /// `keys` and `results` of `xlookup` from two tables.
    DifferentTables {
        function: &'static str,
        span: Span,
    },
    /// No row matches the key: `column` is `Screws.Size`, `key` the key as
    /// an expression writes it (`'M7'`, `7 mm`). A value error.
    NotFound {
        column: String,
        key: String,
        mode: MatchMode,
        span: Span,
    },
    /// The cell looked up is empty: `row` counts from 1, `label` is the
    /// row's first cell. A value error.
    EmptyCell {
        column: String,
        row: usize,
        label: Option<String>,
        span: Span,
    },
    /// A row of `index` outside the column's rows. A value error.
    RowOutOfRange {
        column: String,
        row: f64,
        rows: usize,
        span: Span,
    },
    /// A match mode that the function does not have.
    InvalidMode {
        function: &'static str,
        mode: f64,
        span: Span,
    },
    /// A lookup feature not supported: wildcard matching, the search order.
    LookupUnsupported {
        what: &'static str,
        span: Span,
    },
    /// An approximate match (the next smaller or larger) with a text key.
    ApproximateText {
        function: &'static str,
        span: Span,
    },
    /// An aggregate whose columns have only empty cells and no other
    /// values. A value error.
    NoValues {
        function: &'static str,
        span: Span,
    },
}

impl EvalError {
    /// Where in the source the error is, if it is at a specific place.
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::UnknownReference { span, .. }
            | Self::DimensionMismatch { span, .. }
            | Self::NotDimensionless { span, .. }
            | Self::NotAngle { span, .. }
            | Self::InvalidExponent { span, .. }
            | Self::DimensionOverflow { span }
            | Self::DivisionByZero { span }
            | Self::Domain { span, .. }
            | Self::NotFinite { span }
            | Self::LocalShadowsParameter { span, .. }
            | Self::NoMatch { span, .. }
            | Self::NotNumber { span }
            | Self::NotText { span }
            | Self::MixedValues { span, .. }
            | Self::InvalidFormat { span, .. }
            | Self::TextTooLong { span }
            | Self::ColumnOutsideLookup { span, .. }
            | Self::NotColumn { span, .. }
            | Self::UnknownTable { span, .. }
            | Self::UnknownColumn { span, .. }
            | Self::DifferentTables { span, .. }
            | Self::NotFound { span, .. }
            | Self::EmptyCell { span, .. }
            | Self::RowOutOfRange { span, .. }
            | Self::InvalidMode { span, .. }
            | Self::LookupUnsupported { span, .. }
            | Self::ApproximateText { span, .. }
            | Self::NoValues { span, .. } => Some(*span),
            Self::WrongDimension { .. } | Self::NotUnitless { .. } => None,
        }
    }

    /// Errors about a value rather than the expression's structure: the
    /// ones `iferror` catches. A key not found in a table, an empty cell,
    /// a row outside a column and an aggregate of empty cells are value
    /// errors too (mitcad#667).
    fn is_value_error(&self) -> bool {
        matches!(
            self,
            Self::DivisionByZero { .. }
                | Self::Domain { .. }
                | Self::NotFinite { .. }
                | Self::NoMatch { .. }
                | Self::NotFound { .. }
                | Self::EmptyCell { .. }
                | Self::RowOutOfRange { .. }
                | Self::NoValues { .. }
        )
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownReference { name, .. } => write!(f, "unknown parameter {name}")?,
            Self::DimensionMismatch {
                operation,
                left,
                right,
                ..
            } => write!(f, "cannot apply “{operation}” to {left} and {right}")?,
            Self::NotDimensionless {
                operation, found, ..
            } => write!(f, "“{operation}” needs a unitless value, found {found}")?,
            Self::NotAngle {
                function, found, ..
            } => write!(f, "{function} needs an angle, found {found}")?,
            Self::InvalidExponent { exponent, .. } => write!(
                f,
                "a value with units can only be raised to a simple fraction, not {exponent}"
            )?,
            Self::DimensionOverflow { .. } => f.write_str("unit exponent out of range")?,
            Self::DivisionByZero { .. } => f.write_str("division by zero")?,
            Self::Domain { operation, .. } => write!(f, "argument out of range for “{operation}”")?,
            Self::NotFinite { .. } => f.write_str("result is not a finite number")?,
            Self::WrongDimension { expected, found } => {
                write!(f, "expected {expected}, found {found}")?;
            }
            Self::NotUnitless { hint, .. } => f.write_str(hint)?,
            Self::LocalShadowsParameter { name, .. } => write!(
                f,
                "{name} is a parameter: give the value in 'let' another name"
            )?,
            Self::NoMatch { function, .. } => {
                if *function == "ifs" {
                    f.write_str("no condition of 'ifs' is true")?;
                } else {
                    write!(f, "no value of {function} matches and there is no default")?;
                }
            }
            Self::NotNumber { .. } => f.write_str("text where a number is expected")?,
            Self::NotText { .. } => {
                f.write_str("a number where text is expected: make text of it with text(…) or &")?
            }
            Self::MixedValues { operation, .. } => {
                write!(f, "“{operation}” cannot mix text and numbers")?;
            }
            Self::InvalidFormat { format, .. } => write!(
                f,
                "“{format}” is not a number format such as \"0.00\" or \"0.0 in\""
            )?,
            Self::TextTooLong { .. } => {
                write!(f, "the text is longer than {MAX_TEXT_BYTES} bytes")?;
            }
            Self::ColumnOutsideLookup { table, column, .. } => write!(
                f,
                "{table}.{column} is a column: look a value up with XLOOKUP or INDEX"
            )?,
            Self::NotColumn { function, .. } => {
                write!(f, "{function} needs a column such as Table.Column here")?;
            }
            Self::UnknownTable { name, .. } => write!(f, "unknown table {name}")?,
            Self::UnknownColumn { table, column, .. } => {
                write!(f, "table {table} has no column {column}")?;
            }
            Self::DifferentTables { function, .. } => write!(
                f,
                "{function} needs its keys and results from columns of one table"
            )?,
            Self::NotFound {
                column, key, mode, ..
            } => {
                write!(f, "no row of {column} is {key}")?;
                match mode {
                    MatchMode::Exact => {}
                    MatchMode::NextSmaller => f.write_str(" or less")?,
                    MatchMode::NextLarger => f.write_str(" or more")?,
                }
            }
            Self::EmptyCell {
                column, row, label, ..
            } => {
                write!(f, "{column} is empty in row {row}")?;
                if let Some(label) = label {
                    write!(f, " ({label})")?;
                }
            }
            Self::RowOutOfRange {
                column, row, rows, ..
            } => write!(
                f,
                "row {} is outside the rows of {column} (1 to {rows})",
                format_number(*row, DEFAULT_DECIMALS)
            )?,
            Self::InvalidMode { function, mode, .. } => {
                let modes = if *function == "match" {
                    "0 (exact), 1 (exact or the next smaller) or -1 (exact or the next larger)"
                } else {
                    "0 (exact), -1 (exact or the next smaller) or 1 (exact or the next larger)"
                };
                write!(
                    f,
                    "{function} takes the mode {modes}, not {}",
                    format_number(*mode, DEFAULT_DECIMALS)
                )?;
            }
            Self::LookupUnsupported { what, .. } => write!(f, "{what} is not supported")?,
            Self::ApproximateText { function, .. } => write!(
                f,
                "{function} finds the next smaller or larger value only for a number key"
            )?,
            Self::NoValues { function, .. } => {
                write!(f, "{function} has no values: the column's cells are empty")?;
            }
        }
        if let Some(span) = self.span() {
            write!(f, " at byte {}", span.start)?;
        }
        Ok(())
    }
}

impl std::error::Error for EvalError {}

pub(crate) fn eval(
    root: &Node,
    lookup: &dyn Lookup,
    context: &EvalContext,
) -> Result<Quantity, EvalError> {
    Evaluator {
        lookup,
        context: *context,
        locals: RefCell::new(Vec::new()),
    }
    .eval(root)
}

/// The value of an expression, a number or a text (mitcad#664).
pub(crate) fn eval_value(
    root: &Node,
    lookup: &dyn Lookup,
    context: &EvalContext,
) -> Result<Value, EvalError> {
    Evaluator {
        lookup,
        context: *context,
        locals: RefCell::new(Vec::new()),
    }
    .value(root)
}

/// The text an expression gives, as a text parameter's value (mitcad#664);
/// a number is an error.
pub(crate) fn eval_text(
    root: &Node,
    lookup: &dyn Lookup,
    context: &EvalContext,
) -> Result<String, EvalError> {
    match eval_value(root, lookup, context)? {
        Value::Text(text) => Ok(text),
        Value::Number(_) => Err(EvalError::NotText { span: root.span }),
    }
}

pub(crate) fn eval_as(
    root: &Node,
    lookup: &dyn Lookup,
    context: &EvalContext,
    unit: Unit,
) -> Result<Quantity, EvalError> {
    if unit.is_text() {
        // A text parameter's value is no quantity (see `eval_text`).
        return Err(match eval_value(root, lookup, context)? {
            Value::Text(_) => EvalError::NotNumber { span: root.span },
            Value::Number(_) => EvalError::NotText { span: root.span },
        });
    }
    let context = context.for_unit(unit);
    let q = eval(root, lookup, &context)?;
    let dims = unit.dims();
    if q.dims == dims {
        Ok(q)
    } else if q.is_dimensionless() {
        finite(Quantity::new(unit.to_canonical(q.value), dims), root.span)
    } else if dims.is_dimensionless()
        && let Some(hint) = unitless_hint(root, q, &context)
    {
        Err(EvalError::NotUnitless {
            found: q.dims,
            hint,
        })
    } else {
        Err(EvalError::WrongDimension {
            expected: dims,
            found: q.dims,
        })
    }
}

/// What a unitless value (a count, factor or fraction) that came out a
/// length or an angle says (mitcad#662): the value in the default unit and
/// the expression that gives a number, `d1 / 3` → `d1 / 3 mm`, `5 mm` →
/// `5`, anything else divided by one unit. `None` for other dimensions.
fn unitless_hint(root: &Node, q: Quantity, context: &EvalContext) -> Option<String> {
    let (what, unit) = if q.dims == Dims::LENGTH {
        ("a length", Unit::of_length(context.default_length_unit))
    } else if q.dims == Dims::ANGLE {
        ("an angle", Unit::of_angle(context.default_angle_unit))
    } else {
        return None;
    };
    let span = root.span;
    let node = |kind: NodeKind| Box::new(Node::new(kind, span));
    let with_unit = |value: Box<Node>| node(NodeKind::WithUnit { value, unit });
    let number = match &root.kind {
        NodeKind::Binary {
            op: BinaryOp::Div,
            lhs,
            rhs,
        } if matches!(rhs.kind, NodeKind::Number(_)) => node(NodeKind::Binary {
            op: BinaryOp::Div,
            lhs: lhs.clone(),
            rhs: with_unit(rhs.clone()),
        }),
        NodeKind::WithUnit { value, unit } if unit.dims() == q.dims => value.clone(),
        _ => node(NodeKind::Binary {
            op: BinaryOp::Div,
            lhs: Box::new(root.clone()),
            rhs: with_unit(node(NodeKind::Number(1.0))),
        }),
    };
    Some(format!(
        "the result is {what} ({}); for a count write `{}`",
        format_value(q.value, unit, DEFAULT_DECIMALS),
        Expr::from_node(*number)
    ))
}

fn finite(q: Quantity, span: Span) -> Result<Quantity, EvalError> {
    if q.value.is_finite() {
        Ok(q)
    } else {
        Err(EvalError::NotFinite { span })
    }
}

/// Values closer than this (relative, or absolute near zero) are equal in
/// comparisons.
const EQUALITY_TOLERANCE: f64 = 1e-9;

pub(crate) fn compare(op: BinaryOp, a: f64, b: f64) -> bool {
    let close = (a - b).abs() <= EQUALITY_TOLERANCE * a.abs().max(b.abs()).max(1.0);
    match op {
        BinaryOp::Eq => close,
        BinaryOp::Ne => !close,
        BinaryOp::Lt => a < b && !close,
        BinaryOp::Le => a < b || close,
        BinaryOp::Gt => a > b && !close,
        BinaryOp::Ge => a > b || close,
        _ => unreachable!("not a comparison"),
    }
}

fn truth(value: bool) -> Quantity {
    Quantity::unitless(if value { 1.0 } else { 0.0 })
}

/// The number of a value where a number is expected.
fn number(value: Value, span: Span) -> Result<Quantity, EvalError> {
    match value {
        Value::Number(q) => Ok(q),
        Value::Text(_) => Err(EvalError::NotNumber { span }),
    }
}

/// `text`, unless it is longer than [`MAX_TEXT_BYTES`].
fn bounded(text: String, span: Span) -> Result<String, EvalError> {
    if text.len() > MAX_TEXT_BYTES {
        Err(EvalError::TextTooLong { span })
    } else {
        Ok(text)
    }
}

/// The text of a value where text is expected.
fn text(value: Value, span: Span) -> Result<String, EvalError> {
    match value {
        Value::Text(text) => Ok(text),
        Value::Number(_) => Err(EvalError::NotText { span }),
    }
}

/// A number pattern of `text(x; format)`: zeros and `#`s with at most one
/// decimal point (`.` or `,`). Zeros before the point give the fewest
/// digits there; after it, zeros are decimals always shown and `#`s ones
/// shown when they are not trailing zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NumberPattern {
    integer_digits: usize,
    min_decimals: usize,
    max_decimals: usize,
}

impl NumberPattern {
    /// Decimals beyond this are refused.
    const MAX_DECIMALS: usize = 15;

    fn parse(pattern: &str) -> Option<Self> {
        let mut parts = pattern.split(['.', ',']);
        let integer = parts.next()?;
        let fraction = parts.next().unwrap_or("");
        if parts.next().is_some() || fraction.len() > Self::MAX_DECIMALS {
            return None;
        }
        Some(Self {
            integer_digits: integer.chars().filter(|c| *c == '0').count(),
            min_decimals: fraction.chars().take_while(|c| *c == '0').count(),
            max_decimals: fraction.len(),
        })
    }

    /// `value` rounded half away from zero to the decimals, written by the
    /// pattern.
    fn write(self, value: f64) -> String {
        let scale = 10f64.powi(self.max_decimals as i32);
        let rounded = significant(value * scale).round() / scale;
        let decimals = self.max_decimals;
        let mut text = format!("{rounded:.decimals$}");
        if decimals > self.min_decimals {
            let point = text.find('.').unwrap_or(text.len());
            let keep = point + 1 + self.min_decimals;
            while text.len() > keep && text.ends_with('0') {
                text.pop();
            }
            if text.ends_with('.') {
                text.pop();
            }
        }
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.as_str()),
        };
        let integer = digits.find('.').unwrap_or(digits.len());
        let padding = self.integer_digits.saturating_sub(integer);
        let mut out = String::with_capacity(text.len() + padding);
        let all_zero = digits.chars().all(|c| c == '0' || c == '.');
        if negative && !all_zero {
            out.push('-');
        }
        out.extend(std::iter::repeat_n('0', padding));
        out.push_str(digits);
        out
    }
}

pub(crate) struct Evaluator<'a> {
    pub(crate) lookup: &'a dyn Lookup,
    pub(crate) context: EvalContext,
    /// Values of the enclosing `let`s' names, innermost last.
    locals: RefCell<Vec<(String, Value)>>,
}

impl Evaluator<'_> {
    /// The value of a node: a number or a text (mitcad#664). Nodes that can
    /// only give numbers are [`Evaluator::eval`]'s.
    pub(crate) fn value(&self, node: &Node) -> Result<Value, EvalError> {
        let span = node.span;
        match &node.kind {
            NodeKind::Text(text) => Ok(Value::Text(text.clone())),
            NodeKind::Reference(name) if self.lookup.lookup(name).is_none() => {
                match self.lookup.lookup_text(name) {
                    Some(text) => Ok(Value::Text(text)),
                    None => Err(EvalError::UnknownReference {
                        name: name.clone(),
                        span,
                    }),
                }
            }
            NodeKind::Local(name) => self
                .locals
                .borrow()
                .iter()
                .rev()
                .find(|(local, _)| local == name)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| EvalError::UnknownReference {
                    name: name.clone(),
                    span,
                }),
            NodeKind::Binary {
                op: BinaryOp::Concat,
                lhs,
                rhs,
            } => {
                let l = self.value(lhs)?;
                let r = self.value(rhs)?;
                let mut out = self.as_text(l);
                out.push_str(&self.as_text(r));
                bounded(out, span).map(Value::Text)
            }
            NodeKind::Call { function, args } => match function {
                Function::If
                | Function::Ifs
                | Function::Switch
                | Function::Choose
                | Function::IfError
                | Function::Let => self.choice(*function, args, span),
                // Table lookups (mitcad#667): a number or a text.
                Function::XLookup => self.xlookup(args, span),
                Function::Index => self.index(args),
                Function::Concat => {
                    let mut out = String::new();
                    for arg in args {
                        let value = self.value(arg)?;
                        out.push_str(&self.as_text(value));
                        if out.len() > MAX_TEXT_BYTES {
                            return Err(EvalError::TextTooLong { span });
                        }
                    }
                    Ok(Value::Text(out))
                }
                Function::Text => {
                    let x = number(self.value(&args[0])?, args[0].span)?;
                    let out = match args.get(1) {
                        None => self.number_text(x),
                        Some(format) => {
                            let pattern = text(self.value(format)?, format.span)?;
                            self.format_number_text(x, &pattern, format.span)?
                        }
                    };
                    Ok(Value::Text(out))
                }
                Function::Left | Function::Right => {
                    let t = text(self.value(&args[0])?, args[0].span)?;
                    let count = t.chars().count();
                    let n = match args.get(1) {
                        None => 1,
                        Some(arg) => {
                            let q = self.eval(arg)?;
                            let n = dimensionless(function.name(), q, arg.span)?.trunc();
                            if n < 0.0 {
                                return Err(EvalError::Domain {
                                    operation: function.name(),
                                    span: arg.span,
                                });
                            }
                            // A whole number, at most the length.
                            if n >= count as f64 { count } else { n as usize }
                        }
                    };
                    Ok(Value::Text(if *function == Function::Left {
                        t.chars().take(n).collect()
                    } else {
                        t.chars().skip(count - n).collect()
                    }))
                }
                _ => Ok(Value::Number(self.eval(node)?)),
            },
            _ => Ok(Value::Number(self.eval(node)?)),
        }
    }

    /// A value as text: a text as it is, a number as `text(x)` writes it.
    fn as_text(&self, value: Value) -> String {
        match value {
            Value::Text(text) => text,
            Value::Number(q) => self.number_text(q),
        }
    }

    /// A number as text in the default unit of its dimension (mitcad#664):
    /// `15 mm`, `30 deg`, `6` for a unitless one, with at most six
    /// decimals.
    pub(crate) fn number_text(&self, q: Quantity) -> String {
        let context = &self.context;
        match Unit::in_defaults(
            q.dims,
            context.default_length_unit,
            context.default_angle_unit,
        ) {
            Some(unit) => format_value(q.value, unit, DEFAULT_DECIMALS),
            None => format!("{} ({})", format_number(q.value, DEFAULT_DECIMALS), q.dims),
        }
    }

    /// `text(x; format)`: `x` written by the number pattern at the start of
    /// `format`, in the unit after it (`"0.00 in"`) or else in the default
    /// unit of its dimension, the unit after the number. A unitless `x` is
    /// read in the format's unit.
    fn format_number_text(
        &self,
        x: Quantity,
        format: &str,
        span: Span,
    ) -> Result<String, EvalError> {
        let invalid = || EvalError::InvalidFormat {
            format: format.to_owned(),
            span,
        };
        let trimmed = format.trim();
        let end = trimmed
            .find(|c: char| !matches!(c, '0' | '#' | '.' | ','))
            .unwrap_or(trimmed.len());
        let (pattern, rest) = trimmed.split_at(end);
        let rest = rest.trim();
        let context = &self.context;
        let unit = if rest.is_empty() {
            Unit::in_defaults(
                x.dims,
                context.default_length_unit,
                context.default_angle_unit,
            )
            .ok_or_else(invalid)?
        } else {
            let unit = Unit::parse(rest).map_err(|_| invalid())?;
            if unit.is_text() {
                return Err(invalid());
            }
            unit
        };
        let value = if x.dims == unit.dims() {
            unit.canonical_to_unit(x.value)
        } else if x.is_dimensionless() {
            x.value
        } else {
            return Err(EvalError::DimensionMismatch {
                operation: "text",
                left: x.dims,
                right: unit.dims(),
                span,
            });
        };
        let digits = if pattern.is_empty() {
            format_number(value, DEFAULT_DECIMALS)
        } else {
            NumberPattern::parse(pattern)
                .ok_or_else(invalid)?
                .write(value)
        };
        Ok(if unit.is_unitless() {
            digits
        } else {
            format!("{digits} {unit}")
        })
    }

    /// The value of a choice: `if`, `ifs`, `switch`, `choose`, `iferror`
    /// or `let`.
    fn choice(&self, function: Function, args: &[Node], span: Span) -> Result<Value, EvalError> {
        match function {
            Function::If => self.if_else(args, span),
            Function::Ifs => self.ifs(args, span),
            Function::Switch => self.switch(args, span),
            Function::Choose => self.choose(args, span),
            Function::IfError => self.if_error(args, span),
            _ => self.let_in(args, span),
        }
    }

    pub(crate) fn eval(&self, node: &Node) -> Result<Quantity, EvalError> {
        let span = node.span;
        let q = match &node.kind {
            NodeKind::Number(v) => Quantity::unitless(*v),
            NodeKind::Constant(c) => Quantity::unitless(c.value()),
            NodeKind::Reference(name) => match self.lookup.lookup(name) {
                Some(q) => q,
                // A text parameter (mitcad#664).
                None if self.lookup.lookup_text(name).is_some() => {
                    return Err(EvalError::NotNumber { span });
                }
                None => {
                    return Err(EvalError::UnknownReference {
                        name: name.clone(),
                        span,
                    });
                }
            },
            NodeKind::Text(_)
            | NodeKind::Binary {
                op: BinaryOp::Concat,
                ..
            } => return Err(EvalError::NotNumber { span }),
            NodeKind::Binary {
                op: op @ (BinaryOp::Eq | BinaryOp::Ne),
                lhs,
                rhs,
            } => {
                // Texts compare too: equal when they are the same.
                match (self.value(lhs)?, self.value(rhs)?) {
                    (Value::Number(l), Value::Number(r)) => self.binary(*op, l, r, span)?,
                    (Value::Text(l), Value::Text(r)) => truth((l == r) == (*op == BinaryOp::Eq)),
                    _ => {
                        return Err(EvalError::MixedValues {
                            operation: op.symbol(),
                            span,
                        });
                    }
                }
            }
            NodeKind::Neg(x) => {
                let q = self.eval(x)?;
                Quantity::new(-q.value, q.dims)
            }
            NodeKind::WithUnit { value, unit } => {
                let q = self.eval(value)?;
                Quantity::new(
                    q.value * unit.factor(),
                    mul_dims(q.dims, unit.dims(), span)?,
                )
            }
            NodeKind::Binary { op, lhs, rhs } => {
                let l = self.eval(lhs)?;
                let r = self.eval(rhs)?;
                self.binary(*op, l, r, span)?
            }
            NodeKind::Call { function, args } => self.call(*function, args, span)?,
            NodeKind::Local(_) => number(self.value(node)?, span)?,
            NodeKind::UnitText(unit) => Quantity::new(unit.factor(), unit.dims()),
            // Only a tree built by hand: the parser refuses it (mitcad#667).
            NodeKind::Column { table, column } => {
                return Err(EvalError::ColumnOutsideLookup {
                    table: table.clone(),
                    column: column.clone(),
                    span,
                });
            }
        };
        finite(q, span)
    }

    /// Gives the values of a choice one kind (mitcad#664): all numbers,
    /// unified as [`Evaluator::unify_all`] does, or all texts.
    fn unify_values(
        &self,
        operation: &'static str,
        values: &mut [Value],
        span: Span,
    ) -> Result<(), EvalError> {
        let mut numbers: Vec<Quantity> = values.iter().filter_map(Value::as_number).collect();
        if numbers.len() != values.len() {
            return if numbers.is_empty() {
                Ok(())
            } else {
                Err(EvalError::MixedValues { operation, span })
            };
        }
        self.unify_all(operation, &mut numbers, span)?;
        for (value, q) in values.iter_mut().zip(numbers) {
            *value = Value::Number(q);
        }
        Ok(())
    }

    /// `first` given the kind of `second`, as [`Evaluator::unify`] does for
    /// numbers; two texts stay as they are, a text and a number are an
    /// error.
    pub(crate) fn unify_pair(
        &self,
        operation: &'static str,
        first: Value,
        second: Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        match (first, second) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number(self.unify(operation, a, b, span)?.0))
            }
            (first @ Value::Text(_), Value::Text(_)) => Ok(first),
            _ => Err(EvalError::MixedValues { operation, span }),
        }
    }

    /// Reads a dimensionless value in the default unit of `dims`.
    pub(crate) fn promote(&self, q: Quantity, dims: Dims) -> Quantity {
        Quantity::new(q.value * self.context.default_factor(dims), dims)
    }

    /// Gives two operands the same dimensions, promoting a dimensionless
    /// one.
    pub(crate) fn unify(
        &self,
        operation: &'static str,
        l: Quantity,
        r: Quantity,
        span: Span,
    ) -> Result<(Quantity, Quantity), EvalError> {
        if l.dims == r.dims {
            Ok((l, r))
        } else if l.is_dimensionless() {
            Ok((self.promote(l, r.dims), r))
        } else if r.is_dimensionless() {
            Ok((l, self.promote(r, l.dims)))
        } else {
            Err(EvalError::DimensionMismatch {
                operation,
                left: l.dims,
                right: r.dims,
                span,
            })
        }
    }

    fn binary(
        &self,
        op: BinaryOp,
        l: Quantity,
        r: Quantity,
        span: Span,
    ) -> Result<Quantity, EvalError> {
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Rem => {
                let (l, r) = self.unify(op.symbol(), l, r, span)?;
                let value = match op {
                    BinaryOp::Add => l.value + r.value,
                    BinaryOp::Sub => l.value - r.value,
                    _ if r.value == 0.0 => return Err(EvalError::DivisionByZero { span }),
                    _ => l.value % r.value,
                };
                Ok(Quantity::new(value, l.dims))
            }
            BinaryOp::Mul => Ok(Quantity::new(
                l.value * r.value,
                mul_dims(l.dims, r.dims, span)?,
            )),
            BinaryOp::Div => {
                if r.value == 0.0 {
                    return Err(EvalError::DivisionByZero { span });
                }
                let dims = l
                    .dims
                    .checked_div(r.dims)
                    .ok_or(EvalError::DimensionOverflow { span })?;
                Ok(Quantity::new(l.value / r.value, dims))
            }
            BinaryOp::Pow => pow(l, r, span),
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => {
                let (l, r) = self.unify(op.symbol(), l, r, span)?;
                Ok(truth(compare(op, l.value, r.value)))
            }
            // `&` gives text, never a number (`eval` refuses it first).
            BinaryOp::Concat => Err(EvalError::NotNumber { span }),
        }
    }

    fn call(&self, function: Function, args: &[Node], span: Span) -> Result<Quantity, EvalError> {
        match function {
            Function::If
            | Function::Ifs
            | Function::Switch
            | Function::Choose
            | Function::IfError
            | Function::Let => return number(self.choice(function, args, span)?, span),
            // Text functions (mitcad#664).
            Function::Concat | Function::Text | Function::Left | Function::Right => {
                return Err(EvalError::NotNumber { span });
            }
            Function::Len => {
                let t = text(self.value(&args[0])?, args[0].span)?;
                return Ok(Quantity::unitless(t.chars().count() as f64));
            }
            // Table lookups (mitcad#667).
            Function::XLookup => return number(self.xlookup(args, span)?, span),
            Function::Index => return number(self.index(args)?, span),
            Function::Match => return self.match_row(args, span),
            _ => {}
        }
        let mut values = if function.column_place(0) == ColumnPlace::Allowed {
            // The aggregates take columns beside values (mitcad#667).
            self.aggregate_values(function.name(), args, span)?
        } else {
            args.iter()
                .map(|a| self.eval(a))
                .collect::<Result<Vec<_>, _>>()?
        };
        let name = function.name();
        let x = values[0];
        let x_span = args[0].span;
        let domain = || EvalError::Domain {
            operation: name,
            span: x_span,
        };
        let q = match function {
            Function::Sin
            | Function::Cos
            | Function::Tan
            | Function::Sinh
            | Function::Cosh
            | Function::Tanh => {
                let a = self.angle_arg(name, x, x_span)?;
                Quantity::unitless(match function {
                    Function::Sin => a.sin(),
                    Function::Cos => a.cos(),
                    Function::Tan => a.tan(),
                    Function::Sinh => a.sinh(),
                    Function::Cosh => a.cosh(),
                    _ => a.tanh(),
                })
            }
            Function::Asin | Function::Acos => {
                let v = dimensionless(name, x, x_span)?;
                // Allow floating point noise just outside [-1, 1].
                if v.abs() > 1.0 + 1e-12 {
                    return Err(domain());
                }
                let v = v.clamp(-1.0, 1.0);
                Quantity::angle(if function == Function::Asin {
                    v.asin()
                } else {
                    v.acos()
                })
            }
            Function::Atan => Quantity::angle(dimensionless(name, x, x_span)?.atan()),
            Function::Asinh => Quantity::angle(dimensionless(name, x, x_span)?.asinh()),
            Function::Acosh => {
                let v = dimensionless(name, x, x_span)?;
                if v < 1.0 {
                    return Err(domain());
                }
                Quantity::angle(v.acosh())
            }
            Function::Atanh => {
                let v = dimensionless(name, x, x_span)?;
                if v.abs() >= 1.0 {
                    return Err(domain());
                }
                Quantity::angle(v.atanh())
            }
            Function::Atan2 => {
                let (y, x) = self.unify(name, values[0], values[1], span)?;
                Quantity::angle(y.value.atan2(x.value))
            }
            Function::Sqrt => {
                if x.value < 0.0 {
                    return Err(domain());
                }
                let half = Ratio::new(1, 2).expect("valid ratio");
                let dims = x
                    .dims
                    .checked_pow(half)
                    .ok_or(EvalError::DimensionOverflow { span })?;
                Quantity::new(x.value.sqrt(), dims)
            }
            Function::Abs => Quantity::new(x.value.abs(), x.dims),
            Function::Sign => Quantity::unitless(if x.value < 0.0 { 0.0 } else { 1.0 }),
            Function::Exp => Quantity::unitless(dimensionless(name, x, x_span)?.exp()),
            Function::Ln | Function::Log => {
                let v = dimensionless(name, x, x_span)?;
                if v <= 0.0 {
                    return Err(domain());
                }
                Quantity::unitless(if function == Function::Ln {
                    v.ln()
                } else {
                    v.log10()
                })
            }
            Function::Floor | Function::Ceil | Function::Round if values.len() == 1 => {
                let unit = self.context.default_factor(x.dims);
                let v = x.value / unit;
                let v = match function {
                    Function::Floor => v.floor(),
                    Function::Ceil => v.ceil(),
                    _ => v.round(),
                };
                Quantity::new(v * unit, x.dims)
            }
            Function::Min | Function::Max => {
                self.unify_all(name, &mut values, span)?;
                let pick = |a: Quantity, b: Quantity| {
                    let a_wins = if function == Function::Min {
                        a.value <= b.value
                    } else {
                        a.value >= b.value
                    };
                    if a_wins { a } else { b }
                };
                values
                    .into_iter()
                    .reduce(pick)
                    .expect("min and max have a value")
            }
            Function::Pow => pow(values[0], values[1], span)?,
            Function::And | Function::Or => {
                let mut result = function == Function::And;
                for (q, arg) in values.iter().zip(args) {
                    let v = dimensionless(name, *q, arg.span)? != 0.0;
                    if function == Function::And {
                        result &= v;
                    } else {
                        result |= v;
                    }
                }
                truth(result)
            }
            Function::Not => truth(dimensionless(name, x, x_span)? == 0.0),
            Function::If
            | Function::Ifs
            | Function::Switch
            | Function::Choose
            | Function::IfError
            | Function::Let
            | Function::Concat
            | Function::Text
            | Function::Left
            | Function::Right
            | Function::Len
            | Function::XLookup
            | Function::Match
            | Function::Index => unreachable!("handled above"),
            // Functions in the style of spreadsheet formulas (mitcad#663).
            Function::Floor | Function::Ceil | Function::Ceiling | Function::MRound => {
                let round: fn(f64) -> f64 = match function {
                    Function::Floor => f64::floor,
                    Function::MRound => f64::round,
                    _ => f64::ceil,
                };
                match values.get(1) {
                    Some(&step) => self.round_to_step(name, x, step, span, round)?,
                    // `ceiling(x)`, as `ceil(x)`.
                    None => self.round_digits(name, x, None, round, false)?,
                }
            }
            Function::Round
            | Function::RoundUp
            | Function::RoundDown
            | Function::Trunc
            | Function::Int => {
                let round: fn(f64) -> f64 = match function {
                    Function::Round => f64::round,
                    Function::RoundUp => |v: f64| {
                        if v < 0.0 { v.floor() } else { v.ceil() }
                    },
                    Function::Int => f64::floor,
                    _ => f64::trunc,
                };
                let digits = (values.len() == 2).then(|| (values[1], args[1].span));
                self.round_digits(name, x, digits, round, true)?
            }
            Function::Even | Function::Odd => {
                let round: fn(f64) -> f64 = if function == Function::Even {
                    |v: f64| ((v.abs() / 2.0).ceil() * 2.0).copysign(v)
                } else {
                    |v: f64| {
                        let odd = ((v.abs() - 1.0) / 2.0).ceil() * 2.0 + 1.0;
                        if v < 0.0 { -odd } else { odd }
                    }
                };
                self.round_digits(name, x, None, round, true)?
            }
            Function::Mod => {
                let (a, b) = self.unify(name, values[0], values[1], span)?;
                if b.value == 0.0 {
                    return Err(EvalError::DivisionByZero { span });
                }
                let r = a.value % b.value;
                let r = if r != 0.0 && (r < 0.0) != (b.value < 0.0) {
                    r + b.value
                } else {
                    r
                };
                Quantity::new(r, a.dims)
            }
            Function::Quotient => {
                let (a, b) = (values[0], values[1]);
                if b.value == 0.0 {
                    return Err(EvalError::DivisionByZero { span });
                }
                let dims = a
                    .dims
                    .checked_div(b.dims)
                    .ok_or(EvalError::DimensionOverflow { span })?;
                let q = Quantity::new(a.value / b.value, dims);
                self.round_digits(name, q, None, f64::trunc, true)?
            }
            Function::Sum | Function::Average | Function::Median => {
                self.unify_all(name, &mut values, span)?;
                let dims = values[0].dims;
                let mut numbers: Vec<f64> = values.iter().map(|q| q.value).collect();
                let count = numbers.len() as f64;
                let value = match function {
                    Function::Sum => numbers.iter().sum::<f64>(),
                    Function::Average => numbers.iter().sum::<f64>() / count,
                    _ => {
                        numbers.sort_by(f64::total_cmp);
                        let middle = numbers.len() / 2;
                        if numbers.len().is_multiple_of(2) {
                            (numbers[middle - 1] + numbers[middle]) / 2.0
                        } else {
                            numbers[middle]
                        }
                    }
                };
                Quantity::new(value, dims)
            }
            Function::Product => {
                let mut product = Quantity::unitless(1.0);
                for q in &values {
                    product = Quantity::new(
                        product.value * q.value,
                        mul_dims(product.dims, q.dims, span)?,
                    );
                }
                product
            }
            Function::Clamp => {
                self.unify_all(name, &mut values, span)?;
                let (x, low, high) = (values[0], values[1].value, values[2].value);
                if low > high {
                    return Err(EvalError::Domain {
                        operation: name,
                        span,
                    });
                }
                Quantity::new(x.value.clamp(low, high), x.dims)
            }
            Function::Value => {
                let (x, unit) = self.unify(name, x, values[1], span)?;
                if unit.value == 0.0 {
                    return Err(EvalError::DivisionByZero { span });
                }
                Quantity::unitless(x.value / unit.value)
            }
            Function::Radians | Function::Degrees => {
                if x.dims == Dims::ANGLE {
                    x
                } else if x.is_dimensionless() {
                    // A plain number is in degrees for `radians` and in
                    // radians for `degrees`, as in spreadsheet formulas.
                    Quantity::angle(if function == Function::Radians {
                        x.value.to_radians()
                    } else {
                        x.value
                    })
                } else {
                    return Err(EvalError::NotAngle {
                        function: name,
                        found: x.dims,
                        span: x_span,
                    });
                }
            }
        };
        Ok(q)
    }

    /// `x` rounded by `round` to `digits` decimals (none: to a whole
    /// number) of the default unit of its dimension. With `snap`, the
    /// scaled value is first taken to 15 significant digits, so that
    /// floating point noise does not move it across a step:
    /// `roundup(0.1 + 0.2; 1)` is 0.3.
    fn round_digits(
        &self,
        operation: &'static str,
        x: Quantity,
        digits: Option<(Quantity, Span)>,
        round: fn(f64) -> f64,
        snap: bool,
    ) -> Result<Quantity, EvalError> {
        let digits = match digits {
            Some((d, span)) => dimensionless(operation, d, span)?
                .trunc()
                .clamp(-308.0, 308.0),
            None => 0.0,
        };
        // Within ±308 after the clamp.
        let scale = 10f64.powi(digits as i32);
        let unit = self.context.default_factor(x.dims);
        let v = x.value / unit * scale;
        let v = if snap { significant(v) } else { v };
        Ok(Quantity::new(round(v) / scale * unit, x.dims))
    }

    /// `x` rounded by `round` to a multiple of `step` (whose sign does not
    /// matter), in the dimension of `x`; a unitless step next to a
    /// dimensioned value is read in its default unit.
    fn round_to_step(
        &self,
        operation: &'static str,
        x: Quantity,
        step: Quantity,
        span: Span,
        round: fn(f64) -> f64,
    ) -> Result<Quantity, EvalError> {
        let (x, step) = self.unify(operation, x, step, span)?;
        let step = step.value.abs();
        if step == 0.0 {
            return Err(EvalError::DivisionByZero { span });
        }
        Ok(Quantity::new(
            round(significant(x.value / step)) * step,
            x.dims,
        ))
    }

    /// Evaluates `node`, keeping a value error (division by zero and the
    /// like) for the caller to weigh; other errors return at once.
    pub(crate) fn eval_lenient(&self, node: &Node) -> Result<Result<Value, EvalError>, EvalError> {
        match self.value(node) {
            Ok(value) => Ok(Ok(value)),
            Err(e) if e.is_value_error() => Ok(Err(e)),
            Err(e) => Err(e),
        }
    }

    /// The value of branch `taken`. As with `if`, every branch is
    /// evaluated: a value error counts only in the taken one, and the
    /// branches that evaluate must agree in dimensions (or all be texts).
    fn pick_branch(
        &self,
        operation: &'static str,
        branches: &[&Node],
        taken: usize,
        span: Span,
    ) -> Result<Value, EvalError> {
        if taken >= branches.len() {
            // Only a tree built by hand; the parser checks the arguments.
            return Err(EvalError::Domain { operation, span });
        }
        let mut values = Vec::with_capacity(branches.len());
        let mut at = 0;
        for (i, node) in branches.iter().enumerate() {
            match self.eval_lenient(node)? {
                Ok(q) => {
                    if i == taken {
                        at = values.len();
                    }
                    values.push(q);
                }
                Err(e) if i == taken => return Err(e),
                Err(_) => {}
            }
        }
        self.unify_values(operation, &mut values, span)?;
        Ok(values.swap_remove(at))
    }

    /// `ifs(c1; v1; c2; v2; ...)`: the value after the first true
    /// condition.
    fn ifs(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        let mut taken = None;
        for (k, pair) in args.chunks(2).enumerate() {
            let condition = &pair[0];
            let holds = match self.eval_lenient(condition)? {
                Ok(c) => {
                    let c = number(c, condition.span)?;
                    dimensionless("ifs", c, condition.span)? != 0.0
                }
                Err(e) if taken.is_none() => return Err(e),
                Err(_) => false,
            };
            if holds && taken.is_none() {
                taken = Some(k);
            }
        }
        let Some(taken) = taken else {
            return Err(EvalError::NoMatch {
                function: "ifs",
                span,
            });
        };
        let branches: Vec<&Node> = args.iter().skip(1).step_by(2).collect();
        self.pick_branch("ifs", &branches, taken, span)
    }

    /// `switch(x; k1; v1; ...; [default])`: the value after the first key
    /// equal to `x` (as `==` compares), else the default.
    fn switch(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        let x = self.value(&args[0])?;
        let rest = &args[1..];
        let pairs = rest.len() / 2;
        let default = rest.last().filter(|_| !rest.len().is_multiple_of(2));
        let mut taken = None;
        for k in 0..pairs {
            let key = &rest[2 * k];
            match self.eval_lenient(key)? {
                Ok(value) => {
                    // Texts match when they are the same (mitcad#664).
                    let equal = match (&x, &value) {
                        (Value::Number(x), Value::Number(value)) => {
                            let (a, b) = self.unify("switch", *x, *value, key.span)?;
                            compare(BinaryOp::Eq, a.value, b.value)
                        }
                        (Value::Text(x), Value::Text(value)) => x == value,
                        _ => {
                            return Err(EvalError::MixedValues {
                                operation: "switch",
                                span: key.span,
                            });
                        }
                    };
                    if taken.is_none() && equal {
                        taken = Some(k);
                    }
                }
                Err(e) if taken.is_none() => return Err(e),
                Err(_) => {}
            }
        }
        let taken = match (taken, default) {
            (Some(k), _) => k,
            // The default is the last branch.
            (None, Some(_)) => pairs,
            (None, None) => {
                return Err(EvalError::NoMatch {
                    function: "switch",
                    span,
                });
            }
        };
        let branches: Vec<&Node> = (0..pairs)
            .map(|k| &rest[2 * k + 1])
            .chain(default)
            .collect();
        self.pick_branch("switch", &branches, taken, span)
    }

    /// `choose(i; v1; v2; ...)`: the `i`th value, counting from 1; a
    /// fraction is cut off.
    fn choose(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        let index = self.eval(&args[0])?;
        let index = dimensionless("choose", index, args[0].span)?.trunc();
        let branches: Vec<&Node> = args[1..].iter().collect();
        if index < 1.0 || index > branches.len() as f64 {
            return Err(EvalError::Domain {
                operation: "choose",
                span: args[0].span,
            });
        }
        // A whole number from 1 to the number of values.
        let taken = index as usize - 1;
        self.pick_branch("choose", &branches, taken, span)
    }

    /// `iferror(x; fallback)`: `x`, or the fallback when `x` has a value
    /// error. Other errors (unknown names, units) are not caught; when
    /// both evaluate, their dimensions must agree.
    fn if_error(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        match self.eval_lenient(&args[0])? {
            Ok(value) => match self.eval_lenient(&args[1])? {
                Ok(fallback) => self.unify_pair("iferror", value, fallback, span),
                Err(_) => Ok(value),
            },
            Err(_) => self.value(&args[1]),
        }
    }

    /// `let(n1; v1; n2; v2; ...; result)`: the result with each name
    /// standing for its value. A name must not be a parameter's.
    fn let_in(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        let outer = self.locals.borrow().len();
        let result = self.let_values(args, span);
        self.locals.borrow_mut().truncate(outer);
        result
    }

    fn let_values(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        // Only a tree built by hand can be malformed; the parser checks.
        let malformed = || EvalError::Domain {
            operation: "let",
            span,
        };
        let Some((result, pairs)) = args.split_last() else {
            return Err(malformed());
        };
        for pair in pairs.chunks(2) {
            let [name_node, value] = pair else {
                return Err(malformed());
            };
            let NodeKind::Local(name) = &name_node.kind else {
                return Err(malformed());
            };
            if self.lookup.lookup(name).is_some() || self.lookup.lookup_text(name).is_some() {
                return Err(EvalError::LocalShadowsParameter {
                    name: name.clone(),
                    span: name_node.span,
                });
            }
            let value = self.value(value)?;
            self.locals.borrow_mut().push((name.clone(), value));
        }
        self.value(result)
    }

    /// `if(c; a; b)`: the branch not taken is evaluated too, but only
    /// structural errors in it (unknown names, dimensions, text and
    /// numbers mixed) count.
    fn if_else(&self, args: &[Node], span: Span) -> Result<Value, EvalError> {
        let c = self.eval(&args[0])?;
        let condition = dimensionless("if", c, args[0].span)? != 0.0;
        let (taken, other) = if condition {
            (&args[1], &args[2])
        } else {
            (&args[2], &args[1])
        };
        let value = self.value(taken)?;
        match self.value(other) {
            Ok(o) => self.unify_pair("if", value, o, span),
            Err(e) if e.is_value_error() => Ok(value),
            Err(e) => Err(e),
        }
    }

    /// Gives all values the dimensions of the first dimensioned one,
    /// promoting dimensionless ones.
    fn unify_all(
        &self,
        operation: &'static str,
        values: &mut [Quantity],
        span: Span,
    ) -> Result<(), EvalError> {
        let Some(target) = values
            .iter()
            .map(|q| q.dims)
            .find(|d| !d.is_dimensionless())
        else {
            return Ok(());
        };
        for q in values.iter_mut() {
            if q.dims == target {
                continue;
            }
            if !q.is_dimensionless() {
                return Err(EvalError::DimensionMismatch {
                    operation,
                    left: target,
                    right: q.dims,
                    span,
                });
            }
            *q = self.promote(*q, target);
        }
        Ok(())
    }

    /// An angle in radians; a dimensionless value is in the default angle
    /// unit.
    fn angle_arg(&self, function: &'static str, q: Quantity, span: Span) -> Result<f64, EvalError> {
        if q.dims == Dims::ANGLE {
            Ok(q.value)
        } else if q.is_dimensionless() {
            Ok(q.value * self.context.default_angle_unit.radians())
        } else {
            Err(EvalError::NotAngle {
                function,
                found: q.dims,
                span,
            })
        }
    }
}

pub(crate) fn dimensionless(
    operation: &'static str,
    q: Quantity,
    span: Span,
) -> Result<f64, EvalError> {
    if q.is_dimensionless() {
        Ok(q.value)
    } else {
        Err(EvalError::NotDimensionless {
            operation,
            found: q.dims,
            span,
        })
    }
}

/// `v` to 15 significant digits, as spreadsheet formulas compute, so that
/// `(0.1 + 0.2) * 10` is 3 and `1.005 * 100` is 100.5 when rounded.
fn significant(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    format!("{v:.14e}").parse().unwrap_or(v)
}

fn mul_dims(a: Dims, b: Dims, span: Span) -> Result<Dims, EvalError> {
    a.checked_mul(b)
        .ok_or(EvalError::DimensionOverflow { span })
}

fn pow(base: Quantity, exponent: Quantity, span: Span) -> Result<Quantity, EvalError> {
    let e = dimensionless("exponent", exponent, span)?;
    let dims = if base.is_dimensionless() {
        Dims::NONE
    } else {
        let ratio =
            Ratio::approximate(e).ok_or(EvalError::InvalidExponent { exponent: e, span })?;
        base.dims
            .checked_pow(ratio)
            .ok_or(EvalError::DimensionOverflow { span })?
    };
    let value = base.value.powf(e);
    if value.is_nan() {
        return Err(EvalError::Domain {
            operation: "^",
            span,
        });
    }
    Ok(Quantity::new(value, dims))
}
