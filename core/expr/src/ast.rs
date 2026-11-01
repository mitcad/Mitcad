// SPDX-License-Identifier: MIT
//! Expression syntax tree.

use std::fmt;
use std::str::FromStr;

use super::Span;
use super::eval::{self, EvalContext, EvalError, Lookup, Value};
use super::parser::{self, ParseError};
use super::units::{Quantity, Unit};

/// Parsed expression, such as `d1 * 2 + 5 mm`.
///
/// Equality compares the structure and ignores source positions, so an
/// expression equals the parse of its canonical printing (`Display`).
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    root: Node,
}

impl Expr {
    /// Parses an expression. Errors carry byte positions in `text`.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        parser::parse(text).map(|root| Self { root })
    }

    /// Wraps a syntax tree built by hand.
    pub fn from_node(root: Node) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Names of the referenced parameters, each once, in order of first
    /// appearance.
    pub fn references(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for (name, _) in self.reference_spans() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    /// Every parameter reference with its position in the source text, in
    /// source order.
    pub fn reference_spans(&self) -> Vec<(&str, Span)> {
        let mut refs = Vec::new();
        self.root.walk(&mut |node| {
            if let NodeKind::Reference(name) = &node.kind {
                refs.push((name.as_str(), node.span));
            }
        });
        refs
    }

    /// Whether the expression references no parameters and no tables.
    pub fn is_constant(&self) -> bool {
        self.reference_spans().is_empty() && self.column_reference_spans().is_empty()
    }

    /// Whether the expression is one number as typed (mitcad#1092): a
    /// literal with or without a sign and a unit, such as `90`, `-7.5`,
    /// `450 deg` or `(-1.5) rad`. A parameter name, a constant, a function
    /// call or any operator other than the leading sign makes it something
    /// computed.
    pub fn is_plain_number(&self) -> bool {
        fn number(node: &Node) -> bool {
            matches!(node.kind, NodeKind::Number(_))
        }
        fn signed(node: &Node) -> bool {
            match &node.kind {
                NodeKind::Neg(inner) => number(inner),
                _ => number(node),
            }
        }
        match &self.root.kind {
            // `-450 deg`: the sign before the unit.
            NodeKind::Neg(inner) => match &inner.kind {
                NodeKind::WithUnit { value, .. } => number(value),
                _ => number(inner),
            },
            // `450 deg` or `(-450) deg`.
            NodeKind::WithUnit { value, .. } => signed(value),
            _ => number(&self.root),
        }
    }

    /// The table columns the expression uses (mitcad#667), as `(table,
    /// column)` pairs, each once, in order of first appearance: `Screws.dk`
    /// is `("Screws", "dk")`. Parameter names are [`Expr::references`].
    pub fn table_references(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for (table, column, _) in self.column_reference_spans() {
            if !pairs.contains(&(table, column)) {
                pairs.push((table, column));
            }
        }
        pairs
    }

    /// Every column reference with its position in the source text, in
    /// source order (mitcad#667). The span covers `Table.Column`; as no
    /// spaces stand around the dot, the table's name is its first
    /// `table.len()` bytes and the column's name its last `column.len()`.
    pub fn column_reference_spans(&self) -> Vec<(&str, &str, Span)> {
        let mut refs = Vec::new();
        self.root.walk(&mut |node| {
            if let NodeKind::Column { table, column } = &node.kind {
                refs.push((table.as_str(), column.as_str(), node.span));
            }
        });
        refs
    }

    /// Evaluates the expression. A dimensionless result stays
    /// dimensionless; use [`Expr::eval_as`] to apply a parameter's unit.
    pub fn eval(&self, lookup: &dyn Lookup, context: &EvalContext) -> Result<Quantity, EvalError> {
        eval::eval(&self.root, lookup, context)
    }

    /// Evaluates the expression for a parameter in `unit`: the unit's
    /// length and angle units become the defaults for bare numbers, a
    /// dimensionless result is read in `unit`, and any other result must
    /// have the unit's dimensions.
    pub fn eval_as(
        &self,
        lookup: &dyn Lookup,
        context: &EvalContext,
        unit: Unit,
    ) -> Result<Quantity, EvalError> {
        eval::eval_as(&self.root, lookup, context, unit)
    }

    /// Evaluates the expression to a number or a text (mitcad#664).
    pub fn eval_value(
        &self,
        lookup: &dyn Lookup,
        context: &EvalContext,
    ) -> Result<Value, EvalError> {
        eval::eval_value(&self.root, lookup, context)
    }

    /// Evaluates the expression as a text parameter's value (mitcad#664):
    /// a number is an error that says how to make text of it.
    pub fn eval_text(
        &self,
        lookup: &dyn Lookup,
        context: &EvalContext,
    ) -> Result<String, EvalError> {
        eval::eval_text(&self.root, lookup, context)
    }
}

impl FromStr for Expr {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Syntax tree node with its source position.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
}

impl Node {
    pub fn new(kind: NodeKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Visits this node and its descendants in source order.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Node)) {
        f(self);
        match &self.kind {
            NodeKind::Number(_)
            | NodeKind::Constant(_)
            | NodeKind::Reference(_)
            | NodeKind::Local(_)
            | NodeKind::UnitText(_)
            | NodeKind::Text(_)
            | NodeKind::Column { .. } => {}
            NodeKind::Neg(x) => x.walk(f),
            NodeKind::WithUnit { value, .. } => value.walk(f),
            NodeKind::Binary { lhs, rhs, .. } => {
                lhs.walk(f);
                rhs.walk(f);
            }
            NodeKind::Call { args, .. } => args.iter().for_each(|a| a.walk(f)),
        }
    }

    /// Precedence level for printing: higher binds tighter.
    pub(crate) fn precedence(&self) -> u8 {
        match &self.kind {
            NodeKind::Binary { op, .. } => op.precedence(),
            NodeKind::Neg(_) => PREC_UNARY,
            NodeKind::WithUnit { .. } => PREC_POSTFIX,
            NodeKind::Number(_)
            | NodeKind::Constant(_)
            | NodeKind::Reference(_)
            | NodeKind::Local(_)
            | NodeKind::UnitText(_)
            | NodeKind::Text(_)
            | NodeKind::Column { .. }
            | NodeKind::Call { .. } => PREC_PRIMARY,
        }
    }
}

/// Positions are ignored.
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

pub(crate) const PREC_COMPARE: u8 = 1;
/// `&` (mitcad#664): below `+ -`, above comparisons, as in spreadsheets.
pub(crate) const PREC_CONCAT: u8 = 2;
pub(crate) const PREC_ADD: u8 = 3;
pub(crate) const PREC_MUL: u8 = 4;
pub(crate) const PREC_UNARY: u8 = 5;
pub(crate) const PREC_POW: u8 = 6;
pub(crate) const PREC_POSTFIX: u8 = 7;
pub(crate) const PREC_PRIMARY: u8 = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Non-negative literal; `-2` is `Neg(2)`.
    Number(f64),
    Constant(Constant),
    Reference(String),
    Neg(Box<Node>),
    /// A value followed by a unit, such as `5 mm` or `(a + 2) in^2`.
    WithUnit {
        value: Box<Node>,
        unit: Unit,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Node>,
        rhs: Box<Node>,
    },
    Call {
        function: Function,
        args: Vec<Node>,
    },
    /// A name given by `let(name; value; ...; result)`: where it is
    /// given (the `let`'s argument) and where it is used. Not a
    /// parameter reference.
    Local(String),
    /// A unit in quotes, `"mm"`: the second argument of `value(x; "mm")`.
    /// It evaluates to one of the unit.
    UnitText(Unit),
    /// A text literal, `'6qt'` or `"Box "` (mitcad#664): its characters,
    /// a doubled quote taken as one.
    Text(String),
    /// A column of a table, `Screws.dk` (mitcad#667): a list of cells,
    /// accepted only as an argument of the lookup functions and the
    /// aggregates ([`Function::column_place`]).
    Column {
        table: String,
        column: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    /// Floating point remainder, `%`.
    Rem,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    /// `&`: the operands as text, joined (mitcad#664).
    Concat,
}

impl BinaryOp {
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Concat => "&",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Pow => "^",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Ne => "<>",
        }
    }

    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::Lt | Self::Le | Self::Gt | Self::Ge | Self::Eq | Self::Ne => PREC_COMPARE,
            Self::Concat => PREC_CONCAT,
            Self::Add | Self::Sub => PREC_ADD,
            Self::Mul | Self::Div | Self::Rem => PREC_MUL,
            Self::Pow => PREC_POW,
        }
    }

    pub const fn is_comparison(self) -> bool {
        self.precedence() == PREC_COMPARE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Constant {
    Pi,
    E,
}

impl Constant {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pi => "PI",
            Self::E => "E",
        }
    }

    pub const fn value(self) -> f64 {
        match self {
            Self::Pi => std::f64::consts::PI,
            Self::E => std::f64::consts::E,
        }
    }

    /// The constant `name` names. `PI` is case-insensitive (`pi`, `Pi`);
    /// `E` is not, as `e` is a common name and the exponent letter of
    /// numbers such as `2e3`.
    pub fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("PI") {
            Some(Self::Pi)
        } else if name == "E" {
            Some(Self::E)
        } else {
            None
        }
    }
}

/// Number of arguments a function takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    AtLeast(usize),
    /// From the first to the second, both included: `round(x; [digits])`.
    Range(usize, usize),
}

impl Arity {
    pub const fn accepts(self, n: usize) -> bool {
        match self {
            Self::Exact(k) => n == k,
            Self::AtLeast(k) => n >= k,
            Self::Range(min, max) => min <= n && n <= max,
        }
    }

    /// The fewest arguments accepted.
    pub const fn min(self) -> usize {
        match self {
            Self::Exact(k) | Self::AtLeast(k) | Self::Range(k, _) => k,
        }
    }
}

impl fmt::Display for Arity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(n) => write!(f, "{n}"),
            Self::AtLeast(n) => write!(f, "at least {n}"),
            Self::Range(min, max) => write!(f, "{min} to {max}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Function {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    /// `atan2(y; x)`, a Mitcad addition.
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    Sqrt,
    Abs,
    /// 0 for negative values, 1 otherwise (as in .f3d expressions).
    Sign,
    Exp,
    /// Natural logarithm.
    Ln,
    /// Base 10 logarithm.
    Log,
    Floor,
    Ceil,
    Round,
    Min,
    Max,
    Pow,
    If,
    And,
    Or,
    Not,
    // Functions in the style of spreadsheet formulas (mitcad#663). `floor`,
    // `ceil` and `round` above take an optional step or digits as well.
    /// `ceiling(x; [step])`, the same as `ceil`.
    Ceiling,
    RoundUp,
    RoundDown,
    MRound,
    Trunc,
    Int,
    /// `mod(a; b)`: the remainder with the sign of `b` (`%` keeps the sign
    /// of `a`).
    Mod,
    Quotient,
    Even,
    Odd,
    Sum,
    Average,
    Product,
    Median,
    Clamp,
    Ifs,
    Switch,
    Choose,
    IfError,
    /// `let(name; value; ...; result)`: names local to the formula.
    Let,
    /// `value(x; "unit")`: the number of `x` in the unit.
    Value,
    Radians,
    Degrees,
    // Text functions (mitcad#664). Their names are functions only before
    // `(`: elsewhere they are ordinary names ([`Function::is_reserved`]).
    /// `concat(a; b; ...)`: the values as text, joined, as `&` joins.
    Concat,
    /// `text(x; [format])`: a number as text.
    Text,
    /// `left(t; [n])`: the first `n` characters (one by default).
    Left,
    /// `right(t; [n])`: the last `n` characters (one by default).
    Right,
    /// `len(t)`: the number of characters.
    Len,
    // Table lookups (mitcad#667). Their names are functions only before
    // `(`, as the text functions' are.
    /// `xlookup(key; keys; results; [if_not_found]; [mode])`: the cell of
    /// `results` in the row where `keys` matches `key`.
    XLookup,
    /// `match(key; keys; [mode])`: the row (from 1) where `keys` matches
    /// `key`.
    Match,
    /// `index(column; row)`: the cell of `column` in row `row`.
    Index,
}

/// Where a function takes a column reference such as `Screws.dk`
/// (mitcad#667).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPlace {
    /// The argument must be a column: `keys` and `results` of `xlookup`.
    Required,
    /// A column or a value: the aggregates' arguments.
    Allowed,
    /// A value only.
    No,
}

pub(super) const FUNCTIONS: [(Function, &str, Arity); 60] = [
    (Function::Sin, "sin", Arity::Exact(1)),
    (Function::Cos, "cos", Arity::Exact(1)),
    (Function::Tan, "tan", Arity::Exact(1)),
    (Function::Asin, "asin", Arity::Exact(1)),
    (Function::Acos, "acos", Arity::Exact(1)),
    (Function::Atan, "atan", Arity::Exact(1)),
    (Function::Atan2, "atan2", Arity::Exact(2)),
    (Function::Sinh, "sinh", Arity::Exact(1)),
    (Function::Cosh, "cosh", Arity::Exact(1)),
    (Function::Tanh, "tanh", Arity::Exact(1)),
    (Function::Asinh, "asinh", Arity::Exact(1)),
    (Function::Acosh, "acosh", Arity::Exact(1)),
    (Function::Atanh, "atanh", Arity::Exact(1)),
    (Function::Sqrt, "sqrt", Arity::Exact(1)),
    (Function::Abs, "abs", Arity::Exact(1)),
    (Function::Sign, "sign", Arity::Exact(1)),
    (Function::Exp, "exp", Arity::Exact(1)),
    (Function::Ln, "ln", Arity::Exact(1)),
    (Function::Log, "log", Arity::Exact(1)),
    (Function::Floor, "floor", Arity::Range(1, 2)),
    (Function::Ceil, "ceil", Arity::Range(1, 2)),
    (Function::Round, "round", Arity::Range(1, 2)),
    (Function::Min, "min", Arity::AtLeast(2)),
    (Function::Max, "max", Arity::AtLeast(2)),
    (Function::Pow, "pow", Arity::Exact(2)),
    (Function::If, "if", Arity::Exact(3)),
    (Function::And, "and", Arity::AtLeast(1)),
    (Function::Or, "or", Arity::AtLeast(1)),
    (Function::Not, "not", Arity::Exact(1)),
    (Function::Ceiling, "ceiling", Arity::Range(1, 2)),
    (Function::RoundUp, "roundup", Arity::Range(1, 2)),
    (Function::RoundDown, "rounddown", Arity::Range(1, 2)),
    (Function::MRound, "mround", Arity::Exact(2)),
    (Function::Trunc, "trunc", Arity::Range(1, 2)),
    (Function::Int, "int", Arity::Exact(1)),
    (Function::Mod, "mod", Arity::Exact(2)),
    (Function::Quotient, "quotient", Arity::Exact(2)),
    (Function::Even, "even", Arity::Exact(1)),
    (Function::Odd, "odd", Arity::Exact(1)),
    (Function::Sum, "sum", Arity::AtLeast(1)),
    (Function::Average, "average", Arity::AtLeast(1)),
    (Function::Product, "product", Arity::AtLeast(1)),
    (Function::Median, "median", Arity::AtLeast(1)),
    (Function::Clamp, "clamp", Arity::Exact(3)),
    (Function::Ifs, "ifs", Arity::AtLeast(2)),
    (Function::Switch, "switch", Arity::AtLeast(3)),
    (Function::Choose, "choose", Arity::AtLeast(2)),
    (Function::IfError, "iferror", Arity::Exact(2)),
    (Function::Let, "let", Arity::AtLeast(3)),
    (Function::Value, "value", Arity::Exact(2)),
    (Function::Radians, "radians", Arity::Exact(1)),
    (Function::Degrees, "degrees", Arity::Exact(1)),
    (Function::Concat, "concat", Arity::AtLeast(1)),
    (Function::Text, "text", Arity::Range(1, 2)),
    (Function::Left, "left", Arity::Range(1, 2)),
    (Function::Right, "right", Arity::Range(1, 2)),
    (Function::Len, "len", Arity::Exact(1)),
    // A sixth argument of xlookup (the search order) parses, to be refused
    // with its own message.
    (Function::XLookup, "xlookup", Arity::Range(3, 6)),
    (Function::Match, "match", Arity::Range(2, 3)),
    (Function::Index, "index", Arity::Exact(2)),
];

impl Function {
    /// The function `name` names, in any case: `floor`, `FLOOR`, `Floor`.
    pub fn from_name(name: &str) -> Option<Self> {
        FUNCTIONS
            .iter()
            .find(|(_, n, _)| n.eq_ignore_ascii_case(name))
            .map(|&(f, _, _)| f)
    }

    /// Whether the function's name is reserved, so that it cannot name a
    /// parameter. The text functions (mitcad#664) are not: `Left`, `Text`
    /// or `len` are common parameter names, so these names are functions
    /// only when `(` follows (`len(Label)`), and references elsewhere
    /// (`len * 2`). The lookup functions (mitcad#667) are not either, so
    /// that parameters named `Index` or `Match` keep their names.
    pub fn is_reserved(self) -> bool {
        !matches!(
            self,
            Self::Concat
                | Self::Text
                | Self::Left
                | Self::Right
                | Self::Len
                | Self::XLookup
                | Self::Match
                | Self::Index
        )
    }

    /// Whether argument `index` (from 0) takes a column reference
    /// (mitcad#667): `keys` and `results` of `xlookup`, `keys` of `match`
    /// and `column` of `index` must be columns; `min`, `max`, `sum`,
    /// `average` and `median` take columns beside values.
    pub fn column_place(self, index: usize) -> ColumnPlace {
        match (self, index) {
            (Self::XLookup, 1 | 2) | (Self::Match, 1) | (Self::Index, 0) => ColumnPlace::Required,
            (Self::Min | Self::Max | Self::Sum | Self::Average | Self::Median, _) => {
                ColumnPlace::Allowed
            }
            _ => ColumnPlace::No,
        }
    }

    /// Whether the arguments come in pairs: `ifs(c1; v1; c2; v2)`, and
    /// `let(n1; v1; n2; v2; result)` with the result after the pairs.
    pub(crate) fn takes_pairs(self) -> bool {
        matches!(self, Self::Ifs | Self::Let)
    }

    fn entry(self) -> &'static (Function, &'static str, Arity) {
        FUNCTIONS
            .iter()
            .find(|(f, _, _)| *f == self)
            .expect("every function is listed")
    }

    pub fn name(self) -> &'static str {
        self.entry().1
    }

    pub fn arity(self) -> Arity {
        self.entry().2
    }
}
