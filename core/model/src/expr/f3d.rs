// SPDX-License-Identifier: MIT
//! Parameter data of `.f3d` designs.
//!
//! An `.f3d` file stores expressions as text, such as
//! `( 13 / 3 ) * 1 mm`, `6 mm`, `0.0 deg` or `360.0 deg` (so do the
//! dumps of `core/import/SCHEMA.md`); they use the grammar of this
//! module. The files hold values in internal units: centimetres for
//! length and radians for angles (Mitcad uses millimetres and radians).

use super::ast::Expr;
use super::eval::{EvalContext, EvalError, Lookup};
use super::parser::ParseError;
use super::units::{AngleUnit, Dims, LengthUnit, Quantity, Ratio, Unit};

/// The internal length unit of `.f3d` files.
pub const INTERNAL_LENGTH_UNIT: LengthUnit = LengthUnit::Centimetre;
/// The internal angle unit of `.f3d` files.
pub const INTERNAL_ANGLE_UNIT: AngleUnit = AngleUnit::Radian;

/// Parses an expression as an `.f3d` file stores it. Surrounding white
/// space, including non-breaking spaces, is ignored.
pub fn parse_expression(text: &str) -> Result<Expr, ParseError> {
    Expr::parse(text)
}

/// Parses a unit string of an `.f3d` file: `""` for unitless, `mm`,
/// `in`, `deg`, `cm^3` and so on.
pub fn parse_unit(text: &str) -> Result<Unit, ParseError> {
    Unit::parse(text)
}

/// The evaluation context of an `.f3d` design whose default length unit
/// is `length`. Its angle unit for unitless numbers is the degree.
pub fn context(length: LengthUnit) -> EvalContext {
    EvalContext::new(length, AngleUnit::Degree)
}

/// Converts a value in the internal units of `.f3d` files (cm, rad) with
/// dimensions `dims` to canonical units (mm, rad).
pub fn internal_to_canonical(value: f64, dims: Dims) -> f64 {
    value * length_scale(dims.length)
}

/// Converts a canonical value (mm, rad) to the internal units of `.f3d`
/// files (cm, rad).
pub fn canonical_to_internal(value: f64, dims: Dims) -> f64 {
    value / length_scale(dims.length)
}

/// An internal value of an `.f3d` file as a quantity.
pub fn internal_quantity(value: f64, dims: Dims) -> Quantity {
    Quantity::new(internal_to_canonical(value, dims), dims)
}

/// Millimetres per centimetre raised to the length exponent.
fn length_scale(exp: Ratio) -> f64 {
    let mm_per_cm = INTERNAL_LENGTH_UNIT.millimetres();
    if exp.is_integer() {
        mm_per_cm.powi(exp.numer())
    } else {
        mm_per_cm.powf(exp.to_f64())
    }
}

/// Evaluates a parameter of an `.f3d` file (expression and unit strings)
/// and checks it against the value stored in the file, in internal units.
/// Returns the evaluated quantity and whether it agrees within
/// `relative_tolerance`.
pub fn check_parameter(
    expression: &str,
    unit: &str,
    internal_value: f64,
    lookup: &dyn Lookup,
    context: &EvalContext,
    relative_tolerance: f64,
) -> Result<(Quantity, bool), CheckError> {
    let unit = parse_unit(unit).map_err(CheckError::Unit)?;
    let expr = parse_expression(expression).map_err(CheckError::Expression)?;
    let q = expr
        .eval_as(lookup, context, unit)
        .map_err(CheckError::Eval)?;
    let expected = internal_to_canonical(internal_value, q.dims);
    let agrees = (q.value - expected).abs() <= relative_tolerance * expected.abs().max(1e-9);
    Ok((q, agrees))
}

/// Error from [`check_parameter`].
#[derive(Debug, Clone, PartialEq)]
pub enum CheckError {
    Unit(ParseError),
    Expression(ParseError),
    Eval(EvalError),
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unit(e) => write!(f, "unit: {e}"),
            Self::Expression(e) => write!(f, "expression: {e}"),
            Self::Eval(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for CheckError {}
