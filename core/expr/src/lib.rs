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
//! comparison     = concatenation { compare_op concatenation } ;
//! concatenation  = additive { "&" additive } ;
//! compare_op     = "<" | "<=" | ">" | ">=" | "==" | "<>" | "!="
//!                | "=" ;  (only inside parentheses, see below)
//! additive       = multiplicative { ( "+" | "-" ) multiplicative } ;
//! multiplicative = unary { ( "*" | "/" | "%" ) unary } ;
//! unary          = ( "-" | "+" ) unary | power ;
//! power          = postfix [ "^" unary ] ;
//! postfix        = primary [ unit_suffix ]
//!                | number "'" number inch_mark ;  (feet and inches)
//! unit_suffix    = ( unit | mark ) [ "^" [ "+" | "-" ] digits ] ;
//! mark           = inch_mark | "'" ;  (only right after a number, see below)
//! inch_mark      = '"' | "''" ;
//! primary        = number
//!                | text
//!                | constant [ "(" ")" ]
//!                | name
//!                | column  (only as an argument, rule 31)
//!                | function "(" [ expression { sep expression } ] ")"
//!                | "let" "(" name sep expression { sep name sep expression }
//!                  sep expression ")"
//!                | "value" "(" expression sep ( unit_text | expression ) ")"
//!                | "(" expression ")" ;
//! sep            = ";" | "," ;
//! number         = ( digits [ point { digit } ] | point digits )
//!                  [ ( "e" | "E" ) [ "+" | "-" ] digits ] ;
//! point          = "." | "," ;  (a comma only as described below)
//! name           = ( letter | "_" ) { letter | digit | "_" } ;
//! column         = name "." name ;  (no spaces around the dot)
//! constant       = "PI" | "E" ;
//! unit           = "um" | "micron" | "µm" | "mm" | "cm" | "m" | "km"
//!                | "in" | "inch" | "ft" | "foot" | "yd" | "mi" | "mil"
//!                | "rad" | "deg" | "°" | "grad" ;
//! unit_text      = '"' unit [ "^" [ "-" ] digits ]
//!                  { ( "*" | "/" ) unit [ "^" [ "-" ] digits ] } '"' ;
//! function       = "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
//!                | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh"
//!                | "sqrt" | "abs" | "sign" | "exp" | "ln" | "log"
//!                | "floor" | "ceil" | "round" | "min" | "max" | "pow"
//!                | "if" | "and" | "or" | "not"
//!                | "ceiling" | "roundup" | "rounddown" | "mround" | "trunc"
//!                | "int" | "mod" | "quotient" | "even" | "odd"
//!                | "sum" | "average" | "product" | "median" | "clamp"
//!                | "ifs" | "switch" | "choose" | "iferror"
//!                | "radians" | "degrees"
//!                | "concat" | "text" | "left" | "right" | "len"
//!                | "xlookup" | "match" | "index" ;
//! text           = "'" { character other than "'" | "''" } "'"
//!                | '"' { character other than '"' | '""' } '"' ;
//! ```
//!
//! Whitespace (any Unicode white space) may appear between tokens.
//! Function names and `PI` are case-insensitive (mitcad#663): `FLOOR`,
//! `Floor` and `floor` are one function, and `pi` is `PI`; printing uses
//! the lower-case names. Parameter names, units and `E` are case-sensitive
//! (`e` is a name, `MM` is not a unit). A `name` that is a unit, function
//! or constant, in any case where it is case-insensitive (`Max`, `pi`), is
//! reserved and cannot name a parameter ([`is_valid_name`]). Older files
//! and imports with such a parameter rename it (`Max` to `Max_1`) with a
//! warning, and their expressions follow.
//!
//! A single `=` compares inside parentheses, a function's or a grouping's:
//! `if(GridX = 6; a; b)` is `if(GridX == 6; a; b)`. At the top level of a
//! field `name = expression` names the field's parameter (mitcad#231),
//! which the caller splits off before parsing, so there a single `=` is an
//! error. `==` compares everywhere.
//!
//! Precedence, from lowest to highest: comparisons, `&`, `+ -`, `* / %`,
//! unary minus, `^`, unit suffix (the expressions stored in `.f3d` files read the
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
//! The inch and foot marks (mitcad#1531): a `"` (or two single quotes,
//! `''`) right after a number, with or without spaces between, is the unit
//! `in`, and a `'` there is `ft`: `.875"` is `0.875 in`, `2.5 "` is
//! `2.5 in`, `3'` is `3 ft`. A foot value followed by an inch value adds
//! up, `3' 6"` is `3 ft + 6 in` (42 inches) and binds as one value
//! (`2 * 3' 6"` is 84 inches). Only a number takes a mark; anywhere else a
//! quote starts a text, so `'Width'`, `"mm"` and `value(x; "in")` read as
//! before. Printing writes the unit names (`0.875 in`).
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
//! Functions in the style of spreadsheet formulas (mitcad#663):
//!
//! 13. Rounding to digits: `round(x; [digits])` (half away from zero),
//!     `roundup` (away from zero), `rounddown` and `trunc` (towards zero),
//!     `int(x)` (down). Digits count decimals of the default unit of the
//!     value's dimension, as rule 8 rounds; negative digits round to tens,
//!     hundreds: `round(12.345 mm; 1)` is 12.3 mm, `round(1234; -2)` 1200.
//! 14. Rounding to a step: `floor(x; [step])`, `ceiling(x; [step])` (also
//!     `ceil`) and `mround(x; step)` (nearest, halves away from zero). The
//!     step has the value's dimension; a unitless step next to a
//!     dimensioned value is read in its default unit (`floor(d1; 0.5)` is
//!     `floor(d1; 0.5 mm)`); its sign does not matter; a zero step is a
//!     division by zero. The result keeps the dimension.
//! 15. With a step or digits, and in `roundup`, `rounddown`, `trunc`,
//!     `int`, `quotient`, `even` and `odd`, the scaled value or the
//!     quotient is first taken to 15 significant digits, as spreadsheets
//!     compute, so floating point noise does not cross a step:
//!     `roundup(0.1 + 0.2; 1)` is 0.3, `round(1.005; 2)` 1.01. `floor`,
//!     `ceil`, `ceiling` and `round` with one argument round as rule 8,
//!     without it, so expressions of .f3d designs evaluate as before.
//! 16. `mod(a; b)` is the remainder with the sign of `b` (`mod(-10; 3)` is
//!     2), where `%` keeps the sign of `a` (-1); operands as for `%`.
//!     `quotient(a; b)` is `a / b` cut towards zero, with the dimensions of
//!     `a / b`. `even(x)` and `odd(x)` round away from zero to an even or
//!     odd whole number (`odd(0)` is 1), in the default unit of `x`.
//! 17. `sum`, `average` and `median` take one or more values of the same
//!     dimension (unitless ones promoted, rule 3); `product` multiplies
//!     dimensions as `*` does. `clamp(x; low; high)` needs three values of
//!     one dimension and `low <= high`.
//! 18. `ifs(c1; v1; c2; v2; ...)` gives the value after the first true
//!     condition; `switch(x; k1; v1; ...; [default])` the value after the
//!     first key equal to `x` (as `==`; the key and `x` need one
//!     dimension), else the default; `choose(i; v1; v2; ...)` the `i`th
//!     value, counting from 1 (a fraction is cut off, other indexes are out
//!     of range). With no true condition, or no equal key and no default,
//!     the result is an error. As with `if` (rule 11), every value is
//!     evaluated, a value error counts only in the one chosen, and the
//!     values that evaluate must agree in dimensions.
//! 19. `iferror(x; fallback)` is `x`, or the fallback where `x` has a value
//!     error: division by zero, an argument out of range, a result that is
//!     not finite, no match in `ifs` or `switch`. Unknown names and unit
//!     errors are not caught. When both evaluate they need one dimension.
//! 20. `let(name1; value1; ...; result)` names values inside one formula:
//!     each name can be used from the argument after its value on, up to
//!     the end of the `let` (`let(r; d1 / 2; PI * r ^ 2)`). The names are
//!     not parameter references; they must be valid, not reserved, given
//!     once in a `let` (an inner `let` may reuse an outer name), and not a
//!     parameter's name (an error when the formula is evaluated).
//! 21. `value(x; "unit")` is the number of `x` in the unit, unitless:
//!     `value(d1; "mm")` is 15 for 15 mm, the readable form of
//!     `d1 / 1 mm`. The unit is in double quotes (`"mm"`, `"in^2"`,
//!     `"deg"`), or a value such as `1 mm`; `x` must have its dimension (a
//!     unitless `x` is read in the default unit). Quoted text there is the
//!     unit, not a text (rule 23).
//! 22. `radians(x)` and `degrees(x)` give an angle: a plain number is in
//!     degrees for `radians` and in radians for `degrees`, as in
//!     spreadsheets, and an angle stays as it is. So `sin(radians(30))` is
//!     0.5, and `radians(180)` is 180 degrees; its number of radians is
//!     `value(radians(180); "rad")`. `PI()` is `PI`.
//!
//! Texts (mitcad#664): an expression's value is a number with its
//! dimensions ([`Quantity`]) or a text ([`Value`]); a text parameter (unit
//! `Text`, [`Unit::TEXT`]) has a text.
//!
//! 23. A text is written in single quotes, as `.f3d` expressions write them
//!     (`'6qt'`), or in double quotes (`"Box "`). A quote of the same kind
//!     inside is doubled (`'It''s'`, `"say ""hi"""`); the other kind is an
//!     ordinary character (`"It's"`). Printing uses single quotes
//!     ([`text_literal`]).
//! 24. `a & b` joins its operands as text; a number is written as `text(x)`
//!     writes it, so `"Box " & GridX & "x" & GridY` is `Box 6x4`. `&` binds
//!     less tightly than `+` and `-` and more tightly than comparisons:
//!     `"n = " & a + 1` is `"n = " & (a + 1)`.
//! 25. `text(x)` writes a number in the default unit of its dimension with
//!     at most six decimals: `15 mm`, `30 deg`, `6`. `text(x; format)`
//!     writes it by a number pattern of zeros and `#`s with at most one
//!     decimal point (`.` or `,`): zeros before the point are the fewest
//!     digits there (`"000"` writes 7 as `007`), zeros after it decimals
//!     always written, `#`s decimals written when they are not trailing
//!     zeros (`"0.0#"`); halves round away from zero. A unit may follow the
//!     pattern (`"0.00 in"`, or only `"in"` for up to six decimals): the
//!     value is written in it, a unitless one read in it. Without one, the
//!     default unit of the value's dimension follows the number;
//!     `text(value(d1; "mm"); "0.0")` writes a length's number alone. No
//!     thousands separators.
//! 26. `concat(a; b; ...)` joins its values as `&` does. `left(t; [n])` and
//!     `right(t; [n])` give the first and the last `n` characters (one by
//!     default; all of them when `n` is more; `n` is cut to a whole number
//!     and must not be negative); `len(t)` the number of characters,
//!     unitless.
//! 27. A text is never accepted where a number is expected, nor a number
//!     where a text is: `'5' + 1`, `sin(Label)`, a text parameter in a
//!     length, `left(5; 1)` and a number as a text parameter's value are
//!     errors ("text where a number is expected", "a number where text is
//!     expected"). Only `&`, `concat` and `text` make text of numbers.
//! 28. `==` (and `=` in parentheses) and `<>` compare two texts: equal when
//!     they have the same characters, case included. A text compared with
//!     a number, or by `<`, `<=`, `>` or `>=`, is an error. `if`, `ifs`,
//!     `switch`, `choose`, `iferror` and `let` take texts as values; the
//!     values of one choice are all texts or all numbers, and so are a
//!     `switch`'s keys and value.
//! 29. The text functions' names (`concat`, `text`, `left`, `right`,
//!     `len`, in any case) are functions only before `(`; elsewhere they are
//!     names, so parameters named `Text`, `Left` or `len` keep their names.
//! 30. A text is at most [`MAX_TEXT_BYTES`] bytes long.
//!
//! Table lookups (mitcad#667, `docs/table-lookups.md`): a design keeps
//! named tables of constants, which [`Lookup::column`] gives column by
//! column ([`TableColumn`]).
//!
//! 31. `Table.Column` is a column reference, a list of cells; no spaces
//!     stand around the dot, and the column's name may be a reserved word
//!     (`T.in`). It is accepted only where rules 32 to 35 take a column;
//!     anywhere else it is an error ("Screws.dk is a column: look a value
//!     up with XLOOKUP or INDEX").
//! 32. `xlookup(key; keys; results; [if_not_found]; [mode])` is the cell of
//!     `results` in the row where `keys` matches `key`; `keys` and
//!     `results` are columns of one table. `match(key; keys; [mode])` is
//!     that row's number, counting from 1, unitless. `index(column; row)`
//!     is the cell of `column` in row `row` (a fraction is cut off; a row
//!     outside the column is an error, as `choose` treats an index).
//! 33. Rows are searched from the top, empty key cells skipped; of several
//!     matching rows the first wins. The mode is exact by default (0 of
//!     both); exact, else the next smaller (the largest cell below the
//!     key) is `-1` of `xlookup` and `1` of `match`; exact, else the next
//!     larger is `1` of `xlookup` and `-1` of `match`. The approximate
//!     modes look at every row, so the column need not be sorted. Wildcard
//!     matching (`xlookup` mode 2) and `xlookup`'s sixth argument (the
//!     search order) are not supported.
//! 34. A number key needs a number column of its dimension (a unitless key
//!     or column read in the default unit, as rule 3 promotes), and is
//!     equal to a cell as `==` compares (rule 10). A text key needs a text
//!     column and is equal to a cell of the same characters, case included
//!     (rule 28); the approximate modes need a number key. A text key in a
//!     number column, or a number key in a text column, is an error (rule
//!     27).
//! 35. `min`, `max`, `sum`, `average` and `median` take number columns
//!     beside values: a column gives its non-empty cells (`max(T.d)` alone
//!     is accepted). A column of only empty cells makes `sum` zero and the
//!     others an error.
//! 36. A cell is a value of its column's unit: `5.5` in an `mm` column is
//!     5.5 mm, `0.25` in an `in` column 6.35 mm; a unitless column gives a
//!     number, a text column a text. From there rules 3, 4 and 27 apply.
//! 37. No matching row is a value error (`no row of Screws.Size is 'M7'`),
//!     and so are an empty cell looked up and an `index` row outside the
//!     column: `iferror` catches them. `xlookup`'s `if_not_found` is used
//!     instead of no match; as with `if` (rule 11) it is evaluated anyway,
//!     a value error in it counts only when it is used, and it must agree
//!     with `results` in kind and dimension. `xlookup`, `match` and
//!     `index` are functions only before `(` (as rule 29), so parameters
//!     named `Index` or `Match` keep their names.
//!
//! Not supported: `random()` (not reproducible), and the constants
//! `Gravity` and `SpeedOfLight` (no time dimension), which .f3d
//! expressions have. Their names are reserved and give a clear error.
//! `atan2` is a Mitcad addition.

pub mod assistance;
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
use std::collections::HashMap;
use std::fmt;

pub use ast::{Arity, BinaryOp, ColumnPlace, Constant, Expr, Function, Node, NodeKind};
pub use eval::{EvalContext, EvalError, Lookup, MAX_TEXT_BYTES, Value};
pub use format::{
    DEFAULT_DECIMALS, format_number, format_number_exact, format_value, text_literal,
    value_to_expression,
};
pub use parser::{ParseError, ParseErrorKind};
pub use table::replace_references;
pub use table::{ChangeSet, ParamId, ParamKind, ParamSpec, Parameter, ParameterTable, TableError};
pub use units::{AngleUnit, DimensionError, Dims, LengthUnit, Quantity, Ratio, Unit};
// The document's tables beside the parameters (mitcad#667).
pub use table::{TableSource, replace_table_references};

// Parameter limits: minimum, maximum, step and strictness (mitcad#665).
mod limits;
pub use limits::{Bounds, LimitError, Limited, ParamLimits};

// Table lookups: `Table.Column`, xlookup, match and index (mitcad#667).
mod lookups;
pub use lookups::{ColumnError, MatchMode, TableCell, TableColumn};

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

/// `text` with the names in `renamed` replaced where they stand as whole
/// names, keeping the rest as it was. For expressions of older files whose
/// parameter names are now reserved (`Max`, now a function in any case):
/// it works on the tokens, as such an expression no longer parses. Text
/// that does not lex is returned as it is.
pub fn rename_names(text: &str, renamed: &HashMap<String, String>) -> String {
    let Ok(tokens) = lexer::lex(text) else {
        return text.to_owned();
    };
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (i, token) in tokens.iter().enumerate() {
        let name = &text[token.span.start..token.span.end];
        // A unit (`5 mm`, an identifier right after a value) is never a
        // parameter, nor is any unit name.
        let unit_place = i > 0
            && matches!(
                tokens[i - 1].tok,
                lexer::Tok::Number(_) | lexer::Tok::Ident | lexer::Tok::RParen
            );
        // Nor are the names of a column reference, `Screws.Max`
        // (mitcad#667).
        let dot = |at: Option<usize>| {
            at.and_then(|at| tokens.get(at))
                .is_some_and(|t| t.tok == lexer::Tok::Dot)
        };
        let column_place = dot(i.checked_sub(1)) || dot(Some(i + 1));
        if token.tok != lexer::Tok::Ident || unit_place || column_place || units::is_unit_name(name)
        {
            continue;
        }
        if let Some(new) = renamed.get(name) {
            out.push_str(&text[copied..token.span.start]);
            out.push_str(new);
            copied = token.span.end;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// The function names of the language before mitcad#663, which were
/// reserved only as written.
const FUNCTION_NAMES_BEFORE_663: [&str; 29] = [
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh",
    "atanh", "sqrt", "abs", "sign", "exp", "ln", "log", "floor", "ceil", "round", "min", "max",
    "pow", "if", "and", "or", "not",
];

/// Whether `name` is reserved only since mitcad#663: a function (or `PI`,
/// or an unsupported name) in another case, or a new function, but not a
/// unit and not a name reserved as written before. A design file may have
/// a parameter of such a name (`Max`, `pi`, `sum`), which is renamed on
/// load; any other reserved name (`mm`, `sin`) was never valid with an
/// expression and is still refused.
pub fn is_newly_reserved(name: &str) -> bool {
    is_reserved_name(name)
        && !units::is_unit_name(name)
        && !FUNCTION_NAMES_BEFORE_663.contains(&name)
        && !["PI", "E"].contains(&name)
        && !UNSUPPORTED_NAMES.contains(&name)
}

/// Names that .f3d expressions reserve but Mitcad does not evaluate.
const UNSUPPORTED_NAMES: [&str; 3] = ["random", "Gravity", "SpeedOfLight"];

/// Whether `name` is one of [`UNSUPPORTED_NAMES`], in any case as function
/// and constant names are.
pub(crate) fn is_unsupported_name(name: &str) -> bool {
    UNSUPPORTED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
}

/// Whether `name` is a unit, function or constant (or one of the
/// unsupported reserved names), and so cannot name a parameter. Function
/// and constant names are reserved in any case (`Max`, `pi`), units only as
/// written (`mm`, not `MM`). The text functions' names (`text`, `left`,
/// `right`, `len`, `concat`; mitcad#664) are not reserved: they are
/// functions only before `(`.
pub fn is_reserved_name(name: &str) -> bool {
    units::is_unit_name(name)
        || Function::from_name(name).is_some_and(Function::is_reserved)
        || Constant::from_name(name).is_some()
        || is_unsupported_name(name)
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
