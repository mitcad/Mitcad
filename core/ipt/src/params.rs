// SPDX-License-Identifier: MIT
//! Parameters of the definitions segment: each a record with its name, its
//! unit, its expression as a tree of records and its value (cm, rad).
//!
//! - Parameter (`264d8790…`): the header, two u32, the name, a u32, the
//!   unit and the expression (references), the nominal value and the model
//!   value (f64, cm or rad) and two u16.
//! - Unit (`fd79a7f8…`): u32, u16, the numerator and denominator lists of
//!   base units, a byte and a reference. A base unit (u32, u16, f64
//!   magnitude, f64 factor) is a length or angle in internal units (a
//!   parameter's unit), a display unit (millimetre: metre with magnitude
//!   0.001, inch, degree) or unitless.
//! - Expression nodes start with u32, u16 and their display unit: a
//!   number (`047aa7f8…`: f64 in internal units, u16, u32), a parameter
//!   (`057aa7f8…`: its record), the operators `+ - * / % ^` (`067aa7f8…`
//!   to `0b7aa7f8…`: two operands), negation (`0c7aa7f8…`: one) and
//!   parentheses (`0d7aa7f8…`: one).
//!
//! An expression is translated into Mitcad's expression language and
//! evaluated here against the value the file stores. A parameter with the
//! flag [`COMPUTED`] has a value the model computes (a thread's minor
//! radius from its table, for one): its expression need not give it, and
//! the parameters that name it use its value.

use std::collections::HashMap;

use crate::dc::{self, Definitions, Reader, reference, type_id};

pub const PARAMETER: [u8; 16] = type_id("264d8790d011f8d10008cabc0663dc09");
/// An integer parameter (a pattern's count): the header, the prefix, the
/// name, a u32 and the nominal and model values (u32).
pub const INTEGER: [u8; 16] = type_id("dfd51dbbd1116e72000817bd0663dc09");
pub const UNIT: [u8; 16] = type_id("fd79a7f8d2118f09c0005a9a2378d04f");
pub const NUMBER: [u8; 16] = type_id("047aa7f8d2118f09c0005a9a2378d04f");
pub const PARAMETER_REF: [u8; 16] = type_id("057aa7f8d2118f09c0005a9a2378d04f");
pub const NEGATION: [u8; 16] = type_id("0c7aa7f8d2118f09c0005a9a2378d04f");
/// The binary operators, `067aa7f8…` to `0b7aa7f8…`.
const OPERATORS: [(u8, Op); 6] = [
    (0x06, Op::Add),
    (0x07, Op::Sub),
    (0x08, Op::Mul),
    (0x09, Op::Div),
    (0x0A, Op::Rem),
    (0x0B, Op::Pow),
];
/// The header flag of a parameter whose value the model computes, not its
/// expression (see the module docs).
pub const COMPUTED: u32 = 0x0100_0000;
/// The last 15 bytes of the expression node types.
const NODE_SUFFIX: [u8; 15] = [
    0x7A, 0xA7, 0xF8, 0xD2, 0x11, 0x8F, 0x09, 0xC0, 0x00, 0x5A, 0x9A, 0x23, 0x78, 0xD0, 0x4F,
];

/// Base units.
const LENGTH: [u8; 16] = type_id("bc204162d2119b0b60006ab760fec3b0");
const METRE: [u8; 16] = type_id("f579a7f8d2118f09c0005a9a2378d04f");
const INCH: [u8; 16] = type_id("f679a7f8d2118f09c0005a9a2378d04f");
const FOOT: [u8; 16] = type_id("f779a7f8d2118f09c0005a9a2378d04f");
const ANGLE: [u8; 16] = type_id("f0cd305cd2113f0d60006ab760fec3b0");
const RADIAN: [u8; 16] = type_id("f2cd305cd2113f0d60006ab760fec3b0");
const DEGREE: [u8; 16] = type_id("f6cd305cd2113f0d60006ab760fec3b0");
const UNITLESS: [u8; 16] = type_id("23009d5fd2118e09c0005a9a2378d04f");
const COUNT: [u8; 16] = type_id("22009d5fd2118e09c0005a9a2378d04f");

/// What a parameter measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    Length,
    Angle,
    Unitless,
}

/// A unit a number is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayUnit {
    Um,
    Mm,
    Cm,
    M,
    In,
    Ft,
    Deg,
    Rad,
    Unitless,
}

impl DisplayUnit {
    /// Internal units (cm, rad) per one of this unit.
    pub fn scale(self) -> f64 {
        match self {
            DisplayUnit::Um => 1e-4,
            DisplayUnit::Mm => 0.1,
            DisplayUnit::Cm => 1.0,
            DisplayUnit::M => 100.0,
            DisplayUnit::In => 2.54,
            DisplayUnit::Ft => 30.48,
            DisplayUnit::Deg => std::f64::consts::PI / 180.0,
            DisplayUnit::Rad | DisplayUnit::Unitless => 1.0,
        }
    }

    /// Mitcad's symbol ("" when unitless).
    pub fn symbol(self) -> &'static str {
        match self {
            DisplayUnit::Um => "um",
            DisplayUnit::Mm => "mm",
            DisplayUnit::Cm => "cm",
            DisplayUnit::M => "m",
            DisplayUnit::In => "in",
            DisplayUnit::Ft => "ft",
            DisplayUnit::Deg => "deg",
            DisplayUnit::Rad => "rad",
            DisplayUnit::Unitless => "",
        }
    }

    /// The length unit of a Mitcad symbol.
    pub fn length(symbol: &str) -> Option<DisplayUnit> {
        [
            DisplayUnit::Um,
            DisplayUnit::Mm,
            DisplayUnit::Cm,
            DisplayUnit::M,
            DisplayUnit::In,
            DisplayUnit::Ft,
        ]
        .into_iter()
        .find(|u| u.symbol() == symbol)
    }

    pub fn quantity(self) -> Quantity {
        match self {
            DisplayUnit::Deg | DisplayUnit::Rad => Quantity::Angle,
            DisplayUnit::Unitless => Quantity::Unitless,
            _ => Quantity::Length,
        }
    }
}

/// A unit record: the quantity of a parameter's unit, or the display unit
/// of a number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    /// A length or angle in internal units, or unitless.
    Internal(Quantity),
    Display(DisplayUnit),
}

impl Unit {
    pub fn quantity(self) -> Quantity {
        match self {
            Unit::Internal(q) => q,
            Unit::Display(d) => d.quantity(),
        }
    }
}

/// Reads a unit record.
pub fn unit(dc: &Definitions, record: usize) -> Result<Unit, String> {
    if !dc.is(record, &UNIT) {
        return Err(format!("record {record} is not a unit"));
    }
    let mut r = dc.reader(record);
    let read = |r: &mut Reader| -> dc::Result<(Vec<usize>, Vec<usize>)> {
        r.skip(6)?;
        let numerator = r.references()?;
        let denominator = r.references()?;
        Ok((numerator, denominator))
    };
    let (numerator, denominator) = read(&mut r).map_err(|e| format!("unit {record}: {e}"))?;
    if !denominator.is_empty() {
        return Err(format!("unit {record}: a quotient of units"));
    }
    let mut found = None;
    for base in numerator {
        let Some(t) = dc.type_of(base) else {
            return Err(format!("unit {record}: no record {base}"));
        };
        let mut r = dc.reader(base);
        let magnitude = r
            .skip(6)
            .and_then(|_| r.f64())
            .map_err(|e| format!("base unit {base}: {e}"))?;
        let unit = match *t {
            LENGTH => Unit::Internal(Quantity::Length),
            ANGLE => Unit::Internal(Quantity::Angle),
            UNITLESS | COUNT => continue,
            METRE => Unit::Display(match magnitude {
                1e-6 => DisplayUnit::Um,
                1e-3 => DisplayUnit::Mm,
                1e-2 => DisplayUnit::Cm,
                1.0 => DisplayUnit::M,
                m => return Err(format!("unit {record}: metres times {m}")),
            }),
            INCH => Unit::Display(DisplayUnit::In),
            FOOT => Unit::Display(DisplayUnit::Ft),
            DEGREE => Unit::Display(DisplayUnit::Deg),
            RADIAN => Unit::Display(DisplayUnit::Rad),
            other => {
                return Err(format!(
                    "unit {record}: base unit {} not known",
                    dc::type_text(&other)
                ));
            }
        };
        if found.replace(unit).is_some() {
            return Err(format!("unit {record}: a product of units"));
        }
    }
    Ok(found.unwrap_or(Unit::Display(DisplayUnit::Unitless)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}

impl Op {
    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Rem => "%",
            Op::Pow => "^",
        }
    }

    fn precedence(self) -> u8 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div | Op::Rem => 2,
            Op::Pow => 4,
        }
    }
}

/// An expression tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A number in internal units (cm, rad) and the unit it is shown in.
    Number {
        value: f64,
        unit: DisplayUnit,
    },
    /// Another parameter (its record).
    Parameter(usize),
    Neg(Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    /// A function of its operands (`037aa7f8…`, [`Function`]).
    Call(Function, Vec<Expr>),
}

/// The functions of function nodes (`037aa7f8…`): u32, u16, the display
/// unit, a list (kind 2) of the operands and a u32, the function's code.
/// Codes 1 (cosine, of one angle) and 26 (of three operands, the value and
/// two numbers 1 in units: its value is the first operand's in every case
/// of the test files) are *(seen)*, their values agreeing with the stored
/// ones; the others follow the order of the functions in the public
/// documentation of the expression language, which codes 1 and 26 fit
/// (not seen: a wrong guess shows as an expression that does not give its
/// stored value, and the stored value is used).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    Cos,
    Sin,
    Tan,
    Acos,
    Asin,
    Atan,
    Cosh,
    Sinh,
    Tanh,
    Acosh,
    Asinh,
    Atanh,
    Sqrt,
    Sign,
    Exp,
    Floor,
    Ceil,
    Round,
    Abs,
    Max,
    Min,
    Ln,
    Log,
    Pow,
    /// The first operand with other units (code 26): taken as the first
    /// operand.
    Units,
}

impl Function {
    const CODES: [Function; 24] = [
        Function::Cos,
        Function::Sin,
        Function::Tan,
        Function::Acos,
        Function::Asin,
        Function::Atan,
        Function::Cosh,
        Function::Sinh,
        Function::Tanh,
        Function::Acosh,
        Function::Asinh,
        Function::Atanh,
        Function::Sqrt,
        Function::Sign,
        Function::Exp,
        Function::Floor,
        Function::Ceil,
        Function::Round,
        Function::Abs,
        Function::Max,
        Function::Min,
        Function::Ln,
        Function::Log,
        Function::Pow,
    ];

    /// The function of a code (25, a random number, has none).
    fn of(code: u32) -> Option<Function> {
        match code {
            26 => Some(Function::Units),
            1..=24 => Some(Self::CODES[code as usize - 1]),
            _ => None,
        }
    }

    /// Mitcad's name of it (none for [`Function::Units`]).
    fn name(self) -> &'static str {
        match self {
            Function::Cos => "cos",
            Function::Sin => "sin",
            Function::Tan => "tan",
            Function::Acos => "acos",
            Function::Asin => "asin",
            Function::Atan => "atan",
            Function::Cosh => "cosh",
            Function::Sinh => "sinh",
            Function::Tanh => "tanh",
            Function::Acosh => "acosh",
            Function::Asinh => "asinh",
            Function::Atanh => "atanh",
            Function::Sqrt => "sqrt",
            Function::Sign => "sign",
            Function::Exp => "exp",
            Function::Floor => "floor",
            Function::Ceil => "ceil",
            Function::Round => "round",
            Function::Abs => "abs",
            Function::Max => "max",
            Function::Min => "min",
            Function::Ln => "ln",
            Function::Log => "log",
            Function::Pow => "pow",
            Function::Units => "",
        }
    }

    /// How many operands it takes.
    fn arity(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Function::Units => 3..=3,
            Function::Pow => 2..=2,
            Function::Max | Function::Min => 1..=64,
            _ => 1..=1,
        }
    }

    /// Its value (internal units: angles in radians).
    fn apply(self, a: &[f64]) -> f64 {
        let x = a[0];
        match self {
            Function::Cos => x.cos(),
            Function::Sin => x.sin(),
            Function::Tan => x.tan(),
            Function::Acos => x.acos(),
            Function::Asin => x.asin(),
            Function::Atan => x.atan(),
            Function::Cosh => x.cosh(),
            Function::Sinh => x.sinh(),
            Function::Tanh => x.tanh(),
            Function::Acosh => x.acosh(),
            Function::Asinh => x.asinh(),
            Function::Atanh => x.atanh(),
            Function::Sqrt => x.sqrt(),
            Function::Sign => {
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            Function::Exp => x.exp(),
            Function::Floor => x.floor(),
            Function::Ceil => x.ceil(),
            Function::Round => x.round(),
            Function::Abs => x.abs(),
            Function::Max => a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Function::Min => a.iter().copied().fold(f64::INFINITY, f64::min),
            Function::Ln => x.ln(),
            Function::Log => x.log10(),
            Function::Pow => x.powf(a[1]),
            Function::Units => x,
        }
    }
}

/// Reads an expression tree from its root record.
pub fn expression(dc: &Definitions, record: usize) -> Result<Expr, String> {
    expression_at(dc, record, 0)
}

fn expression_at(dc: &Definitions, record: usize, depth: usize) -> Result<Expr, String> {
    if depth > 256 {
        return Err("an expression nested too deeply (a cycle?)".to_owned());
    }
    let Some(t) = dc.type_of(record).copied() else {
        return Err(format!("no expression record {record}"));
    };
    let mut r = dc.reader(record);
    let fail = |e: dc::DcError| format!("expression record {record}: {e}");
    r.skip(6).map_err(fail)?;
    let unit_ref = r.reference().map_err(fail)?;
    if t[1..] != NODE_SUFFIX {
        return Err(format!(
            "expression node {} is not known",
            dc::type_text(&t)
        ));
    }
    let child = |r: &mut Reader| -> Result<Box<Expr>, String> {
        let c = r
            .reference()
            .map_err(fail)?
            .ok_or_else(|| format!("expression record {record}: a missing operand"))?;
        Ok(Box::new(expression_at(dc, c, depth + 1)?))
    };
    match t[0] {
        0x04 => {
            let value = r.f64().map_err(fail)?;
            let unit = match unit_ref {
                Some(u) => match unit(dc, u)? {
                    Unit::Display(d) => d,
                    Unit::Internal(Quantity::Unitless) => DisplayUnit::Unitless,
                    Unit::Internal(q) => {
                        return Err(format!(
                            "a number in internal units ({q:?}) without a display unit"
                        ));
                    }
                },
                None => DisplayUnit::Unitless,
            };
            if !value.is_finite() {
                return Err(format!("expression record {record}: not a finite number"));
            }
            Ok(Expr::Number { value, unit })
        }
        0x05 => {
            let p = r
                .reference()
                .map_err(fail)?
                .ok_or_else(|| format!("expression record {record}: no parameter"))?;
            if !dc.is(p, &PARAMETER) {
                return Err(format!(
                    "expression record {record}: record {p} is not a parameter"
                ));
            }
            Ok(Expr::Parameter(p))
        }
        0x03 => {
            let operands = r.list().map_err(fail)?.1;
            let code = r.u32().map_err(fail)?;
            let f = Function::of(code)
                .ok_or_else(|| format!("expression record {record}: function {code} not known"))?;
            if !f.arity().contains(&operands.len()) {
                return Err(format!(
                    "expression record {record}: function {code} of {} operands",
                    operands.len()
                ));
            }
            let operands = operands
                .into_iter()
                .map(|v| {
                    let o = dc::reference(v)
                        .ok_or_else(|| format!("expression record {record}: a missing operand"))?;
                    expression_at(dc, o, depth + 1)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Call(f, operands))
        }
        0x0C => Ok(Expr::Neg(child(&mut r)?)),
        // Parentheses: the operand as it is (the text puts them back where
        // the operators need them).
        0x0D => Ok(*child(&mut r)?),
        code => match OPERATORS.iter().find(|(c, _)| *c == code) {
            Some(&(_, op)) => {
                let a = child(&mut r)?;
                let b = child(&mut r)?;
                Ok(Expr::Binary(op, a, b))
            }
            None => Err(format!(
                "expression node {} is not known",
                dc::type_text(&t)
            )),
        },
    }
}

/// Model parameters are named `d<n>` by the file; others are user
/// parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Model,
    User,
}

/// A parameter of the file.
#[derive(Clone, Debug)]
pub struct Parameter {
    pub record: usize,
    pub name: String,
    pub kind: Kind,
    /// In the part's parameter table (other parameter records belong to
    /// annotations and the like, and are not the part's; nor are the
    /// table's `RDxVar<n>`, the features' internal variables).
    pub in_table: bool,
    pub flags: u32,
    pub quantity: Result<Quantity, String>,
    /// The nominal value (cm, rad, or unitless).
    pub value: f64,
    /// The model value (the same in the files seen, unless toleranced).
    pub model_value: f64,
    pub expression: Result<Expr, String>,
}

impl Parameter {
    /// The model computes its value (the flag [`COMPUTED`]).
    pub fn is_computed(&self) -> bool {
        self.flags & COMPUTED != 0
    }
}

/// The parameters of a file.
#[derive(Clone, Debug, Default)]
pub struct Parameters {
    pub list: Vec<Parameter>,
    /// Parameter records that could not be read, with why.
    pub unread: Vec<(usize, String)>,
    /// The part's parameter table (a collection the document record names).
    pub table: Option<usize>,
    by_record: HashMap<usize, usize>,
}

/// Reads an integer parameter record.
fn integer(dc: &Definitions, record: usize, table: Option<usize>) -> dc::Result<Parameter> {
    let header = dc
        .header(record)
        .ok_or_else(|| dc::DcError("no header".to_owned()))?;
    let mut r = dc.fields(record);
    let name = r.text()?;
    r.u32()?;
    let value = f64::from(r.u32()?);
    let model_value = f64::from(r.u32()?);
    Ok(Parameter {
        record,
        kind: kind_of(&name),
        name,
        in_table: table.is_some() && header.context == table,
        flags: header.flags,
        quantity: Ok(Quantity::Unitless),
        value,
        model_value,
        expression: Ok(Expr::Number {
            value,
            unit: DisplayUnit::Unitless,
        }),
    })
}

/// Model parameters are named `d<n>` by the file; others are user
/// parameters.
fn kind_of(name: &str) -> Kind {
    if name.len() > 1 && name.starts_with('d') && name[1..].bytes().all(|b| b.is_ascii_digit()) {
        Kind::Model
    } else {
        Kind::User
    }
}

/// Reads a parameter record.
fn parameter(dc: &Definitions, record: usize, table: Option<usize>) -> dc::Result<Parameter> {
    let header = dc
        .header(record)
        .ok_or_else(|| dc::DcError("no header".to_owned()))?;
    let mut r = dc.fields(record);
    let name = r.text()?;
    r.u32()?;
    let unit_ref = r.reference()?;
    let formula = r.reference()?;
    let value = r.f64()?;
    let model_value = r.f64()?;
    let quantity = match unit_ref {
        Some(u) => unit(dc, u).map(Unit::quantity),
        None => Err("no unit".to_owned()),
    };
    let expression = match formula {
        Some(f) => expression(dc, f),
        None => Err("no expression".to_owned()),
    };
    let kind = kind_of(&name);
    Ok(Parameter {
        record,
        name,
        kind,
        in_table: table.is_some() && header.context == table,
        flags: header.flags,
        quantity,
        value,
        model_value,
        expression,
    })
}

impl Parameters {
    /// Every parameter record of the segment.
    pub fn read(dc: &Definitions) -> Parameters {
        // The part's parameter table: the first collection the document
        // record names.
        let table = dc.of_type(&dc::DOCUMENT).next().and_then(|doc| {
            let bytes = dc.bytes(doc);
            (0..bytes.len().saturating_sub(3))
                .filter_map(|at| {
                    let v = u32::from_le_bytes([
                        bytes[at],
                        bytes[at + 1],
                        bytes[at + 2],
                        bytes[at + 3],
                    ]);
                    (v & 0x8000_0000 != 0).then(|| reference(v)).flatten()
                })
                .find(|&r| dc.is(r, &dc::COLLECTION))
        });
        let mut out = Parameters {
            table,
            ..Parameters::default()
        };
        let records: Vec<usize> = (0..dc.len())
            .filter(|&r| dc.is(r, &PARAMETER) || dc.is(r, &INTEGER))
            .collect();
        for record in records {
            let read = if dc.is(record, &INTEGER) {
                integer(dc, record, table)
            } else {
                parameter(dc, record, table)
            };
            match read {
                Ok(mut p) => {
                    if p.name.starts_with("RDxVar") {
                        p.in_table = false;
                    }
                    out.by_record.insert(record, out.list.len());
                    out.list.push(p);
                }
                Err(e) => out.unread.push((record, e.to_string())),
            }
        }
        out
    }

    /// The parameter records an expression names.
    pub fn named(e: &Expr, out: &mut Vec<usize>) {
        match e {
            Expr::Number { .. } => {}
            Expr::Parameter(r) => out.push(*r),
            Expr::Neg(a) => Self::named(a, out),
            Expr::Binary(_, a, b) => {
                Self::named(a, out);
                Self::named(b, out);
            }
            Expr::Call(_, operands) => {
                for o in operands {
                    Self::named(o, out);
                }
            }
        }
    }

    pub fn by_record(&self, record: usize) -> Option<&Parameter> {
        self.by_record.get(&record).map(|&i| &self.list[i])
    }

    /// The parameters of the part's table.
    pub fn table(&self) -> impl Iterator<Item = &Parameter> {
        self.list.iter().filter(|p| p.in_table)
    }

    /// Evaluates a parameter's expression (internal units), following the
    /// parameters it names (the values of computed ones).
    pub fn evaluate(&self, p: &Parameter) -> Result<f64, String> {
        self.evaluate_depth(p, 0)
    }

    fn evaluate_depth(&self, p: &Parameter, depth: usize) -> Result<f64, String> {
        if depth > 256 {
            return Err(format!(
                "{}: the parameters refer to each other in a cycle",
                p.name
            ));
        }
        let e = p.expression.as_ref().map_err(Clone::clone)?;
        self.eval(e, depth)
    }

    fn eval(&self, e: &Expr, depth: usize) -> Result<f64, String> {
        Ok(match e {
            Expr::Number { value, .. } => *value,
            Expr::Parameter(r) => {
                let p = self
                    .by_record(*r)
                    .ok_or_else(|| format!("parameter record {r} was not read"))?;
                // One whose expression is not read comes in as its stored
                // value too.
                if p.is_computed() || p.expression.is_err() {
                    p.value
                } else {
                    self.evaluate_depth(p, depth + 1)?
                }
            }
            Expr::Neg(a) => -self.eval(a, depth)?,
            Expr::Call(f, operands) => {
                let values = operands
                    .iter()
                    .map(|o| self.eval(o, depth))
                    .collect::<Result<Vec<f64>, String>>()?;
                f.apply(&values)
            }
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.eval(a, depth)?, self.eval(b, depth)?);
                match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                    Op::Div => a / b,
                    Op::Rem => a % b,
                    Op::Pow => a.powf(b),
                }
            }
        })
    }

    /// The expression in Mitcad's expression language. A number is
    /// written in the unit the file shows it in; a plain unitless number
    /// for a length or angle is written in `length` or degrees.
    pub fn text(&self, p: &Parameter, length: DisplayUnit) -> Result<String, String> {
        let e = p.expression.as_ref().map_err(Clone::clone)?;
        if let (
            Expr::Number {
                value,
                unit: DisplayUnit::Unitless,
            },
            Ok(q),
        ) = (e, &p.quantity)
        {
            let unit = match q {
                Quantity::Length => length,
                Quantity::Angle => DisplayUnit::Deg,
                Quantity::Unitless => DisplayUnit::Unitless,
            };
            return Ok(number(*value, unit));
        }
        let mut out = String::new();
        self.write(e, 0, &mut out)?;
        Ok(out)
    }

    fn write(&self, e: &Expr, outer: u8, out: &mut String) -> Result<(), String> {
        match e {
            Expr::Number { value, unit } => {
                let text = number(*value, *unit);
                // A negative number binds like a negation.
                if text.starts_with('-') && outer > 0 {
                    out.push('(');
                    out.push_str(&text);
                    out.push(')');
                } else {
                    out.push_str(&text);
                }
            }
            Expr::Parameter(r) => {
                let p = self
                    .by_record(*r)
                    .ok_or_else(|| format!("parameter record {r} was not read"))?;
                out.push_str(&p.name);
            }
            // The first operand with other units: the operand as it is.
            Expr::Call(Function::Units, operands) => self.write(&operands[0], outer, out)?,
            Expr::Call(f, operands) => {
                out.push_str(f.name());
                out.push('(');
                for (k, o) in operands.iter().enumerate() {
                    if k > 0 {
                        out.push_str("; ");
                    }
                    self.write(o, 0, out)?;
                }
                out.push(')');
            }
            Expr::Neg(a) => {
                let wrap = outer > 3;
                if wrap {
                    out.push('(');
                }
                out.push('-');
                self.write(a, 3, out)?;
                if wrap {
                    out.push(')');
                }
            }
            Expr::Binary(op, a, b) => {
                let prec = op.precedence();
                let wrap = prec < outer;
                if wrap {
                    out.push('(');
                }
                // Left-associative operators: the right operand of equal
                // precedence needs parentheses; `^` the other way round.
                let (left, right) = if *op == Op::Pow {
                    (prec + 1, prec)
                } else {
                    (prec, prec + 1)
                };
                self.write(a, left, out)?;
                out.push(' ');
                out.push_str(op.symbol());
                out.push(' ');
                self.write(b, right, out)?;
                if wrap {
                    out.push(')');
                }
            }
        }
        Ok(())
    }
}

/// A number (internal units) in a unit: at most 12 significant digits, so
/// the rounding of the file's unit conversion (6.000000000000001 mm) does
/// not show, then the shortest form that reads back as that.
pub fn number(internal: f64, unit: DisplayUnit) -> String {
    let value = internal / unit.scale();
    let rounded: f64 = format!("{value:.11e}").parse().unwrap_or(value);
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let symbol = unit.symbol();
    if symbol.is_empty() {
        format!("{rounded}")
    } else {
        format!("{rounded} {symbol}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dc::{COLLECTION, DOCUMENT};

    /// Records of a small parameter table, built as the files store them.
    pub(crate) struct Builder {
        pub records: Vec<([u8; 16], Vec<u8>)>,
    }

    fn reference_bytes(r: usize) -> [u8; 4] {
        (r as u32 + 1).to_le_bytes()
    }

    impl Builder {
        pub fn new() -> Builder {
            let mut b = Builder {
                records: Vec::new(),
            };
            // The document names the table (record 1).
            let mut doc = vec![0u8; 22];
            doc.extend_from_slice(&(0x8000_0002u32).to_le_bytes());
            b.records.push((DOCUMENT, doc));
            b.records.push((COLLECTION, vec![0u8; 30]));
            b
        }

        fn push(&mut self, t: [u8; 16], bytes: Vec<u8>) -> usize {
            self.records.push((t, bytes));
            self.records.len() - 1
        }

        fn base(&mut self, t: [u8; 16], magnitude: f64) -> usize {
            let mut b = vec![0u8; 6];
            b.extend_from_slice(&magnitude.to_le_bytes());
            b.extend_from_slice(&1f64.to_le_bytes());
            self.push(t, b)
        }

        pub fn unit(&mut self, base: [u8; 16], magnitude: f64) -> usize {
            let base = self.base(base, magnitude);
            let mut b = vec![0u8; 6];
            b.extend_from_slice(&[3, 0, 0, 0x30, 1, 0, 0, 0, 0, 0, 0, 0]);
            b.extend_from_slice(&reference_bytes(base));
            b.extend_from_slice(&[3, 0, 0, 0x30, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
            self.push(UNIT, b)
        }

        pub fn number(&mut self, value: f64, unit: usize) -> usize {
            let mut b = vec![0u8; 6];
            b.extend_from_slice(&reference_bytes(unit));
            b.extend_from_slice(&value.to_le_bytes());
            b.extend_from_slice(&[0; 6]);
            self.push(NUMBER, b)
        }

        pub fn node(&mut self, code: u8, operands: &[usize]) -> usize {
            let mut t = NUMBER;
            t[0] = code;
            let mut b = vec![0u8; 6];
            b.extend_from_slice(&[0; 4]);
            for &o in operands {
                b.extend_from_slice(&((o as u32 + 1) | 0x8000_0000).to_le_bytes());
            }
            self.push(t, b)
        }

        /// A function node (`037aa7f8…`): its operands as a list, then
        /// its code.
        pub fn call(&mut self, code: u32, unit: usize, operands: &[usize]) -> usize {
            let mut t = NUMBER;
            t[0] = 0x03;
            let mut b = vec![0u8; 6];
            b.extend_from_slice(&reference_bytes(unit));
            b.extend_from_slice(&[2, 0, 0, 0x30]);
            b.extend_from_slice(&(operands.len() as u32).to_le_bytes());
            b.extend_from_slice(&[4, 0, 0, 0, 0, 0, 0, 0]);
            for &o in operands {
                b.extend_from_slice(&reference_bytes(o));
            }
            b.extend_from_slice(&code.to_le_bytes());
            self.push(t, b)
        }

        pub fn parameter(&mut self, name: &str, unit: usize, formula: usize, value: f64) -> usize {
            let mut b = vec![0u8; 14];
            b.extend_from_slice(&0x8000_0002u32.to_le_bytes()); // context: the table
            b.extend_from_slice(&[0; 4]);
            b.extend_from_slice(&[0xFF; 8]);
            let units: Vec<u16> = name.encode_utf16().collect();
            b.extend_from_slice(&(units.len() as u32).to_le_bytes());
            for u in units {
                b.extend_from_slice(&u.to_le_bytes());
            }
            b.extend_from_slice(&[0; 4]);
            b.extend_from_slice(&reference_bytes(unit));
            b.extend_from_slice(&reference_bytes(formula));
            b.extend_from_slice(&value.to_le_bytes());
            b.extend_from_slice(&value.to_le_bytes());
            b.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
            self.push(PARAMETER, b)
        }

        pub fn definitions(&self) -> Definitions {
            Definitions::from_records(25, &self.records)
        }
    }

    #[test]
    fn reads_translates_and_evaluates_expressions() {
        let mut b = Builder::new();
        let length = b.unit(LENGTH, 1.0);
        let angle = b.unit(ANGLE, 1.0);
        let mm = b.unit(METRE, 1e-3);
        let inch = b.unit(INCH, 1.0);
        let deg = b.unit(DEGREE, 1.0);
        let none = b.unit(UNITLESS, 1.0);
        // d0 = 45 mm (4.5 cm); d1 = 2 in; d2 = 30 deg.
        let n = b.number(4.5, mm);
        let d0 = b.parameter("d0", length, n, 4.5);
        let n = b.number(5.08, inch);
        b.parameter("d1", length, n, 5.08);
        let n = b.number(std::f64::consts::PI / 6.0, deg);
        b.parameter("d2", angle, n, std::f64::consts::PI / 6.0);
        // Width = -(d0 * 0.5) + 2 in - (1 mm - 1 mm)
        let r = b.node(0x05, &[d0]);
        let half = b.number(0.5, none);
        let product = b.node(0x08, &[r, half]);
        let negated = b.node(0x0C, &[product]);
        let two = b.number(5.08, inch);
        let sum = b.node(0x06, &[negated, two]);
        let one = b.number(0.1, mm);
        let one2 = b.number(0.1, mm);
        let diff = b.node(0x07, &[one, one2]);
        let total = b.node(0x07, &[sum, diff]);
        b.parameter("Width", length, total, 5.08 - 2.25);
        let dc = b.definitions();
        let params = Parameters::read(&dc);
        assert!(params.unread.is_empty(), "{:?}", params.unread);
        assert_eq!(params.table, Some(1));
        let texts: Vec<(String, String, Kind)> = params
            .table()
            .map(|p| {
                (
                    p.name.clone(),
                    params.text(p, DisplayUnit::Mm).unwrap(),
                    p.kind,
                )
            })
            .collect();
        assert_eq!(
            texts,
            [
                ("d0".into(), "45 mm".into(), Kind::Model),
                ("d1".into(), "2 in".into(), Kind::Model),
                ("d2".into(), "30 deg".into(), Kind::Model),
                (
                    "Width".into(),
                    "-(d0 * 0.5) + 2 in - (1 mm - 1 mm)".into(),
                    Kind::User
                ),
            ]
        );
        for p in params.table() {
            let v = params.evaluate(p).unwrap();
            assert!((v - p.value).abs() < 1e-12, "{}: {v} {}", p.name, p.value);
        }
        assert_eq!(params.list[2].quantity, Ok(Quantity::Angle));
    }

    #[test]
    fn reports_what_it_cannot_read() {
        let mut b = Builder::new();
        let length = b.unit(LENGTH, 1.0);
        let mut unknown = NUMBER;
        unknown[0] = 0x0E;
        let n = b.push(unknown, vec![0; 14]);
        let d0_record = b.parameter("d0", length, n, 1.0);
        // A parameter referring to itself.
        let cycle_ref = b.records.len() + 1;
        let r = b.node(0x05, &[cycle_ref]);
        b.parameter("d1", length, r, 1.0);
        // A parameter naming d0, whose expression is not read: d0 comes
        // in as its stored value.
        let r = b.node(0x05, &[d0_record]);
        b.parameter("d2", length, r, 1.0);
        let dc = b.definitions();
        let params = Parameters::read(&dc);
        let d0 = &params.list[0];
        assert!(d0.expression.as_ref().unwrap_err().contains("not known"));
        let d1 = &params.list[1];
        assert!(params.evaluate(d1).unwrap_err().contains("cycle"));
        assert_eq!(params.evaluate(&params.list[2]), Ok(1.0));
        assert_eq!(number(0.6000000000000001, DisplayUnit::Mm), "6 mm");
        assert_eq!(number(-0.0, DisplayUnit::Unitless), "0");
    }

    #[test]
    fn reads_functions() {
        let mut b = Builder::new();
        let length = b.unit(LENGTH, 1.0);
        let mm = b.unit(METRE, 1e-3);
        let deg = b.unit(DEGREE, 1.0);
        let none = b.unit(UNITLESS, 1.0);
        // SW = 2 mm; d1 = SW / cos(30 deg); d2 = the units function of
        // SW / 2 with 1 mm and 1 deg: SW / 2.
        let n = b.number(0.2, mm);
        let sw = b.parameter("SW", length, n, 0.2);
        let r = b.node(0x05, &[sw]);
        let thirty = b.number(std::f64::consts::PI / 6.0, deg);
        let cos = b.call(1, none, &[thirty]);
        let quotient = b.node(0x09, &[r, cos]);
        b.parameter("d1", length, quotient, 0.2 / 0.75f64.sqrt());
        let r = b.node(0x05, &[sw]);
        let two = b.number(2.0, none);
        let half = b.node(0x09, &[r, two]);
        let one_mm = b.number(0.1, mm);
        let one_deg = b.number(std::f64::consts::PI / 180.0, deg);
        let units = b.call(26, mm, &[half, one_mm, one_deg]);
        b.parameter("d2", length, units, 0.1);
        // A function not known, and one of too many operands.
        let x = b.number(1.0, none);
        let random = b.call(25, none, &[x]);
        b.parameter("d3", length, random, 1.0);
        let x = b.number(1.0, none);
        let y = b.number(1.0, none);
        let cos2 = b.call(1, none, &[x, y]);
        b.parameter("d4", length, cos2, 1.0);
        let dc = b.definitions();
        let params = Parameters::read(&dc);
        let texts: Vec<String> = params.list[1..3]
            .iter()
            .map(|p| params.text(p, DisplayUnit::Mm).unwrap())
            .collect();
        assert_eq!(texts, ["SW / cos(30 deg)", "SW / 2"]);
        for p in &params.list[..3] {
            let v = params.evaluate(p).unwrap();
            assert!((v - p.value).abs() < 1e-12, "{}: {v} {}", p.name, p.value);
        }
        assert!(
            params.list[3]
                .expression
                .as_ref()
                .unwrap_err()
                .contains("function 25 not known")
        );
        assert!(
            params.list[4]
                .expression
                .as_ref()
                .unwrap_err()
                .contains("of 2 operands")
        );
        let mut named = Vec::new();
        Parameters::named(params.list[2].expression.as_ref().unwrap(), &mut named);
        assert_eq!(named, [sw]);
    }

    #[test]
    fn reads_parentheses_and_computed_parameters() {
        let mut b = Builder::new();
        let length = b.unit(LENGTH, 1.0);
        let mm = b.unit(METRE, 1e-3);
        let none = b.unit(UNITLESS, 1.0);
        // d0 = 190 mm, but the model computed 21 cm.
        let n = b.number(19.0, mm);
        let d0 = b.parameter("d0", length, n, 21.0);
        let header = &mut b.records[d0].1;
        header[10..14].copy_from_slice(&(0x0102_4200u32).to_le_bytes());
        // d1 = (d0 / 2 - 62 mm / 2) * 2: 21 / 2 - 3.1 = 7.4 cm, times 2.
        let r = b.node(0x05, &[d0]);
        let two = b.number(2.0, none);
        let half = b.node(0x09, &[r, two]);
        let n62 = b.number(6.2, mm);
        let two2 = b.number(2.0, none);
        let half62 = b.node(0x09, &[n62, two2]);
        let diff = b.node(0x07, &[half, half62]);
        let group = b.node(0x0D, &[diff]);
        let two3 = b.number(2.0, none);
        let product = b.node(0x08, &[group, two3]);
        b.parameter("d1", length, product, 14.8);
        let dc = b.definitions();
        let params = Parameters::read(&dc);
        assert!(params.unread.is_empty(), "{:?}", params.unread);
        let (p0, p1) = (&params.list[0], &params.list[1]);
        assert!(p0.is_computed() && !p1.is_computed());
        // The computed parameter's expression gives another value; the
        // parameters that name it take its value.
        assert!((params.evaluate(p0).unwrap() - 19.0).abs() < 1e-12);
        assert!((params.evaluate(p1).unwrap() - 14.8).abs() < 1e-12);
        assert_eq!(
            params.text(p1, DisplayUnit::Mm).unwrap(),
            "(d0 / 2 - 62 mm / 2) * 2"
        );
    }
}
