// SPDX-License-Identifier: MIT
//! Dimensions, units of measure and quantities.
//!
//! Canonical units are the millimetre and the radian: a [`Quantity`]'s
//! value is always in `mm^a · rad^b` for its dimensions `length^a ·
//! angle^b`.

use std::f64::consts::PI;
use std::fmt;
use std::str::FromStr;

use super::Span;
use super::lexer::{Tok, lex};
use super::parser::{ParseError, ParseErrorKind};

/// Rational exponent of a base dimension, in lowest terms with a positive
/// denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio {
    num: i32,
    den: i32,
}

impl Default for Ratio {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Ratio {
    pub const ZERO: Self = Self::integer(0);
    pub const ONE: Self = Self::integer(1);

    pub const fn integer(n: i32) -> Self {
        Self { num: n, den: 1 }
    }

    /// `num / den` in lowest terms, or `None` if `den` is zero or the
    /// result does not fit in `i32`.
    pub fn new(num: i32, den: i32) -> Option<Self> {
        Self::reduce(i128::from(num), i128::from(den))
    }

    fn reduce(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let g = gcd(num.unsigned_abs(), den.unsigned_abs()) as i128;
        let (mut num, mut den) = (num / g, den / g);
        if den < 0 {
            num = -num;
            den = -den;
        }
        Some(Self {
            num: i32::try_from(num).ok()?,
            den: i32::try_from(den).ok()?,
        })
    }

    pub const fn numer(self) -> i32 {
        self.num
    }

    pub const fn denom(self) -> i32 {
        self.den
    }

    pub const fn is_zero(self) -> bool {
        self.num == 0
    }

    pub const fn is_integer(self) -> bool {
        self.den == 1
    }

    pub fn to_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        let (a, b, c, d) = self.parts(other);
        Self::reduce(a * d + c * b, b * d)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        let (a, b, c, d) = self.parts(other);
        Self::reduce(a * d - c * b, b * d)
    }

    pub fn checked_mul(self, other: Self) -> Option<Self> {
        let (a, b, c, d) = self.parts(other);
        Self::reduce(a * c, b * d)
    }

    fn parts(self, other: Self) -> (i128, i128, i128, i128) {
        (
            i128::from(self.num),
            i128::from(self.den),
            i128::from(other.num),
            i128::from(other.den),
        )
    }

    /// The simple fraction (denominator at most 12) equal to `x` within
    /// floating point noise, if there is one.
    pub fn approximate(x: f64) -> Option<Self> {
        if !x.is_finite() {
            return None;
        }
        for den in 1..=12_i32 {
            let scaled = x * f64::from(den);
            let rounded = scaled.round();
            if (scaled - rounded).abs() <= 1e-9 * rounded.abs().max(1.0) {
                if rounded.abs() > f64::from(i32::MAX) {
                    return None;
                }
                return Self::new(rounded as i32, den);
            }
        }
        None
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

/// Physical dimension `length^length · angle^angle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dims {
    pub length: Ratio,
    pub angle: Ratio,
}

impl Dims {
    pub const NONE: Self = Self::new(Ratio::ZERO, Ratio::ZERO);
    pub const LENGTH: Self = Self::new(Ratio::ONE, Ratio::ZERO);
    pub const AREA: Self = Self::new(Ratio::integer(2), Ratio::ZERO);
    pub const VOLUME: Self = Self::new(Ratio::integer(3), Ratio::ZERO);
    pub const ANGLE: Self = Self::new(Ratio::ZERO, Ratio::ONE);

    pub const fn new(length: Ratio, angle: Ratio) -> Self {
        Self { length, angle }
    }

    pub const fn is_dimensionless(self) -> bool {
        self.length.is_zero() && self.angle.is_zero()
    }

    /// Dimensions of a product.
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        Some(Self::new(
            self.length.checked_add(other.length)?,
            self.angle.checked_add(other.angle)?,
        ))
    }

    /// Dimensions of a quotient.
    pub fn checked_div(self, other: Self) -> Option<Self> {
        Some(Self::new(
            self.length.checked_sub(other.length)?,
            self.angle.checked_sub(other.angle)?,
        ))
    }

    /// Dimensions of a power.
    pub fn checked_pow(self, exponent: Ratio) -> Option<Self> {
        Some(Self::new(
            self.length.checked_mul(exponent)?,
            self.angle.checked_mul(exponent)?,
        ))
    }
}

impl fmt::Display for Dims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return f.write_str("unitless");
        }
        let mut first = true;
        for (name, exp) in [("length", self.length), ("angle", self.angle)] {
            if exp.is_zero() {
                continue;
            }
            if !first {
                f.write_str("*")?;
            }
            first = false;
            f.write_str(name)?;
            if exp == Ratio::ONE {
                continue;
            }
            if exp.is_integer() {
                write!(f, "^{exp}")?;
            } else {
                write!(f, "^({exp})")?;
            }
        }
        Ok(())
    }
}

/// Unit of length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    Micrometre,
    Millimetre,
    Centimetre,
    Metre,
    Kilometre,
    Inch,
    Foot,
    Yard,
    Mile,
    /// A thousandth of an inch.
    Mil,
}

impl LengthUnit {
    pub const ALL: [Self; 10] = [
        Self::Micrometre,
        Self::Millimetre,
        Self::Centimetre,
        Self::Metre,
        Self::Kilometre,
        Self::Inch,
        Self::Foot,
        Self::Yard,
        Self::Mile,
        Self::Mil,
    ];

    /// The canonical symbol, as printed.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Micrometre => "um",
            Self::Millimetre => "mm",
            Self::Centimetre => "cm",
            Self::Metre => "m",
            Self::Kilometre => "km",
            Self::Inch => "in",
            Self::Foot => "ft",
            Self::Yard => "yd",
            Self::Mile => "mi",
            Self::Mil => "mil",
        }
    }

    /// Length of one unit in millimetres. The inch is exactly 25.4 mm.
    pub const fn millimetres(self) -> f64 {
        match self {
            Self::Micrometre => 0.001,
            Self::Millimetre => 1.0,
            Self::Centimetre => 10.0,
            Self::Metre => 1000.0,
            Self::Kilometre => 1_000_000.0,
            Self::Inch => 25.4,
            Self::Foot => 304.8,
            Self::Yard => 914.4,
            Self::Mile => 1_609_344.0,
            Self::Mil => 0.0254,
        }
    }
}

impl fmt::Display for LengthUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

/// Unit of plane angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AngleUnit {
    Radian,
    Degree,
    /// A 400th of a full turn.
    Gradian,
}

impl AngleUnit {
    pub const ALL: [Self; 3] = [Self::Radian, Self::Degree, Self::Gradian];

    /// The canonical symbol, as printed.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Radian => "rad",
            Self::Degree => "deg",
            Self::Gradian => "grad",
        }
    }

    /// Size of one unit in radians.
    pub const fn radians(self) -> f64 {
        match self {
            Self::Radian => 1.0,
            Self::Degree => PI / 180.0,
            Self::Gradian => PI / 200.0,
        }
    }
}

impl fmt::Display for AngleUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseUnit {
    Length(LengthUnit),
    Angle(AngleUnit),
}

/// Unit names accepted in expressions and unit strings. The first name of
/// each unit is its canonical symbol.
const UNIT_NAMES: [(&str, BaseUnit); 19] = [
    ("um", BaseUnit::Length(LengthUnit::Micrometre)),
    ("micron", BaseUnit::Length(LengthUnit::Micrometre)),
    ("µm", BaseUnit::Length(LengthUnit::Micrometre)),
    ("μm", BaseUnit::Length(LengthUnit::Micrometre)),
    ("mm", BaseUnit::Length(LengthUnit::Millimetre)),
    ("cm", BaseUnit::Length(LengthUnit::Centimetre)),
    ("m", BaseUnit::Length(LengthUnit::Metre)),
    ("km", BaseUnit::Length(LengthUnit::Kilometre)),
    ("in", BaseUnit::Length(LengthUnit::Inch)),
    ("inch", BaseUnit::Length(LengthUnit::Inch)),
    ("ft", BaseUnit::Length(LengthUnit::Foot)),
    ("foot", BaseUnit::Length(LengthUnit::Foot)),
    ("yd", BaseUnit::Length(LengthUnit::Yard)),
    ("mi", BaseUnit::Length(LengthUnit::Mile)),
    ("mil", BaseUnit::Length(LengthUnit::Mil)),
    ("rad", BaseUnit::Angle(AngleUnit::Radian)),
    ("deg", BaseUnit::Angle(AngleUnit::Degree)),
    ("°", BaseUnit::Angle(AngleUnit::Degree)),
    ("grad", BaseUnit::Angle(AngleUnit::Gradian)),
];

pub(crate) fn base_unit(name: &str) -> Option<BaseUnit> {
    UNIT_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, unit)| unit)
}

pub(crate) fn is_unit_name(name: &str) -> bool {
    base_unit(name).is_some()
}

/// Unit of measure: a power of one length unit times a power of one angle
/// unit, such as `mm`, `in^2`, `deg` or `mm*deg^-1`. The default is
/// unitless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Unit {
    length: Option<(LengthUnit, i32)>,
    angle: Option<(AngleUnit, i32)>,
}

impl Unit {
    pub const NONE: Self = Self {
        length: None,
        angle: None,
    };
    pub const UM: Self = Self::of_length(LengthUnit::Micrometre);
    pub const MM: Self = Self::of_length(LengthUnit::Millimetre);
    pub const CM: Self = Self::of_length(LengthUnit::Centimetre);
    pub const M: Self = Self::of_length(LengthUnit::Metre);
    pub const IN: Self = Self::of_length(LengthUnit::Inch);
    pub const FT: Self = Self::of_length(LengthUnit::Foot);
    pub const DEG: Self = Self::of_angle(AngleUnit::Degree);
    pub const RAD: Self = Self::of_angle(AngleUnit::Radian);

    pub const fn of_length(unit: LengthUnit) -> Self {
        Self {
            length: Some((unit, 1)),
            angle: None,
        }
    }

    pub const fn of_angle(unit: AngleUnit) -> Self {
        Self {
            length: None,
            angle: Some((unit, 1)),
        }
    }

    pub(crate) const fn of_base(unit: BaseUnit) -> Self {
        match unit {
            BaseUnit::Length(u) => Self::of_length(u),
            BaseUnit::Angle(u) => Self::of_angle(u),
        }
    }

    /// The length unit and its power, if any.
    pub const fn length(self) -> Option<(LengthUnit, i32)> {
        self.length
    }

    /// The angle unit and its power, if any.
    pub const fn angle(self) -> Option<(AngleUnit, i32)> {
        self.angle
    }

    pub const fn is_unitless(self) -> bool {
        self.length.is_none() && self.angle.is_none()
    }

    /// This unit raised to an integer power, or `None` on overflow.
    pub fn powi(self, n: i32) -> Option<Self> {
        fn pow<U>(part: Option<(U, i32)>, n: i32) -> Option<Option<(U, i32)>> {
            match part {
                None => Some(None),
                Some((unit, p)) => {
                    let p = p.checked_mul(n)?;
                    Some((p != 0).then_some((unit, p)))
                }
            }
        }
        Some(Self {
            length: pow(self.length, n)?,
            angle: pow(self.angle, n)?,
        })
    }

    /// The product of two units, or `None` if they use two different
    /// length (or angle) units, such as `mm*in`, or on overflow.
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        fn mul<U: PartialEq>(a: Option<(U, i32)>, b: Option<(U, i32)>) -> Option<Option<(U, i32)>> {
            match (a, b) {
                (None, x) | (x, None) => Some(x),
                (Some((ua, pa)), Some((ub, pb))) => {
                    if ua != ub {
                        return None;
                    }
                    let p = pa.checked_add(pb)?;
                    Some((p != 0).then_some((ua, p)))
                }
            }
        }
        Some(Self {
            length: mul(self.length, other.length)?,
            angle: mul(self.angle, other.angle)?,
        })
    }

    pub fn dims(self) -> Dims {
        let exp = |part: Option<i32>| Ratio::integer(part.unwrap_or(0));
        Dims::new(
            exp(self.length.map(|(_, p)| p)),
            exp(self.angle.map(|(_, p)| p)),
        )
    }

    /// Size of one of this unit in canonical units (`mm^a · rad^b`).
    pub fn factor(self) -> f64 {
        let length = self.length.map_or(1.0, |(u, p)| u.millimetres().powi(p));
        let angle = self.angle.map_or(1.0, |(u, p)| u.radians().powi(p));
        length * angle
    }

    /// Converts a value in this unit to canonical units.
    pub fn to_canonical(self, value: f64) -> f64 {
        value * self.factor()
    }

    /// Converts a value in canonical units to this unit.
    pub fn canonical_to_unit(self, canonical: f64) -> f64 {
        canonical / self.factor()
    }

    /// The unit used to display `dims`: millimetres and degrees. `None`
    /// for fractional exponents, which no unit can express.
    pub fn display_for(dims: Dims) -> Option<Self> {
        if !dims.length.is_integer() || !dims.angle.is_integer() {
            return None;
        }
        let part = |p: i32| (p != 0).then_some(p);
        Some(Self {
            length: part(dims.length.numer()).map(|p| (LengthUnit::Millimetre, p)),
            angle: part(dims.angle.numer()).map(|p| (AngleUnit::Degree, p)),
        })
    }

    /// Parses a unit string: empty for unitless, or unit names with
    /// optional integer powers joined by `*` and `/`, such as `mm`,
    /// `in^2`, `deg` or `mm/deg`.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let tokens = lex(text)?;
        let mut unit = Self::NONE;
        let mut i = 0;
        let mut sign = 1;
        while i < tokens.len() {
            let token = tokens[i];
            let name = &text[token.span.start..token.span.end];
            let base = match token.tok {
                Tok::Ident => base_unit(name).ok_or_else(|| {
                    ParseError::new(ParseErrorKind::UnknownUnit(name.to_owned()), token.span)
                })?,
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::Unexpected {
                            found: name.to_owned(),
                            expected: "a unit",
                        },
                        token.span,
                    ));
                }
            };
            i += 1;
            let mut power = 1;
            let mut end = token.span.end;
            if let Some((p, used, span)) = unit_power(text, &tokens[i..])? {
                power = p;
                i += used;
                end = span.end;
            }
            let factor = Self::of_base(base)
                .powi(power * sign)
                .ok_or_else(|| ParseError::new(ParseErrorKind::InvalidUnitPower, token.span))?;
            unit = unit.checked_mul(factor).ok_or_else(|| {
                ParseError::new(
                    ParseErrorKind::IncompatibleUnits(name.to_owned()),
                    Span::new(token.span.start, end),
                )
            })?;
            match tokens.get(i).map(|t| t.tok) {
                None => {}
                Some(Tok::Star) => sign = 1,
                Some(Tok::Slash) => sign = -1,
                Some(_) => {
                    let t = tokens[i];
                    return Err(ParseError::new(
                        ParseErrorKind::Unexpected {
                            found: text[t.span.start..t.span.end].to_owned(),
                            expected: "'*', '/' or the end of the unit",
                        },
                        t.span,
                    ));
                }
            }
            if i < tokens.len() {
                i += 1;
                if i == tokens.len() {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedEnd { expected: "a unit" },
                        Span::new(text.len(), text.len()),
                    ));
                }
            }
        }
        Ok(unit)
    }
}

/// An integer power `^ [+|-] digits` at the start of `tokens`: the power,
/// the number of tokens used and the span of the power. `None` if the
/// tokens do not start with one.
pub(crate) fn unit_power(
    text: &str,
    tokens: &[super::lexer::Token],
) -> Result<Option<(i32, usize, Span)>, ParseError> {
    if tokens.first().map(|t| t.tok) != Some(Tok::Caret) {
        return Ok(None);
    }
    let (negative, digits_at) = match tokens.get(1).map(|t| t.tok) {
        Some(Tok::Minus) => (true, 2),
        Some(Tok::Plus) => (false, 2),
        _ => (false, 1),
    };
    let Some(digits) = tokens.get(digits_at) else {
        return Ok(None);
    };
    let digits_text = &text[digits.span.start..digits.span.end];
    if !matches!(digits.tok, Tok::Number(_)) || !digits_text.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let span = Span::new(tokens[0].span.start, digits.span.end);
    let power = digits_text
        .parse::<i32>()
        .ok()
        .filter(|&p| p != 0)
        .ok_or_else(|| ParseError::new(ParseErrorKind::InvalidUnitPower, span))?;
    Ok(Some((
        if negative { -power } else { power },
        digits_at + 1,
        span,
    )))
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let parts = [
            self.length.map(|(u, p)| (u.symbol(), p)),
            self.angle.map(|(u, p)| (u.symbol(), p)),
        ];
        for (symbol, power) in parts.into_iter().flatten() {
            if !first {
                f.write_str("*")?;
            }
            first = false;
            f.write_str(symbol)?;
            if power != 1 {
                write!(f, "^{power}")?;
            }
        }
        Ok(())
    }
}

impl FromStr for Unit {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<LengthUnit> for Unit {
    fn from(unit: LengthUnit) -> Self {
        Self::of_length(unit)
    }
}

impl From<AngleUnit> for Unit {
    fn from(unit: AngleUnit) -> Self {
        Self::of_angle(unit)
    }
}

/// A value in canonical units (millimetres and radians) with its
/// dimensions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub dims: Dims,
}

impl Quantity {
    pub const fn new(value: f64, dims: Dims) -> Self {
        Self { value, dims }
    }

    pub const fn unitless(value: f64) -> Self {
        Self::new(value, Dims::NONE)
    }

    /// A length in millimetres.
    pub const fn length(mm: f64) -> Self {
        Self::new(mm, Dims::LENGTH)
    }

    /// An angle in radians.
    pub const fn angle(rad: f64) -> Self {
        Self::new(rad, Dims::ANGLE)
    }

    /// `value` given in `unit`.
    pub fn from_unit(value: f64, unit: Unit) -> Self {
        Self::new(unit.to_canonical(value), unit.dims())
    }

    pub const fn is_dimensionless(self) -> bool {
        self.dims.is_dimensionless()
    }

    /// The value expressed in `unit`.
    pub fn in_unit(self, unit: Unit) -> Result<f64, DimensionError> {
        if self.dims == unit.dims() {
            Ok(unit.canonical_to_unit(self.value))
        } else {
            Err(DimensionError {
                expected: unit.dims(),
                found: self.dims,
            })
        }
    }
}

impl fmt::Display for Quantity {
    /// Millimetres and degrees with at most six decimals, such as
    /// `12.5 mm` or `30 deg`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match Unit::display_for(self.dims) {
            Some(unit) => f.write_str(&super::format_value(
                self.value,
                unit,
                super::DEFAULT_DECIMALS,
            )),
            None => write!(
                f,
                "{} ({})",
                super::format_number(self.value, super::DEFAULT_DECIMALS),
                self.dims
            ),
        }
    }
}

/// A value has other dimensions than required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionError {
    pub expected: Dims,
    pub found: Dims,
}

impl fmt::Display for DimensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}, found {}", self.expected, self.found)
    }
}

impl std::error::Error for DimensionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_arithmetic_reduces() {
        let half = Ratio::new(2, 4).unwrap();
        assert_eq!((half.numer(), half.denom()), (1, 2));
        assert_eq!(Ratio::new(3, -6), Ratio::new(-1, 2));
        assert_eq!(half.checked_add(half), Some(Ratio::ONE));
        assert_eq!(Ratio::ONE.checked_sub(half), Some(half));
        assert_eq!(half.checked_mul(Ratio::integer(4)), Some(Ratio::integer(2)));
        assert_eq!(Ratio::new(1, 0), None);
        assert_eq!(Ratio::integer(i32::MAX).checked_add(Ratio::ONE), None);
        assert_eq!(Ratio::approximate(0.5), Ratio::new(1, 2));
        assert_eq!(Ratio::approximate(1.0 / 3.0), Ratio::new(1, 3));
        assert_eq!(Ratio::approximate(-2.0), Some(Ratio::integer(-2)));
        assert_eq!(Ratio::approximate(std::f64::consts::PI), None);
        assert_eq!(Ratio::new(-3, 2).unwrap().to_string(), "-3/2");
    }

    #[test]
    fn dims_display() {
        assert_eq!(Dims::NONE.to_string(), "unitless");
        assert_eq!(Dims::LENGTH.to_string(), "length");
        assert_eq!(Dims::AREA.to_string(), "length^2");
        assert_eq!(
            Dims::new(Ratio::new(1, 2).unwrap(), Ratio::integer(-1)).to_string(),
            "length^(1/2)*angle^-1"
        );
    }

    #[test]
    fn unit_factors_are_exact_definitions() {
        assert_eq!(Unit::IN.factor(), 25.4);
        assert_eq!(Unit::FT.factor(), 304.8);
        assert_eq!(Unit::CM.factor(), 10.0);
        assert_eq!(Unit::of_length(LengthUnit::Mil).factor(), 0.0254);
        assert_eq!(Unit::DEG.factor(), PI / 180.0);
        assert_eq!(Unit::CM.powi(3).unwrap().factor(), 1000.0);
        assert_eq!(Unit::IN.canonical_to_unit(25.4), 1.0);
        assert_eq!(Unit::FT.canonical_to_unit(304.8), 1.0);
        // Degrees are not exact in binary; the round trip is within an ulp.
        let back = Unit::DEG.canonical_to_unit(Unit::DEG.to_canonical(30.0));
        assert!((back - 30.0).abs() <= 4.0 * f64::EPSILON * 30.0);
    }

    #[test]
    fn unit_parse_and_display() {
        assert_eq!(Unit::parse(""), Ok(Unit::NONE));
        assert_eq!(Unit::parse(" mm "), Ok(Unit::MM));
        assert_eq!(Unit::parse("inch"), Ok(Unit::IN));
        assert_eq!(Unit::parse("°"), Ok(Unit::DEG));
        assert_eq!(Unit::parse("µm"), Ok(Unit::UM));
        let area = Unit::parse("mm^2").unwrap();
        assert_eq!(area.dims(), Dims::AREA);
        assert_eq!(area.to_string(), "mm^2");
        let per = Unit::parse("mm/deg").unwrap();
        assert_eq!(per.to_string(), "mm*deg^-1");
        assert_eq!(Unit::parse(&per.to_string()), Ok(per));
        assert_eq!(Unit::parse("mm*mm"), Ok(area));
        assert_eq!(Unit::parse("mm/mm"), Ok(Unit::NONE));
        assert_eq!(Unit::parse("cm^-1").unwrap().to_string(), "cm^-1");
        assert!(matches!(
            Unit::parse("mm*in").unwrap_err().kind,
            ParseErrorKind::IncompatibleUnits(_)
        ));
        assert_eq!(
            Unit::parse("xx").unwrap_err().kind,
            ParseErrorKind::UnknownUnit("xx".into())
        );
        assert!(Unit::parse("mm*").is_err());
        assert!(Unit::parse("mm^0").is_err());
        assert!(Unit::parse("2").is_err());
        assert_eq!("deg".parse::<Unit>(), Ok(Unit::DEG));
    }

    #[test]
    fn quantity_conversion_and_display() {
        let q = Quantity::from_unit(1.0, Unit::IN);
        assert_eq!(q.value, 25.4);
        assert_eq!(q.in_unit(Unit::IN), Ok(1.0));
        assert_eq!(
            q.in_unit(Unit::DEG),
            Err(DimensionError {
                expected: Dims::ANGLE,
                found: Dims::LENGTH
            })
        );
        assert_eq!(Quantity::length(12.5).to_string(), "12.5 mm");
        assert_eq!(Quantity::from_unit(30.0, Unit::DEG).to_string(), "30 deg");
        assert_eq!(Quantity::new(100.0, Dims::AREA).to_string(), "100 mm^2");
        assert_eq!(Quantity::unitless(0.5).to_string(), "0.5");
        let root = Quantity::new(2.0, Dims::new(Ratio::new(1, 2).unwrap(), Ratio::ZERO));
        assert_eq!(root.to_string(), "2 (length^(1/2))");
    }
}
