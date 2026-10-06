// SPDX-License-Identifier: MIT
//! Formatting of values and canonical printing of expressions.

use std::fmt::{self, Write as _};

use super::ast::{BinaryOp, Expr, Node, NodeKind, PREC_POW, PREC_PRIMARY, PREC_UNARY};
use super::units::Unit;

/// Decimals shown when no precision is given.
pub const DEFAULT_DECIMALS: usize = 6;

/// `value` rounded to at most `max_decimals` decimals, without trailing
/// zeros: `12.5`, `30`, `0.333333`.
pub fn format_number(value: f64, max_decimals: usize) -> String {
    let mut text = format!("{value:.max_decimals$}");
    if text.contains('.') {
        let trimmed = text.trim_end_matches('0').trim_end_matches('.').len();
        text.truncate(trimmed);
    }
    if text == "-0" {
        text.remove(0);
    }
    text
}

/// The shortest text that parses back to exactly `value`. Magnitudes from
/// 1e16 up and below 1e-6 use an exponent (`1e-12`).
pub fn format_number_exact(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs();
    if (1e-6..1e16).contains(&magnitude) {
        value.to_string()
    } else {
        format!("{value:e}")
    }
}

/// A canonical value as text in `unit`, such as `12.5 mm` or `30 deg`;
/// unitless values have no suffix. The caller makes sure the dimensions
/// match (see [`super::Quantity::in_unit`]).
pub fn format_value(canonical: f64, unit: Unit, max_decimals: usize) -> String {
    with_unit(
        format_number(unit.canonical_to_unit(canonical), max_decimals),
        unit,
    )
}

/// An expression text for a canonical value in `unit` that evaluates back
/// to the same value, such as `12.5 mm`. The number is the shortest
/// decimal that converts to exactly `canonical`, so a 30 degree angle is
/// `30 deg` even though converting it back gives 29.999999999999996.
/// `canonical` must be finite.
pub fn value_to_expression(canonical: f64, unit: Unit) -> String {
    let value = unit.canonical_to_unit(canonical);
    let shortest = (0..17)
        .filter_map(|digits| format!("{value:.digits$e}").parse::<f64>().ok())
        .find(|&candidate| unit.to_canonical(candidate) == canonical)
        .unwrap_or(value);
    with_unit(format_number_exact(shortest), unit)
}

fn with_unit(mut number: String, unit: Unit) -> String {
    if !unit.is_unitless() {
        write!(number, " {unit}").expect("writing to a string");
    }
    number
}

/// Canonical printing: single spaces around binary operators, the fewest
/// parentheses that keep the structure, `;` between arguments. Parsing
/// the result gives an equal expression.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_node(f, self.root())
    }
}

fn write_node(f: &mut fmt::Formatter<'_>, node: &Node) -> fmt::Result {
    match &node.kind {
        NodeKind::Number(v) => f.write_str(&format_number_exact(*v)),
        NodeKind::Constant(c) => f.write_str(c.name()),
        NodeKind::Reference(name) => f.write_str(name),
        NodeKind::Neg(x) => {
            f.write_str("-")?;
            let parens = x.precedence() < PREC_UNARY || matches!(x.kind, NodeKind::Neg(_));
            write_operand(f, x, parens)
        }
        NodeKind::WithUnit { value, unit } => {
            write_operand(f, value, value.precedence() < PREC_PRIMARY)?;
            write!(f, " {unit}")
        }
        NodeKind::Binary { op, lhs, rhs } => {
            let prec = op.precedence();
            let (lhs_parens, rhs_parens) = if *op == BinaryOp::Pow {
                // Right-associative; the exponent is parsed as a unary
                // expression, and `10 mm ^ 2` would read as a unit power.
                (
                    lhs.precedence() <= PREC_POW || matches!(lhs.kind, NodeKind::WithUnit { .. }),
                    rhs.precedence() < PREC_UNARY,
                )
            } else {
                (lhs.precedence() < prec, rhs.precedence() <= prec)
            };
            write_operand(f, lhs, lhs_parens)?;
            write!(f, " {} ", op.symbol())?;
            write_operand(f, rhs, rhs_parens)
        }
        NodeKind::Call { function, args } => {
            write!(f, "{}(", function.name())?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.write_str("; ")?;
                }
                write_node(f, arg)?;
            }
            f.write_str(")")
        }
    }
}

fn write_operand(f: &mut fmt::Formatter<'_>, node: &Node, parens: bool) -> fmt::Result {
    if parens {
        f.write_str("(")?;
        write_node(f, node)?;
        f.write_str(")")
    } else {
        write_node(f, node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(format_number(12.5, 3), "12.5");
        assert_eq!(format_number(30.0, 2), "30");
        assert_eq!(format_number(29.999_999_999_999_996, 6), "30");
        assert_eq!(format_number(1.0 / 3.0, 6), "0.333333");
        assert_eq!(format_number(-0.000_000_1, 3), "0");
        assert_eq!(format_number(-2.25, 1), "-2.2");
        assert_eq!(format_number(1234.0, 0), "1234");
        assert_eq!(format_number_exact(0.1), "0.1");
        assert_eq!(format_number_exact(-0.0), "0");
        assert_eq!(format_number_exact(1e-12), "1e-12");
        assert_eq!(format_number_exact(1.5e300), "1.5e300");
        assert_eq!(format_number_exact(123_456.789), "123456.789");
        assert_eq!(format_number_exact(1e6), "1000000");
        for v in [0.1, 1.0 / 3.0, 2.0f64.sqrt(), 1e-300, 6.02e23, 25.4, 0.0254] {
            assert_eq!(format_number_exact(v).parse::<f64>(), Ok(v));
        }
    }

    #[test]
    fn values_in_units() {
        assert_eq!(format_value(12.5, Unit::MM, 3), "12.5 mm");
        assert_eq!(format_value(25.4, Unit::IN, 3), "1 in");
        assert_eq!(
            format_value(Unit::DEG.to_canonical(30.0), Unit::DEG, 2),
            "30 deg"
        );
        assert_eq!(format_value(0.5, Unit::NONE, 3), "0.5");
        assert_eq!(format_value(100.0, Unit::CM.powi(2).unwrap(), 3), "1 cm^2");
        assert_eq!(
            value_to_expression(4.0 / 3.0, Unit::MM),
            "1.3333333333333333 mm"
        );
        assert_eq!(
            value_to_expression(Unit::DEG.to_canonical(30.0), Unit::DEG),
            "30 deg"
        );
        assert_eq!(value_to_expression(25.4, Unit::IN), "1 in");
        assert_eq!(value_to_expression(0.0, Unit::DEG), "0 deg");
        for (v, unit) in [(0.1, Unit::IN), (1.0 / 7.0, Unit::DEG), (123.456, Unit::CM)] {
            let canonical = unit.to_canonical(v);
            let text = value_to_expression(canonical, unit);
            let number: f64 = text.split(' ').next().unwrap().parse().unwrap();
            assert_eq!(unit.to_canonical(number), canonical, "{text}");
        }
    }
}
