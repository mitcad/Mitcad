// SPDX-License-Identifier: MIT
//! Expression syntax tree.

use std::fmt;
use std::str::FromStr;

use super::Span;
use super::eval::{self, EvalContext, EvalError, Lookup};
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

    /// Whether the expression references no parameters.
    pub fn is_constant(&self) -> bool {
        self.reference_spans().is_empty()
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
            NodeKind::Number(_) | NodeKind::Constant(_) | NodeKind::Reference(_) => {}
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
pub(crate) const PREC_ADD: u8 = 2;
pub(crate) const PREC_MUL: u8 = 3;
pub(crate) const PREC_UNARY: u8 = 4;
pub(crate) const PREC_POW: u8 = 5;
pub(crate) const PREC_POSTFIX: u8 = 6;
pub(crate) const PREC_PRIMARY: u8 = 7;

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
}

impl BinaryOp {
    pub const fn symbol(self) -> &'static str {
        match self {
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

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "PI" => Some(Self::Pi),
            "E" => Some(Self::E),
            _ => None,
        }
    }
}

/// Number of arguments a function takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    AtLeast(usize),
}

impl Arity {
    pub const fn accepts(self, n: usize) -> bool {
        match self {
            Self::Exact(k) => n == k,
            Self::AtLeast(k) => n >= k,
        }
    }

    /// The fewest arguments accepted.
    pub const fn min(self) -> usize {
        match self {
            Self::Exact(k) | Self::AtLeast(k) => k,
        }
    }
}

impl fmt::Display for Arity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(n) => write!(f, "{n}"),
            Self::AtLeast(n) => write!(f, "at least {n}"),
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
}

const FUNCTIONS: [(Function, &str, Arity); 29] = [
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
    (Function::Floor, "floor", Arity::Exact(1)),
    (Function::Ceil, "ceil", Arity::Exact(1)),
    (Function::Round, "round", Arity::Exact(1)),
    (Function::Min, "min", Arity::AtLeast(2)),
    (Function::Max, "max", Arity::AtLeast(2)),
    (Function::Pow, "pow", Arity::Exact(2)),
    (Function::If, "if", Arity::Exact(3)),
    (Function::And, "and", Arity::AtLeast(1)),
    (Function::Or, "or", Arity::AtLeast(1)),
    (Function::Not, "not", Arity::Exact(1)),
];

impl Function {
    pub fn from_name(name: &str) -> Option<Self> {
        FUNCTIONS
            .iter()
            .find(|(_, n, _)| *n == name)
            .map(|&(f, _, _)| f)
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
