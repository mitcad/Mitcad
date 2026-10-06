// SPDX-License-Identifier: MIT
//! Evaluation of expressions to quantities, with the unit rules of the
//! module documentation.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;

use super::Span;
use super::ast::{BinaryOp, Function, Node, NodeKind};
use super::units::{AngleUnit, Dims, LengthUnit, Quantity, Ratio, Unit};

/// Source of parameter values for evaluation.
pub trait Lookup {
    /// The value of the parameter `name`, or `None` if there is none.
    fn lookup(&self, name: &str) -> Option<Quantity>;
}

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
            | Self::NotFinite { span } => Some(*span),
            Self::WrongDimension { .. } => None,
        }
    }

    /// Errors about a value rather than the expression's structure.
    fn is_value_error(&self) -> bool {
        matches!(
            self,
            Self::DivisionByZero { .. } | Self::Domain { .. } | Self::NotFinite { .. }
        )
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownReference { name, .. } => write!(f, "unknown parameter '{name}'")?,
            Self::DimensionMismatch {
                operation,
                left,
                right,
                ..
            } => write!(f, "cannot apply '{operation}' to {left} and {right}")?,
            Self::NotDimensionless {
                operation, found, ..
            } => write!(f, "'{operation}' needs a unitless value, found {found}")?,
            Self::NotAngle {
                function, found, ..
            } => write!(f, "'{function}' needs an angle, found {found}")?,
            Self::InvalidExponent { exponent, .. } => write!(
                f,
                "a value with units can only be raised to a simple fraction, not {exponent}"
            )?,
            Self::DimensionOverflow { .. } => f.write_str("unit exponent out of range")?,
            Self::DivisionByZero { .. } => f.write_str("division by zero")?,
            Self::Domain { operation, .. } => write!(f, "argument out of range for '{operation}'")?,
            Self::NotFinite { .. } => f.write_str("result is not a finite number")?,
            Self::WrongDimension { expected, found } => {
                write!(f, "expected {expected}, found {found}")?;
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
    }
    .eval(root)
}

pub(crate) fn eval_as(
    root: &Node,
    lookup: &dyn Lookup,
    context: &EvalContext,
    unit: Unit,
) -> Result<Quantity, EvalError> {
    let q = eval(root, lookup, &context.for_unit(unit))?;
    let dims = unit.dims();
    if q.dims == dims {
        Ok(q)
    } else if q.is_dimensionless() {
        finite(Quantity::new(unit.to_canonical(q.value), dims), root.span)
    } else {
        Err(EvalError::WrongDimension {
            expected: dims,
            found: q.dims,
        })
    }
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

fn compare(op: BinaryOp, a: f64, b: f64) -> bool {
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

struct Evaluator<'a> {
    lookup: &'a dyn Lookup,
    context: EvalContext,
}

impl Evaluator<'_> {
    fn eval(&self, node: &Node) -> Result<Quantity, EvalError> {
        let span = node.span;
        let q = match &node.kind {
            NodeKind::Number(v) => Quantity::unitless(*v),
            NodeKind::Constant(c) => Quantity::unitless(c.value()),
            NodeKind::Reference(name) => {
                self.lookup
                    .lookup(name)
                    .ok_or_else(|| EvalError::UnknownReference {
                        name: name.clone(),
                        span,
                    })?
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
        };
        finite(q, span)
    }

    /// Reads a dimensionless value in the default unit of `dims`.
    fn promote(&self, q: Quantity, dims: Dims) -> Quantity {
        Quantity::new(q.value * self.context.default_factor(dims), dims)
    }

    /// Gives two operands the same dimensions, promoting a dimensionless
    /// one.
    fn unify(
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
        }
    }

    fn call(&self, function: Function, args: &[Node], span: Span) -> Result<Quantity, EvalError> {
        if function == Function::If {
            return self.if_else(args, span);
        }
        let mut values = args
            .iter()
            .map(|a| self.eval(a))
            .collect::<Result<Vec<_>, _>>()?;
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
            Function::Floor | Function::Ceil | Function::Round => {
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
                    .expect("arity is at least two")
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
            Function::If => unreachable!("handled above"),
        };
        Ok(q)
    }

    /// `if(c; a; b)`: the branch not taken is evaluated too, but only
    /// structural errors in it (unknown names, dimensions) count.
    fn if_else(&self, args: &[Node], span: Span) -> Result<Quantity, EvalError> {
        let c = self.eval(&args[0])?;
        let condition = dimensionless("if", c, args[0].span)? != 0.0;
        let (taken, other) = if condition {
            (&args[1], &args[2])
        } else {
            (&args[2], &args[1])
        };
        let value = self.eval(taken)?;
        match self.eval(other) {
            Ok(o) => Ok(self.unify("if", value, o, span)?.0),
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

fn dimensionless(operation: &'static str, q: Quantity, span: Span) -> Result<f64, EvalError> {
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
