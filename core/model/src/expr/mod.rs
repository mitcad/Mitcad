// SPDX-License-Identifier: MIT
//! Parameter expressions with units. The language is compatible with the
//! parameter expressions of .f3d designs, so that imported parameters
//! evaluate the same; the notes below say where a choice was made.
//!
//! An expression such as `d1 * 2 + 5 mm`, `30 deg` or `( 13 / 3 ) * 1 mm`
//! is parsed into an [`Expr`], evaluated against named values (a
//! [`Lookup`]) to a [`Quantity`], and checked against the unit the
//! parameter expects. [`ParameterTable`] keeps named parameters with their
//! expression text and evaluates them in dependency order.
//!
//! Internal (canonical) units are the millimetre and the radian. .f3d
//! designs store centimetres and radians; see [`f3d`].
//!
//! # Grammar
//!
//! ```text
//! expression     = comparison ;
//! comparison     = additive { compare_op additive } ;
//! compare_op     = "<" | "<=" | ">" | ">=" | "==" | "<>" | "!=" ;
//! additive       = multiplicative { ( "+" | "-" ) multiplicative } ;
//! multiplicative = unary { ( "*" | "/" | "%" ) unary } ;
//! unary          = ( "-" | "+" ) unary | power ;
//! power          = postfix [ "^" unary ] ;
//! postfix        = primary [ unit_suffix ] ;
//! unit_suffix    = unit [ "^" [ "+" | "-" ] digits ] ;
//! primary        = number
//!                | constant
//!                | name
//!                | function "(" [ expression { ( ";" | "," ) expression } ] ")"
//!                | "(" expression ")" ;
//! number         = ( digits [ point { digit } ] | point digits )
//!                  [ ( "e" | "E" ) [ "+" | "-" ] digits ] ;
//! point          = "." | "," ;  (a comma only as described below)
//! name           = ( letter | "_" ) { letter | digit | "_" } ;
//! constant       = "PI" | "E" ;
//! unit           = "um" | "micron" | "µm" | "mm" | "cm" | "m" | "km"
//!                | "in" | "inch" | "ft" | "foot" | "yd" | "mi" | "mil"
//!                | "rad" | "deg" | "°" | "grad" ;
//! function       = "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
//!                | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh"
//!                | "sqrt" | "abs" | "sign" | "exp" | "ln" | "log"
//!                | "floor" | "ceil" | "round" | "min" | "max" | "pow"
//!                | "if" | "and" | "or" | "not" ;
//! ```
//!
//! Whitespace (any Unicode white space) may appear between tokens. Names,
//! units, functions and constants are case-sensitive. A
//! `name` that is a unit, function or constant is reserved and cannot name
//! a parameter ([`is_valid_name`]).
//!
//! Precedence, from lowest to highest: comparisons, `+ -`, `* / %`, unary
//! minus, `^`, unit suffix (the expressions stored in `.f3d` files read the
//! same: parentheses, exponentiation, negation, multiplication and
//! division, addition and subtraction). Hence `-2^2` is `-(2^2) = -4` and
//! `2^-1 = 0.5`. `^` is right-associative, as in mathematics:
//! `2^3^2 = 2^9 = 512`. All other binary operators are left-associative.
//! There is no `**`; using it is a parse error that points to `^`.
//!
//! A unit suffix binds tighter than any operator and applies to the
//! primary just before it: `2 ^ 3 mm` is `2 ^ (3 mm)` (an error, since an
//! exponent must be unitless) and `-5 mm` is `-(5 mm)`. A suffix may carry
//! an integer power, so `10 mm^2` is ten square millimetres, whereas
//! `(10 mm)^2` is a hundred.
//!
//! Function arguments are separated by `;` (which avoids
//! clashing with the decimal comma); `,` is accepted as well. The canonical
//! printing ([`Expr`]'s `Display`) uses `;`.
//!
//! The decimal separator is `.` or `,`, so that `1,5 mm` is `1.5 mm`. A
//! comma directly between two digits is a decimal comma (`1,5`,
//! `0,25 mm`, `2,5e3`, `d1 * 1,5`), and so is one that starts a number
//! with no value before it (`,5`, `2 * ,5`) or ends one where no other
//! argument can follow (`5,` at the end, before `)`, `;` or an operator
//! other than a sign). Every other comma separates arguments: `max(a, b)`,
//! `max(a,b)`, `max(1, 5)`, `max(1,-5)`. Hence `max(1,5)` is `max(1.5)`,
//! and its error says to write `max(1; 5)`. A number has one decimal
//! separator and no thousands separators: `1,000` is `1.0`, and `1 000`
//! is an error. [`with_decimal_points`] gives the text with points, which
//! is how parameters keep their expressions.
//!
//! # Units and dimensions
//!
//! Every value is a [`Quantity`]: a number in canonical units and its
//! [`Dims`], the exponents of length and angle (`length^a · angle^b`, with
//! rational exponents so that `sqrt(d1 * d2)` is a length). The rules:
//!
//! 1. A plain number is dimensionless. A unit suffix multiplies by the
//!    unit: `5 mm`, `(a + 2) mm` = `(a + 2) * 1 mm`, `2 * 1 in`.
//! 2. `*` and `/` combine dimensions. A dimensionless operand is a scale
//!    factor: `d1 * 2` and `d1 / 2` are lengths, `d1 * d2` is an area, and
//!    `d1 / d2` is dimensionless.
//! 3. `+`, `-`, `%`, comparisons, `min`, `max`, `atan2` and the branches
//!    of `if` need operands of the same dimension. A dimensionless operand
//!    next to a dimensioned one is *promoted*: it is read in the default
//!    unit of that dimension. In a millimetre document `d1 + 5` is
//!    `d1 + 5 mm` and `30 deg + 5` is `35 deg`. Mixing two different
//!    dimensions (`10 mm + 30 deg`, `d1 + d1 * d2`) is an error.
//! 4. The result is checked against the expected unit
//!    ([`Expr::eval_as`]). Equal dimensions are accepted; a dimensionless
//!    result is promoted to the expected unit, so `10` is 10 mm and
//!    `10 / 2` is 5 mm in a millimetre parameter (so are the stored values
//!    of `.f3d` expressions). Consequently `d1 / d2` in a length parameter
//!    is the ratio in millimetres, the permissive reading. Anything else
//!    (`d1 * d2` in a length parameter, `5 mm` in a unitless one) is an
//!    error.
//! 5. Default units come from [`EvalContext`]: the document's length unit
//!    and the angle unit (degrees). When an expression is
//!    evaluated for an expected unit, that unit's length and angle units
//!    replace the defaults ([`EvalContext::for_unit`]); a parameter in
//!    inches reads bare numbers as inches.
//! 6. `^` needs a dimensionless exponent. A dimensioned base needs an
//!    exponent that is a simple fraction (denominator at most 12):
//!    `d1 ^ 2` is an area, `(d1 * d2) ^ 0.5` a length.
//! 7. Trigonometric functions take an angle. A dimensionless argument is
//!    read in the default angle unit, so `sin(30)` is `sin(30 deg)` = 0.5
//!    and `sin(PI / 2)` is the sine of about 1.57 degrees. Inverse
//!    functions take a dimensionless value and return an angle. As in .f3d
//!    expressions, the hyperbolic functions
//!    behave the same way (`sinh` takes an angle, `asinh` returns one).
//! 8. `sqrt` halves the exponents, `abs` keeps them, `exp`, `ln` and
//!    `log` (base 10) need dimensionless arguments. `floor`, `ceil` and
//!    `round` (half away from zero) round a dimensioned value in the
//!    default unit of its dimension: `round(12.7 mm)` is 13 mm.
//! 9. `sign` is 0 for a negative value and 1 otherwise (as in .f3d
//!    expressions; not the mathematical signum). It accepts any dimension
//!    and returns a dimensionless number.
//! 10. Comparisons and `and`, `or`, `not` return 1 or 0. Equality uses a
//!     relative tolerance of 1e-9 (absolute 1e-9 canonical units near
//!     zero). Conditions must be dimensionless; non-zero is true.
//! 11. `if(c; a; b)` evaluates both branches. Errors such as division by
//!     zero in the branch not taken are ignored; if both evaluate, their
//!     dimensions must agree.
//! 12. Results must be finite: division by zero, out-of-domain arguments
//!     (`sqrt(-1)`, `ln(0)`) and overflow are errors.
//!
//! Not supported: `random()` (not reproducible), and the constants
//! `Gravity` and `SpeedOfLight` (no time dimension), which .f3d
//! expressions have. Their names are reserved and give a clear error.
//! `atan2` is a Mitcad addition.

mod ast;
mod eval;
pub mod f3d;
mod format;
mod lexer;
mod parser;
mod table;
mod units;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::fmt;

pub use ast::{Arity, BinaryOp, Constant, Expr, Function, Node, NodeKind};
pub use eval::{EvalContext, EvalError, Lookup};
pub use format::{
    DEFAULT_DECIMALS, format_number, format_number_exact, format_value, value_to_expression,
};
pub use parser::{ParseError, ParseErrorKind};
pub(crate) use table::replace_references;
pub use table::{ChangeSet, ParamId, ParamKind, ParamSpec, Parameter, ParameterTable, TableError};
pub use units::{AngleUnit, DimensionError, Dims, LengthUnit, Quantity, Ratio, Unit};

/// Byte range in an expression's source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// Error from parsing or evaluating an expression text.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprError {
    Parse(ParseError),
    Eval(EvalError),
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => e.fmt(f),
            Self::Eval(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ExprError {}

impl From<ParseError> for ExprError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<EvalError> for ExprError {
    fn from(e: EvalError) -> Self {
        Self::Eval(e)
    }
}

/// Parses `text` and evaluates it for the expected `unit`
/// (see [`Expr::eval_as`]).
pub fn evaluate(
    text: &str,
    lookup: &dyn Lookup,
    context: &EvalContext,
    unit: Unit,
) -> Result<Quantity, ExprError> {
    Ok(Expr::parse(text)?.eval_as(lookup, context, unit)?)
}

/// `text` with its decimal commas written as points (`1,5 mm` as
/// `1.5 mm`, `d1 * ,5` as `d1 * .5`), all else as it was; it parses to
/// the same expression. Text that does not lex is returned as it is.
pub fn with_decimal_points(text: &str) -> Cow<'_, str> {
    let commas = lexer::decimal_commas(text);
    if commas.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut points = text.to_owned();
    for at in commas {
        points.replace_range(at..=at, ".");
    }
    Cow::Owned(points)
}

/// Names that .f3d expressions reserve but Mitcad does not evaluate.
const UNSUPPORTED_NAMES: [&str; 3] = ["random", "Gravity", "SpeedOfLight"];

/// Whether `name` is a unit, function or constant (or one of the
/// unsupported reserved names), and so cannot name a parameter.
pub fn is_reserved_name(name: &str) -> bool {
    units::is_unit_name(name)
        || Function::from_name(name).is_some()
        || Constant::from_name(name).is_some()
        || UNSUPPORTED_NAMES.contains(&name)
}

/// Whether `name` can name a parameter: a letter or `_` followed by
/// letters, digits and `_`, and not reserved. Names are case-sensitive.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if is_name_start(c) => {}
        _ => return false,
    }
    chars.all(is_name_char) && !is_reserved_name(name)
}

pub(crate) fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

pub(crate) fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
