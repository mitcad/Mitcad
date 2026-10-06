// SPDX-License-Identifier: MIT
//! Parameters and expressions (stage 4): FreeCAD's quantities as Mitcad
//! parameters, and its expressions as Mitcad expressions of them.
//!
//! - Parameters, made before the timeline in the order of their
//!   dependencies: every spreadsheet cell with an alias (named after it)
//!   and every cell an expression names by its address (`<sheet>_B3`), the
//!   numeric properties of VarSets (named after them), the sketches' named
//!   constraints (named after them; the sketch's dimension then is that
//!   parameter, adopted as the sketch's), and any other property an
//!   expression refers to (`<label>_<property>`, `Pad_Length`; the feature
//!   made of it uses the parameter and adopts it), of the objects whose
//!   values the import carries over (features of the history, sketches;
//!   an architecture model's walls' expressions make none). A name Mitcad
//!   cannot take, or one several of these share, gets its owner's label in
//!   front (`<owner>_<name>`), else a number. A parameter's expression is
//!   FreeCAD's translated, else FreeCAD's value (a cell keeps no value: one
//!   whose formula does not translate is left out).
//! - Expressions bound to the values the import carries over (features'
//!   lengths, angles, sizes and counts, sketches' dimensions, cones'
//!   sizes, attachment offsets along a sketch's or a datum plane's normal,
//!   a sketch's turn about its support's x or y axis) become those values'
//!   expressions (Mitcad's dimension parameters of them).
//! - Translation ([`Importer::mitcad_text`]): references to the
//!   parameters' names, units (`gon` as `grad`, `thou` as `mil`, `"` and
//!   `'` as `in` and `ft`, others by their factor), `pi` and `e` as `PI`
//!   and `E`, `c ? a : b` as `if(c; a; b)`, `log` and `log10` as `ln` and
//!   `log`, `mod(a; b)` as `a % b`, `hypot`, `cath`, `cbrt`, `trunc`,
//!   `sum` and `average` (ranges too) written out; `-x^2` (FreeCAD's signs
//!   bind tighter) as `(-x)^2`. A result without a unit where FreeCAD has
//!   a length or an angle gets FreeCAD's unit (`B3 * 1 mm`).
//! - Check: every translated expression is evaluated and compared with
//!   FreeCAD's stored value (the property's, the constraint's; 1e-9
//!   relative; an integer property's expression rounded as FreeCAD rounds
//!   it, `round(…)`, when that gives the value); one that differs, or does
//!   not translate (other documents,
//!   vectors, rotations and shapes' measures, strings, functions without
//!   an equivalent, other quantities), keeps FreeCAD's value, and the
//!   report says why.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use mitcad_freecad::expression::{
    self as fe, BinaryOp, Component, Constant, Expr, Reference, range_cells,
};
use mitcad_freecad::sketch::{self as fc, ConstraintType};
use mitcad_freecad::spreadsheet::{Content, Sheet, is_sheet};
use mitcad_freecad::{Object, Value as FcValue};
use mitcad_model::expr::{Dims, Quantity, Unit, value_to_expression};
use mitcad_model::{FeatureUid, Kernel};
use serde_json::{Value, json};

use super::Importer;
use super::report::{ExpressionOutcome, ExpressionReport, ParameterReport};

/// Relative agreement of a translated expression with FreeCAD's value.
const AGREEMENT: f64 = 1e-9;

/// What a value measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Length,
    Angle,
    Number,
}

impl Kind {
    fn dims(self) -> Dims {
        match self {
            Kind::Length => Dims::LENGTH,
            Kind::Angle => Dims::ANGLE,
            Kind::Number => Dims::NONE,
        }
    }

    fn unit(self) -> Unit {
        match self {
            Kind::Length => Unit::MM,
            Kind::Angle => Unit::DEG,
            Kind::Number => Unit::NONE,
        }
    }

    /// FreeCAD's number of a property (millimetres, degrees) in Mitcad's
    /// units (millimetres, radians).
    pub fn canonical(self, freecad: f64) -> f64 {
        match self {
            Kind::Angle => freecad.to_radians(),
            _ => freecad,
        }
    }

    fn of_dims(dims: Dims) -> Option<Kind> {
        [Kind::Length, Kind::Angle, Kind::Number]
            .into_iter()
            .find(|k| k.dims() == dims)
    }
}

/// A quantity of the document an expression can name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Source {
    Cell {
        sheet: String,
        address: String,
    },
    /// A property, or a part of one: `Length`, `AttachmentOffset.Base.z`.
    Property {
        object: String,
        path: String,
    },
    Constraint {
        sketch: String,
        index: usize,
    },
}

impl Source {
    pub fn property(object: &str, path: &str) -> Source {
        Source::Property {
            object: object.to_owned(),
            path: path.to_owned(),
        }
    }

    fn object(&self) -> &str {
        match self {
            Source::Cell { sheet, .. } => sheet,
            Source::Property { object, .. } => object,
            Source::Constraint { sketch, .. } => sketch,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Cell { sheet, address } => write!(f, "{sheet}.{address}"),
            Source::Property { object, path } => write!(f, "{object}.{path}"),
            Source::Constraint { sketch, index } => write!(f, "{sketch}.Constraints[{index}]"),
        }
    }
}

/// A value of a definition: FreeCAD's number in Mitcad's units (mm,
/// radians, or as it is), and the Mitcad expression it comes from when
/// FreeCAD's is parametric.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Q {
    pub value: f64,
    pub kind: Kind,
    pub text: Option<String>,
}

impl Q {
    pub fn number(value: f64, kind: Kind) -> Q {
        Q {
            value,
            kind,
            text: None,
        }
    }

    /// The definition's value: the expression, else the number.
    pub fn json(&self) -> Value {
        match &self.text {
            Some(t) => json!(t),
            None => json!(self.value),
        }
    }

    /// Its text in an expression of others: the expression, else the
    /// number with its unit.
    fn operand(&self) -> String {
        match &self.text {
            Some(t) => t.clone(),
            None => value_to_expression(self.value, self.kind.unit()),
        }
    }

    fn derive(&self, value: f64, text: impl FnOnce(&str) -> String) -> Q {
        Q {
            value,
            kind: self.kind,
            text: self.text.as_deref().map(text),
        }
    }

    pub fn neg(&self) -> Q {
        self.derive(-self.value, |t| format!("-{}", wrap(t)))
    }

    /// Times +1 or −1.
    pub fn signed(&self, sign: f64) -> Q {
        if sign < 0.0 { self.neg() } else { self.clone() }
    }

    pub fn scale(&self, k: f64) -> Q {
        if k == 1.0 {
            return self.clone();
        }
        if k < 0.0 {
            return self.neg().scale(-k);
        }
        let reciprocal = 1.0 / k;
        self.derive(self.value * k, |t| {
            if reciprocal.fract() == 0.0 && reciprocal.abs() < 1e6 {
                format!("{} / {}", wrap(t), reciprocal)
            } else {
                format!(
                    "{} * {}",
                    wrap(t),
                    mitcad_model::expr::format_number_exact(k)
                )
            }
        })
    }

    /// A value made of several: an expression when one of them is.
    pub fn combine(
        parts: &[&Q],
        value: f64,
        kind: Kind,
        text: impl FnOnce(&[String]) -> String,
    ) -> Q {
        let parametric = parts.iter().any(|q| q.text.is_some());
        let operands: Vec<String> = parts.iter().map(|q| wrap(&q.operand())).collect();
        Q {
            value,
            kind,
            text: parametric.then(|| text(&operands)),
        }
    }
}

/// An expression in parentheses unless it is a name, a number (with its
/// unit) or a call.
fn wrap(text: &str) -> String {
    let t = text.trim();
    let word = |w: &str| {
        w.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    };
    let simple = word(t)
        || matches!(t.split_whitespace().collect::<Vec<_>>().as_slice(),
                    [n, u] if n.parse::<f64>().is_ok() && (word(u) || *u == "°"));
    let call = t.ends_with(')')
        && t.find('(').is_some_and(|open| {
            open > 0
                && t[..open].chars().all(|c| c.is_alphanumeric() || c == '_')
                && balanced_once(&t[open..])
        });
    if (simple && !t.is_empty()) || call {
        t.to_owned()
    } else {
        format!("({t})")
    }
}

/// Whether `(…)` closes only at its end.
fn balanced_once(t: &str) -> bool {
    let mut depth = 0;
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i + 1 != t.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// An expression bound to a property (FreeCAD's `ExpressionEngine`).
#[derive(Debug, Clone)]
struct Binding {
    owner: String,
    text: String,
    /// Its row in the report's expressions.
    row: usize,
}

/// A quantity to make a parameter of.
#[derive(Debug, Clone)]
struct Wanted {
    source: Source,
    /// The name it would like, and the longer one with its owner's label
    /// when that is not in it already.
    name: String,
    longer: Option<String>,
    /// What it measures, when FreeCAD says (a cell's comes from its
    /// formula).
    kind: Option<Kind>,
    /// FreeCAD's expression and the object it belongs to.
    expression: Option<(String, String)>,
    /// FreeCAD's value in Mitcad's units, when the file keeps one.
    stored: Option<f64>,
    deps: Vec<Source>,
    /// For the parameter's comment and the report.
    describe: String,
}

/// The parameters' state during an import.
#[derive(Debug, Default)]
pub(super) struct Params {
    /// The parameter made of each quantity.
    names: HashMap<Source, String>,
    bindings: HashMap<Source, Binding>,
    /// The values given to definitions, once per quantity.
    quantities: HashMap<Source, Q>,
    /// Each sketch's constraints by index: name, type and value
    /// (millimetres, radians).
    constraints: HashMap<String, Vec<(String, ConstraintType, f64)>>,
    /// Parameters changed after the import (`set_parameters`).
    pub changed: bool,
}

/// Mitcad's precedence levels for printing (higher binds tighter).
const COMPARE: u8 = 1;
const ADD: u8 = 2;
const MUL: u8 = 3;
const UNARY: u8 = 4;
const POWER: u8 = 5;
const ATOM: u8 = 7;

fn paren(text: String, level: u8, needed: u8) -> String {
    if level < needed {
        format!("({text})")
    } else {
        text
    }
}

/// A FreeCAD unit as Mitcad's unit and the factor into it.
fn mitcad_unit(name: &str) -> Option<(&'static str, f64)> {
    Some(match name {
        "mm" => ("mm", 1.0),
        "cm" => ("cm", 1.0),
        "m" => ("m", 1.0),
        "km" => ("km", 1.0),
        "um" | "µm" => ("um", 1.0),
        "nm" => ("um", 1e-3),
        "dm" => ("cm", 10.0),
        "in" | "\"" => ("in", 1.0),
        "ft" | "'" => ("ft", 1.0),
        "yd" => ("yd", 1.0),
        "mi" => ("mi", 1.0),
        "mil" | "thou" => ("mil", 1.0),
        "deg" | "°" => ("deg", 1.0),
        "rad" => ("rad", 1.0),
        "gon" => ("grad", 1.0),
        "′" => ("deg", 1.0 / 60.0),
        "″" => ("deg", 1.0 / 3600.0),
        _ => return None,
    })
}

/// A property type's kind (lengths, angles, plain numbers).
fn property_kind(type_name: &str) -> Option<Kind> {
    let t = type_name.strip_prefix("App::Property")?;
    Some(match t {
        "Length" | "Distance" | "XDistance" | "YDistance" | "ZDistance" => Kind::Length,
        "Angle" => Kind::Angle,
        "Integer" | "IntegerConstraint" | "Float" | "FloatConstraint" | "Percent" | "Precision"
        | "Quantity" | "QuantityConstraint" => Kind::Number,
        _ => return None,
    })
}

/// A dimensional constraint's kind.
fn constraint_kind(kind: ConstraintType) -> Option<Kind> {
    match kind {
        ConstraintType::Distance
        | ConstraintType::DistanceX
        | ConstraintType::DistanceY
        | ConstraintType::Radius
        | ConstraintType::Diameter => Some(Kind::Length),
        ConstraintType::Angle => Some(Kind::Angle),
        ConstraintType::Weight | ConstraintType::SnellsLaw => Some(Kind::Number),
        _ => None,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= AGREEMENT * a.abs().max(b.abs()).max(1e-3)
}

/// A placement's rotation as its unit axis (none for no turn) and its angle
/// (radians, from 0 to a whole turn, as FreeCAD's `Rotation.Angle`).
fn turn_of(p: &mitcad_freecad::Placement) -> (Option<[f64; 3]>, f64) {
    let [x, y, z, w] = p.rotation;
    let s = (x * x + y * y + z * z).sqrt();
    if s < 1e-12 {
        return (None, 0.0);
    }
    (Some([x / s, y / s, z / s]), 2.0 * s.atan2(w))
}

/// A translated expression of an integer property: FreeCAD rounds its
/// result to the nearest integer, halves away from zero (`Count / 2` with
/// a count of 5 is 3), as Mitcad's `round` does. The expression as it is
/// when it already gives FreeCAD's value.
fn rounded(text: String, value: Quantity, stored: f64, integer: bool) -> (String, Quantity) {
    if integer
        && value.dims.is_dimensionless()
        && !close(value.value, stored)
        && close(value.value.round(), stored)
    {
        (
            format!("round({text})"),
            Quantity::new(value.value.round(), value.dims),
        )
    } else {
        (text, value)
    }
}

/// A FreeCAD name as a Mitcad parameter name, or None when it cannot be
/// one as it is.
fn valid(name: &str) -> Option<String> {
    mitcad_model::expr::is_valid_name(name).then(|| name.to_owned())
}

impl<K: Kernel> Importer<'_, '_, K> {
    fn fc_document(&self) -> &mitcad_freecad::Document {
        &self.sources[0].file.document
    }

    /// The quantity a reference in `owner`'s expression names.
    fn source_of(&self, owner: &Object, r: &Reference) -> Result<Source, String> {
        if r.document.is_some() {
            return Err(format!("{r}: another document's object"));
        }
        let doc = self.fc_document();
        let names = r.names();
        let (object, path): (&Object, &[Component]) = if let Some(label) = &r.label {
            let o = doc
                .objects
                .iter()
                .find(|o| o.label() == label)
                .ok_or_else(|| format!("{r}: no object labelled {label}"))?;
            (o, &r.path[..])
        } else if r.relative {
            (owner, &r.path[..])
        } else {
            let first = names.first().copied().unwrap_or_default();
            if is_sheet(&owner.type_name) && r.path.len() == 1 {
                (owner, &r.path[..])
            } else if let Some(o) = doc.object(first) {
                (o, &r.path[1..])
            } else if owner.property(first).is_some() {
                (owner, &r.path[..])
            } else {
                return Err(format!("{r}: no object or property {first}"));
            }
        };
        if is_sheet(&object.type_name) {
            let [Component::Name(cell)] = path else {
                return Err(format!("{r}: not a cell"));
            };
            let sheet = Sheet::of(object);
            let address = match sheet.named(cell) {
                Some(c) => c.address.clone(),
                None if fe::is_cell_address(cell) => cell.clone(),
                None => return Err(format!("{r}: no cell {cell}")),
            };
            return Ok(Source::Cell {
                sheet: object.name.clone(),
                address,
            });
        }
        if let [Component::Name(c), rest @ ..] = path
            && c == "Constraints"
            && super::is_sketch(&object.type_name)
        {
            let index = match rest {
                [Component::Index(i)] => usize::try_from(*i).map_err(|_| format!("{r}"))?,
                [Component::Name(n)] => self
                    .params
                    .constraints
                    .get(&object.name)
                    .and_then(|list| list.iter().position(|(x, _, _)| x == n))
                    .ok_or_else(|| format!("{r}: no constraint named {n}"))?,
                _ => return Err(format!("{r}: not a constraint")),
            };
            return Ok(Source::Constraint {
                sketch: object.name.clone(),
                index,
            });
        }
        let parts: Vec<&str> = path
            .iter()
            .map(|c| match c {
                Component::Name(n) => Ok(n.as_str()),
                Component::Index(_) => Err(format!("{r}: an indexed property")),
            })
            .collect::<Result<_, _>>()?;
        let Some(first) = parts.first() else {
            return Err(format!("{r}: an object, not a property"));
        };
        if object.property(first).is_none() {
            return Err(format!("{r}: {} has no property {first}", object.name));
        }
        Ok(Source::property(&object.name, &parts.join(".")))
    }

    /// The kind and FreeCAD's value (in Mitcad's units) of a quantity.
    fn stored(&self, source: &Source) -> Result<(Option<Kind>, Option<f64>), String> {
        let doc = self.fc_document();
        match source {
            Source::Cell { sheet, address } => {
                let cell = doc
                    .object(sheet)
                    .map(Sheet::of)
                    .and_then(|s| s.cell(address).cloned());
                Ok(match cell.as_ref().map(|c| c.content()) {
                    Some(Content::Number(n)) => (Some(Kind::Number), n.parse().ok()),
                    Some(Content::Text(_)) => return Err(format!("{source} holds text")),
                    Some(Content::Empty) | None => return Err(format!("{source} is empty")),
                    Some(Content::Expression(_)) => (None, None),
                })
            }
            Source::Constraint { sketch, index } => {
                let (_, type_, value) = self
                    .params
                    .constraints
                    .get(sketch)
                    .and_then(|list| list.get(*index))
                    .ok_or_else(|| format!("{source}: no such constraint"))?;
                let kind = constraint_kind(*type_)
                    .ok_or_else(|| format!("{source}: a {} has no value", type_.name()))?;
                Ok((Some(kind), Some(*value)))
            }
            Source::Property { object, path } => {
                let o = doc
                    .object(object)
                    .ok_or_else(|| format!("{source}: no object"))?;
                let mut parts = path.split('.');
                let first = parts.next().unwrap_or_default();
                let rest: Vec<&str> = parts.collect();
                let p = o
                    .property(first)
                    .ok_or_else(|| format!("{source}: no property"))?;
                match (&p.value, rest.as_slice()) {
                    (FcValue::Placement(pl), ["Rotation", "Angle"]) => {
                        Ok((Some(Kind::Angle), Some(turn_of(pl).1)))
                    }
                    (FcValue::Placement(pl), ["Base", axis]) => {
                        let k = ["x", "y", "z"]
                            .iter()
                            .position(|a| a == axis)
                            .ok_or_else(|| format!("{source}: not a part of a placement"))?;
                        Ok((Some(Kind::Length), Some(pl.position[k])))
                    }
                    (FcValue::Vector(v), [axis]) => {
                        let k = ["x", "y", "z"]
                            .iter()
                            .position(|a| a == axis)
                            .ok_or_else(|| format!("{source}: not a part of a vector"))?;
                        Ok((Some(Kind::Length), Some(v[k])))
                    }
                    (value, []) => {
                        let kind = property_kind(&p.type_name).ok_or_else(|| {
                            format!("{source}: a {} is not a number", p.type_name)
                        })?;
                        let v = value
                            .as_f64()
                            .ok_or_else(|| format!("{source}: no number"))?;
                        // A quantity's unit is its expression's.
                        if p.type_name.contains("Quantity") {
                            return Ok((None, Some(v)));
                        }
                        Ok((Some(kind), Some(kind.canonical(v))))
                    }
                    _ => Err(format!("{source}: a part of a property")),
                }
            }
        }
    }

    // Translation.

    /// FreeCAD's expression in Mitcad's language, with the parameters made
    /// so far: the text and its precedence level.
    fn mitcad_text(&self, owner: &Object, e: &Expr) -> Result<(String, u8), String> {
        let text = |e: &Expr, needed: u8| -> Result<String, String> {
            let (t, level) = self.mitcad_text(owner, e)?;
            Ok(paren(t, level, needed))
        };
        Ok(match e {
            Expr::Number(v) => (mitcad_model::expr::format_number_exact(*v), ATOM),
            Expr::WithUnit(inner, units) => {
                let [unit] = units.as_slice() else {
                    let names: Vec<&str> = units.iter().map(|u| u.name.as_str()).collect();
                    return Err(format!(
                        "the unit {} is not a length or an angle",
                        names.join(" ")
                    ));
                };
                let (name, factor) = mitcad_unit(&unit.name)
                    .ok_or_else(|| format!("the unit {} is not a length or an angle", unit.name))?;
                let factor = factor.powi(unit.power);
                let suffix = if unit.power == 1 {
                    name.to_owned()
                } else {
                    format!("{name}^{}", unit.power)
                };
                // A number with its unit binds tighter than any operator,
                // but as the base of a power it needs parentheses: Mitcad
                // reads `3 mm^2` as a unit's power.
                match inner.as_ref() {
                    Expr::Number(v) => (
                        format!(
                            "{} {suffix}",
                            mitcad_model::expr::format_number_exact(v * factor)
                        ),
                        POWER,
                    ),
                    other => {
                        let t = text(other, ATOM)?;
                        if factor == 1.0 {
                            (format!("{t} {suffix}"), POWER)
                        } else {
                            (
                                format!(
                                    "{t} * {} {suffix}",
                                    mitcad_model::expr::format_number_exact(factor)
                                ),
                                MUL,
                            )
                        }
                    }
                }
            }
            Expr::Constant(Constant::Pi) => ("PI".to_owned(), ATOM),
            Expr::Constant(Constant::E) => ("E".to_owned(), ATOM),
            Expr::Reference(r) => (self.reference_text(owner, r)?, ATOM),
            Expr::Range(r, _) => return Err(format!("{r}: a range outside a function")),
            Expr::Text(t) => return Err(format!("the text <<{t}>>")),
            Expr::Negate(inner) => (format!("-{}", text(inner, UNARY)?), UNARY),
            Expr::Plus(inner) => self.mitcad_text(owner, inner)?,
            Expr::Binary(op, a, b) => {
                let (level, left, right) = match op {
                    BinaryOp::Add | BinaryOp::Sub => (ADD, ADD, MUL),
                    BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => (MUL, MUL, UNARY),
                    // Mitcad's powers are right-associative.
                    BinaryOp::Pow => (POWER, POWER + 1, UNARY),
                    _ => (COMPARE, COMPARE, ADD),
                };
                let symbol = match op {
                    BinaryOp::Ne => "!=",
                    other => other.symbol(),
                };
                (
                    format!("{} {symbol} {}", text(a, left)?, text(b, right)?),
                    level,
                )
            }
            Expr::Conditional(c, a, b) => (
                format!(
                    "if({}; {}; {})",
                    text(c, COMPARE)?,
                    text(a, COMPARE)?,
                    text(b, COMPARE)?
                ),
                ATOM,
            ),
            Expr::Call(name, args) => self.call_text(owner, name, args)?,
        })
    }

    /// The parameter a reference names (a quantity's `.Value` as a plain
    /// number in FreeCAD's units).
    fn reference_text(&self, owner: &Object, r: &Reference) -> Result<String, String> {
        // `.Value`: the number without its unit.
        let plain = r.path.len() > 1 && r.path.last() == Some(&Component::Name("Value".into()));
        let r = if plain {
            let mut shorter = r.clone();
            shorter.path.pop();
            shorter
        } else {
            r.clone()
        };
        let source = self.source_of(owner, &r)?;
        let name = self.params.names.get(&source).ok_or_else(|| {
            // Why there is none.
            match self.stored(&source) {
                Err(why) => why,
                Ok(_) => format!("{r}: no parameter of it"),
            }
        })?;
        if !plain {
            return Ok(name.clone());
        }
        let unit = self
            .doc
            .parameters()
            .find(name)
            .and_then(|id| self.doc.parameters().get(id))
            .map(|p| p.unit().dims());
        Ok(match unit {
            Some(d) if d == Dims::LENGTH => format!("({name} / 1 mm)"),
            Some(d) if d == Dims::ANGLE => format!("({name} / 1 deg)"),
            _ => name.clone(),
        })
    }

    /// A function call.
    fn call_text(&self, owner: &Object, name: &str, args: &[Expr]) -> Result<(String, u8), String> {
        // The arguments (text, level) with ranges spread into their cells.
        let mut spread: Vec<(String, u8)> = Vec::new();
        for a in args {
            match a {
                Expr::Range(r, to) => {
                    let Some(Component::Name(from)) = r.path.last() else {
                        return Err(format!("{r}: not a range"));
                    };
                    let cells = range_cells(from, to).ok_or_else(|| format!("{r}:{to}"))?;
                    for cell in cells {
                        let mut c = r.clone();
                        c.path.pop();
                        c.path.push(Component::Name(cell));
                        // Empty cells count for nothing.
                        match self.source_of(owner, &c) {
                            Ok(Source::Cell { sheet, address })
                                if self
                                    .fc_document()
                                    .object(&sheet)
                                    .is_some_and(|s| Sheet::of(s).cell(&address).is_none()) => {}
                            _ => spread.push((self.reference_text(owner, &c)?, ATOM)),
                        }
                    }
                }
                other => spread.push(self.mitcad_text(owner, other)?),
            }
        }
        let count = spread.len();
        let wrong = || Err(format!("{name}() with {count} arguments"));
        let at = |k: usize, needed: u8| paren(spread[k].0.clone(), spread[k].1, needed);
        Ok(match (name, count) {
            (
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh"
                | "sqrt" | "abs" | "exp" | "floor" | "ceil" | "round",
                1,
            ) => (format!("{name}({})", spread[0].0), ATOM),
            ("atan2" | "pow", 2) => (format!("{name}({}; {})", spread[0].0, spread[1].0), ATOM),
            ("log", 1) => (format!("ln({})", spread[0].0), ATOM),
            ("log10", 1) => (format!("log({})", spread[0].0), ATOM),
            ("min" | "max", 0) => return wrong(),
            ("min" | "max", 1) => spread[0].clone(),
            ("min" | "max", _) => {
                let all: Vec<&str> = spread.iter().map(|(t, _)| t.as_str()).collect();
                (format!("{name}({})", all.join("; ")), ATOM)
            }
            ("mod", 2) => (format!("{} % {}", at(0, MUL), at(1, UNARY)), MUL),
            ("hypot", 2) => (
                format!("sqrt({}^2 + {}^2)", at(0, POWER + 1), at(1, POWER + 1)),
                ATOM,
            ),
            ("cath", 2) => (
                format!("sqrt({}^2 - {}^2)", at(0, POWER + 1), at(1, POWER + 1)),
                ATOM,
            ),
            ("cbrt", 1) => (
                format!(
                    "if({x} < 0; -abs({x})^(1 / 3); abs({x})^(1 / 3))",
                    x = at(0, ADD)
                ),
                ATOM,
            ),
            ("trunc", 1) => (
                format!("if({x} < 0; ceil({x}); floor({x}))", x = at(0, ADD)),
                ATOM,
            ),
            ("sum" | "average", 0) => return wrong(),
            ("sum", _) => {
                let terms: Vec<String> = (0..count).map(|k| at(k, ADD)).collect();
                (terms.join(" + "), ADD)
            }
            ("average", _) => {
                let terms: Vec<String> = (0..count).map(|k| at(k, ADD)).collect();
                (format!("({}) / {count}", terms.join(" + ")), MUL)
            }
            (
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh"
                | "sqrt" | "abs" | "exp" | "floor" | "ceil" | "round" | "atan2" | "pow" | "log"
                | "log10" | "mod" | "hypot" | "cath" | "cbrt" | "trunc",
                _,
            ) => return wrong(),
            (other, _) => return Err(format!("{other}() has no Mitcad equivalent")),
        })
    }

    /// Translates FreeCAD's expression of `owner` and evaluates it with the
    /// document's parameters: Mitcad's text and its value. With a kind, a
    /// result without a unit gets the kind's unit, and another unit is an
    /// error.
    pub(super) fn translate(
        &self,
        owner: &str,
        freecad: &str,
        kind: Option<Kind>,
    ) -> Result<(String, Quantity), String> {
        let object = self
            .fc_document()
            .object(owner)
            .ok_or_else(|| format!("no object {owner}"))?;
        let parsed = Expr::parse(freecad).map_err(|e| format!("not read: {e}"))?;
        let (mut text, _) = self.mitcad_text(object, &parsed)?;
        let evaluate = |text: &str| -> Result<Quantity, String> {
            let e = mitcad_model::expr::Expr::parse(text).map_err(|e| format!("{text}: {e}"))?;
            e.eval(
                self.doc.parameters().table(),
                &self.doc.parameters().context(),
            )
            .map_err(|e| format!("{text}: {e}"))
        };
        let mut q = evaluate(&text)?;
        if let Some(kind) = kind
            && q.dims != kind.dims()
        {
            if !q.dims.is_dimensionless() {
                return Err(format!(
                    "{text} is not {}",
                    match kind {
                        Kind::Length => "a length",
                        Kind::Angle => "an angle",
                        Kind::Number => "a plain number",
                    }
                ));
            }
            // FreeCAD reads a plain number as millimetres or degrees.
            let unit = if kind == Kind::Length { "mm" } else { "deg" };
            text = if text.parse::<f64>().is_ok() {
                format!("{text} {unit}")
            } else {
                format!("{} * 1 {unit}", wrap(&text))
            };
            q = evaluate(&text)?;
        }
        Ok((text, q))
    }

    // The parameters.

    /// Makes the parameters of the document's quantities (before the
    /// timeline).
    pub(super) fn import_parameters(&mut self) {
        let doc = &self.sources[0].file.document;
        // Each sketch's constraints, read once.
        for o in doc
            .objects
            .iter()
            .filter(|o| super::is_sketch(&o.type_name))
        {
            let list = fc::Sketch::of(o)
                .constraints
                .iter()
                .map(|c| (c.name.clone(), c.kind, c.value))
                .collect();
            self.params.constraints.insert(o.name.clone(), list);
        }
        // The bindings.
        let mut bindings = Vec::new();
        for o in &doc.objects {
            let Some(FcValue::Expressions(list)) = o.value("ExpressionEngine") else {
                continue;
            };
            for e in list {
                let target = Expr::parse(&e.path).ok().and_then(|p| match p {
                    Expr::Reference(mut r) => {
                        r.relative = true;
                        self.source_of(o, &r).ok()
                    }
                    _ => None,
                });
                bindings.push((o.name.clone(), e.clone(), target));
            }
        }
        let mut wanted: BTreeSet<Source> = BTreeSet::new();
        let mut formulas: Vec<(String, String)> = Vec::new();
        for o in doc.objects.iter().filter(|o| is_sheet(&o.type_name)) {
            for cell in Sheet::of(o).cells {
                if let Content::Expression(e) = cell.content() {
                    formulas.push((o.name.clone(), e.to_owned()));
                }
                if cell.alias.is_some() && !matches!(cell.content(), Content::Text(_)) {
                    wanted.insert(Source::Cell {
                        sheet: o.name.clone(),
                        address: cell.address.clone(),
                    });
                }
            }
        }
        // VarSets' numbers, named constraints.
        for o in &doc.objects {
            if o.type_name == "App::VarSet" {
                for p in &o.properties {
                    if p.group.is_some() && property_kind(&p.type_name).is_some() {
                        wanted.insert(Source::property(&o.name, &p.name));
                    }
                }
            }
        }
        for (sketch, list) in &self.params.constraints {
            for (index, (name, kind, _)) in list.iter().enumerate() {
                if !name.is_empty() && constraint_kind(*kind).is_some() {
                    wanted.insert(Source::Constraint {
                        sketch: sketch.clone(),
                        index,
                    });
                }
            }
        }
        for (owner, e, target) in bindings {
            let row = self.report.expressions.len();
            self.report.expressions.push(ExpressionReport {
                object: owner.clone(),
                path: e.path.clone(),
                expression: e.expression.clone(),
                mitcad: None,
                outcome: ExpressionOutcome::Unused,
                note: None,
            });
            let Some(source) = target else {
                self.report.expressions[row].note =
                    Some("its property path is not read".to_owned());
                continue;
            };
            // What the expressions of what the import carries over refer
            // to (also only among those: the expressions of objects it
            // leaves out, an architecture model's, would make parameters
            // that drive nothing).
            if self.relevant(&owner) && self.relevant(source.object()) {
                formulas.push((owner.clone(), e.expression.clone()));
            }
            self.params.bindings.insert(
                source,
                Binding {
                    owner,
                    text: e.expression,
                    row,
                },
            );
        }
        for (owner, text) in &formulas {
            let referenced = self.referenced(owner, text);
            wanted.extend(referenced.into_iter().filter(|s| self.relevant(s.object())));
        }
        // Each one's definition (the expressions that name one that has
        // none say why).
        let list: Vec<Wanted> = wanted
            .iter()
            .filter_map(|source| self.wanted(source).ok())
            .collect();
        let names = self.parameter_names(&list);
        // In the order of their dependencies; the rest (cycles) as values.
        let mut done: HashSet<Source> = HashSet::new();
        let mut left: Vec<usize> = (0..list.len()).collect();
        loop {
            let ready: Vec<usize> = left
                .iter()
                .copied()
                .filter(|&i| {
                    list[i]
                        .deps
                        .iter()
                        .all(|d| done.contains(d) || !list.iter().any(|w| w.source == *d))
                })
                .collect();
            if ready.is_empty() {
                break;
            }
            for i in ready {
                self.make_parameter(&list[i], &names[i], false);
                done.insert(list[i].source.clone());
                left.retain(|&j| j != i);
            }
        }
        for i in left {
            self.make_parameter(&list[i], &names[i], true);
        }
    }

    /// The quantities an expression of `owner` refers to.
    fn referenced(&self, owner: &str, text: &str) -> Vec<Source> {
        let Some(o) = self.fc_document().object(owner) else {
            return Vec::new();
        };
        let Ok(parsed) = Expr::parse(text) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in parsed.references() {
            let mut r = r.clone();
            if r.path.len() > 1 && r.path.last() == Some(&Component::Name("Value".into())) {
                r.path.pop();
            }
            if let Ok(s) = self.source_of(o, &r) {
                out.push(s);
            }
        }
        // Ranges' cells.
        let mut stack = vec![&parsed];
        while let Some(e) = stack.pop() {
            match e {
                Expr::Call(_, args) => {
                    for a in args {
                        if let Expr::Range(r, to) = a
                            && let Some(Component::Name(from)) = r.path.last()
                            && let Some(cells) = range_cells(from, to)
                        {
                            for cell in cells {
                                let mut c = r.clone();
                                c.path.pop();
                                c.path.push(Component::Name(cell));
                                if let Ok(s @ Source::Cell { .. }) = self.source_of(o, &c)
                                    && self.stored(&s).is_ok()
                                {
                                    out.push(s);
                                }
                            }
                        }
                        stack.push(a);
                    }
                }
                Expr::WithUnit(x, _) | Expr::Negate(x) | Expr::Plus(x) => stack.push(x),
                Expr::Binary(_, a, b) => {
                    stack.push(a);
                    stack.push(b);
                }
                Expr::Conditional(c, a, b) => {
                    stack.push(c);
                    stack.push(a);
                    stack.push(b);
                }
                _ => {}
            }
        }
        out.retain(|s| !matches!(s, Source::Cell { .. }) || self.stored(s).is_ok());
        out
    }

    /// A quantity's definition: its name, kind, expression, value and
    /// dependencies.
    fn wanted(&self, source: &Source) -> Result<Wanted, String> {
        let doc = self.fc_document();
        let (kind, stored) = self.stored(source)?;
        let label = |name: &str| doc.object(name).map_or(name, Object::label).to_owned();
        let owner = label(source.object());
        // A name of its own (and the longer one), or one with its owner's
        // label already.
        let own = |name: &str| (name.to_owned(), Some(format!("{owner}_{name}")));
        let bound = || {
            self.params
                .bindings
                .get(source)
                .map(|b| (b.owner.clone(), b.text.clone()))
        };
        let ((name, longer), expression, describe) = match source {
            Source::Cell { sheet, address } => {
                let cell = doc
                    .object(sheet)
                    .map(Sheet::of)
                    .and_then(|s| s.cell(address).cloned())
                    .ok_or_else(|| format!("{source} is empty"))?;
                let expression = match cell.content() {
                    Content::Expression(e) => Some((sheet.clone(), e.to_owned())),
                    _ => None,
                };
                match &cell.alias {
                    Some(a) => (own(a), expression, format!("{owner} {address} ({a})")),
                    None => (
                        (format!("{owner}_{address}"), None),
                        expression,
                        format!("{owner} {address}"),
                    ),
                }
            }
            Source::Property { object, path } => {
                let o = doc
                    .object(object)
                    .ok_or_else(|| format!("{source}: no object"))?;
                let name = if o.type_name == "App::VarSet" && !path.contains('.') {
                    own(path)
                } else {
                    (format!("{owner}_{}", path.replace('.', "_")), None)
                };
                (name, bound(), format!("{owner} {path}"))
            }
            Source::Constraint { sketch, index } => {
                let constraint_name = self
                    .params
                    .constraints
                    .get(sketch)
                    .and_then(|list| list.get(*index))
                    .map(|(name, _, _)| name.clone())
                    .unwrap_or_default();
                if constraint_name.is_empty() {
                    (
                        (format!("{owner}_c{index}"), None),
                        bound(),
                        format!("{owner} constraint {index}"),
                    )
                } else {
                    (
                        own(&constraint_name),
                        bound(),
                        format!("{owner} constraint {constraint_name}"),
                    )
                }
            }
        };
        let deps = expression
            .as_ref()
            .map(|(owner, text)| self.referenced(owner, text))
            .unwrap_or_default();
        Ok(Wanted {
            source: source.clone(),
            name,
            longer,
            kind,
            expression,
            stored,
            deps,
            describe,
        })
    }

    /// Mitcad names for the quantities: their own where Mitcad can take
    /// it and no other quantity wants it, else with the owner's label in
    /// front; unique among the document's parameters.
    fn parameter_names(&self, list: &[Wanted]) -> Vec<String> {
        let mut count: BTreeMap<&str, usize> = BTreeMap::new();
        for w in list {
            *count.entry(w.name.as_str()).or_default() += 1;
        }
        let mut taken: BTreeSet<String> = self
            .doc
            .parameters()
            .iter()
            .map(|p| p.name().to_owned())
            .collect();
        let mut out = Vec::new();
        for w in list {
            let own = valid(&w.name).filter(|n| count[n.as_str()] == 1 && !taken.contains(n));
            let name = own.unwrap_or_else(|| {
                let longer = w.longer.as_deref().unwrap_or(&w.name);
                crate::params::free_name(longer, &taken)
            });
            taken.insert(name.clone());
            out.push(name);
        }
        out
    }

    /// Whether the import carries an object's values over (its features,
    /// sketches, datums, spreadsheets and VarSets): only expressions of
    /// these, and only their properties, make parameters.
    fn relevant(&self, object: &str) -> bool {
        self.fc_document().object(object).is_some_and(|o| {
            is_sheet(&o.type_name)
                || o.type_name == "App::VarSet"
                || super::is_sketch(&o.type_name)
                || self.replay.replayed.contains(object)
        })
    }

    /// Makes one parameter (`literal`: of FreeCAD's value only).
    fn make_parameter(&mut self, w: &Wanted, name: &str, literal: bool) {
        let mut row = ParameterReport {
            name: name.to_owned(),
            source: w.source.to_string(),
            object: w.source.object().to_owned(),
            describe: w.describe.clone(),
            freecad: w.expression.as_ref().map(|(_, t)| t.clone()),
            freecad_value: w.stored,
            ..ParameterReport::default()
        };
        match &w.source {
            Source::Cell { address, .. } => row.cell = Some(address.clone()),
            Source::Property { path, .. } => row.property = Some(path.clone()),
            Source::Constraint { index, .. } => row.constraint = Some(*index),
        }
        let context = self.doc.parameters().context();
        let unit_of = |kind: Kind| match kind {
            Kind::Length => Unit::of_length(context.default_length_unit),
            Kind::Angle => Unit::DEG,
            Kind::Number => Unit::NONE,
        };
        // The translated expression, else FreeCAD's value.
        let integer = self.integer_property(&w.source);
        let translated = match (&w.expression, literal) {
            (Some(_), true) => Err("its references form a cycle".to_owned()),
            (Some((owner, text)), false) => {
                self.translate(owner, text, w.kind)
                    .map(|(text, q)| match w.stored {
                        Some(v) => rounded(text, q, v, integer),
                        None => (text, q),
                    })
            }
            (None, _) => Err(String::new()),
        };
        let (text, unit, outcome, note) = match translated {
            Ok((text, q)) => {
                let kind = w.kind.or_else(|| Kind::of_dims(q.dims));
                let unit = match kind {
                    Some(k) => unit_of(k),
                    None => match Unit::display_for(q.dims) {
                        Some(u) => u,
                        None => {
                            row.outcome = ExpressionOutcome::Skipped;
                            row.note = Some(format!("{text}: a unit Mitcad has not"));
                            self.report.parameters.push(row);
                            return;
                        }
                    },
                };
                match w.stored {
                    Some(v) if !close(q.value, v) => (
                        value_to_expression(v, unit),
                        unit,
                        ExpressionOutcome::Mismatched,
                        Some(format!("{text} gives {} where FreeCAD has {}", q.value, v)),
                    ),
                    _ => (text, unit, ExpressionOutcome::Parameter, None),
                }
            }
            Err(why) => match (w.stored, w.kind) {
                (Some(v), Some(kind)) => {
                    let unit = unit_of(kind);
                    let note = (!why.is_empty()).then_some(why);
                    (
                        value_to_expression(v, unit),
                        unit,
                        ExpressionOutcome::Value,
                        note,
                    )
                }
                _ => {
                    row.outcome = ExpressionOutcome::Skipped;
                    row.note = Some(if why.is_empty() {
                        "no value".to_owned()
                    } else {
                        why
                    });
                    self.report.parameters.push(row);
                    return;
                }
            },
        };
        let comment = format!("FreeCAD: {}", w.describe);
        if let Err(e) = self
            .doc
            .add_parameter_expression(name, &text, Some(unit), &comment)
        {
            row.outcome = ExpressionOutcome::Skipped;
            row.note = Some(format!("{text}: {e}"));
            self.report.parameters.push(row);
            return;
        }
        row.expression = text.clone();
        row.outcome = outcome;
        row.note = note.clone();
        self.params.names.insert(w.source.clone(), name.to_owned());
        self.report.parameters.push(row);
        if let Some(b) = self.params.bindings.get(&w.source) {
            let r = &mut self.report.expressions[b.row];
            r.mitcad = Some(name.to_owned());
            r.outcome = outcome;
            r.note = note;
        }
    }

    // Values of definitions.

    /// The value of a quantity for a definition: its parameter, else its
    /// bound expression translated (checked against FreeCAD's `stored`
    /// value, Mitcad's units), else the number.
    pub(super) fn quantity(&mut self, source: Source, kind: Kind, stored: f64) -> Q {
        if let Some(q) = self.params.quantities.get(&source) {
            return q.clone();
        }
        let q = if let Some(name) = self.params.names.get(&source).cloned() {
            match self.translate_name(&name, kind) {
                Some(text) => Q {
                    value: stored,
                    kind,
                    text: Some(text),
                },
                None => Q::number(stored, kind),
            }
        } else if let Some(b) = self.params.bindings.get(&source).cloned() {
            let integer = self.integer_property(&source);
            let translated = self
                .translate(&b.owner, &b.text, Some(kind))
                .map(|(text, value)| rounded(text, value, stored, integer));
            let (q, outcome, mitcad, note) = match translated {
                Ok((text, value)) if close(value.value, stored) => (
                    Q {
                        value: stored,
                        kind,
                        text: Some(text.clone()),
                    },
                    ExpressionOutcome::Expression,
                    Some(text),
                    None,
                ),
                Ok((text, value)) => (
                    Q::number(stored, kind),
                    ExpressionOutcome::Mismatched,
                    Some(text.clone()),
                    Some(format!(
                        "{text} gives {} where FreeCAD has {stored}",
                        value.value
                    )),
                ),
                Err(why) => (
                    Q::number(stored, kind),
                    ExpressionOutcome::Value,
                    None,
                    Some(why),
                ),
            };
            let r = &mut self.report.expressions[b.row];
            r.outcome = outcome;
            r.mitcad = mitcad;
            r.note = note;
            q
        } else {
            Q::number(stored, kind)
        };
        self.params.quantities.insert(source, q.clone());
        q
    }

    /// Whether a quantity is an integer property's (FreeCAD rounds what an
    /// expression gives it).
    fn integer_property(&self, source: &Source) -> bool {
        let Source::Property { object, path } = source else {
            return false;
        };
        self.fc_document()
            .object(object)
            .and_then(|o| o.property(path))
            .is_some_and(|p| {
                matches!(
                    p.type_name.as_str(),
                    "App::PropertyInteger" | "App::PropertyIntegerConstraint"
                )
            })
    }

    /// A parameter by name as a value of a kind (a plain number's
    /// parameter in an angle gets degrees).
    fn translate_name(&self, name: &str, kind: Kind) -> Option<String> {
        let p = self
            .doc
            .parameters()
            .find(name)
            .and_then(|id| self.doc.parameters().get(id))?;
        let dims = p.unit().dims();
        if dims == kind.dims() || (dims.is_dimensionless() && kind == Kind::Length) {
            Some(name.to_owned())
        } else if dims.is_dimensionless() {
            Some(format!("{name} * 1 deg"))
        } else {
            None
        }
    }

    /// A property's value: `default` (FreeCAD's units) when it has none.
    pub(super) fn prop(&mut self, o: &Object, property: &str, kind: Kind, default: f64) -> Q {
        let stored = kind.canonical(o.f64(property).unwrap_or(default));
        self.quantity(Source::property(&o.name, property), kind, stored)
    }

    /// The values of a sketch's driving dimensions: the expressions of the
    /// constraints that have parameters or bound expressions (the
    /// translator's value is FreeCAD's, or its opposite, or for angles its
    /// supplement). Returns the parameters used as they are (to adopt).
    pub(super) fn dimension_values(
        &mut self,
        sketch: &Object,
        t: &mut super::sketch::Translated,
    ) -> Vec<String> {
        let mut adopt = Vec::new();
        for k in 0..t.sources.len() {
            let s = t.sources[k].clone();
            let Some(kind) = self
                .params
                .constraints
                .get(&sketch.name)
                .and_then(|list| list.get(s.index))
                .and_then(|(_, c, _)| constraint_kind(*c))
            else {
                continue;
            };
            let source = Source::Constraint {
                sketch: sketch.name.clone(),
                index: s.index,
            };
            let q = self.quantity(source.clone(), kind, s.value);
            if q.text.is_none() {
                continue;
            }
            let Some(value) = related(&q, s.mitcad) else {
                continue;
            };
            let Some(text) = value.text else {
                continue;
            };
            let Some(dimension) = t
                .dimensions
                .iter_mut()
                .find(|d| d["id"].as_str() == Some(s.dimension.as_str()))
            else {
                continue;
            };
            dimension["value"] = json!(text);
            if self.params.names.get(&source) == Some(&text) {
                adopt.push(text.clone());
            }
            t.sources[k].parameter = Some(text);
        }
        adopt
    }

    /// Adopts the parameters made of an object's properties that its
    /// Mitcad features use as they are.
    pub(super) fn adopt_properties(&mut self, object: &str, features: &[FeatureUid]) {
        let names: Vec<String> = self
            .params
            .names
            .iter()
            .filter(|(s, _)| matches!(s, Source::Property { object: o, .. } if o == object))
            .map(|(_, n)| n.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        for uid in features {
            let Some(f) = self.doc.feature(*uid) else {
                continue;
            };
            let used: Vec<String> = f
                .def
                .params()
                .into_iter()
                .map(|id| self.doc.parameters().name(id))
                .filter(|n| names.contains(n))
                .collect();
            if !used.is_empty() {
                let _ = self.doc.adopt_parameters(*uid, &used);
            }
        }
    }

    /// Changes parameters after the import (a test against FreeCAD's own
    /// change of them).
    pub(super) fn set_parameters(&mut self, changes: &[(String, String)]) {
        for (name, expression) in changes {
            match self.doc.set_parameter_expression(name, expression, None) {
                Ok(_) => self.params.changed = true,
                Err(e) => self.warn(format!("{name} = {expression}: {e}")),
            }
        }
    }

    /// The parameters' values now, and why bindings kept their values.
    pub(super) fn finish_parameters(&mut self) {
        for row in &mut self.report.parameters {
            if let Some(p) = self
                .doc
                .parameters()
                .find(&row.name)
                .and_then(|id| self.doc.parameters().get(id))
                .filter(|_| row.outcome != ExpressionOutcome::Skipped)
            {
                row.value = Some(p.value());
                row.expression = p.expression().to_owned();
            }
        }
        let outcomes: HashMap<&str, super::ObjectOutcome> = self
            .report
            .features
            .iter()
            .map(|f| (f.object.as_str(), f.outcome))
            .chain(
                self.report
                    .sketches
                    .iter()
                    .map(|s| (s.object.as_str(), s.outcome)),
            )
            .collect();
        for r in &mut self.report.expressions {
            let fell_back = outcomes.get(r.object.as_str()).is_some_and(|o| {
                matches!(
                    o,
                    super::ObjectOutcome::Fallback | super::ObjectOutcome::Skipped
                )
            });
            if fell_back
                && matches!(
                    r.outcome,
                    ExpressionOutcome::Expression | ExpressionOutcome::Unused
                )
            {
                r.outcome = ExpressionOutcome::Value;
                r.note = Some(format!("{} was not replayed", r.object));
            } else if r.outcome == ExpressionOutcome::Unused && r.note.is_none() {
                r.note = Some("not a value the import carries over".to_owned());
            }
        }
    }

    /// A bound property's expression that its definition could not use
    /// after all: FreeCAD's value, and why.
    pub(super) fn left_out(&mut self, o: &Object, path: &str, why: &str) {
        let source = Source::property(&o.name, path);
        if let Some(b) = self.params.bindings.get(&source)
            && self.report.expressions[b.row].outcome == ExpressionOutcome::Expression
        {
            let r = &mut self.report.expressions[b.row];
            r.outcome = ExpressionOutcome::Value;
            r.note = Some(why.to_owned());
        }
    }

    /// A bound property's expression that the import cannot carry over
    /// (it drives nothing Mitcad has): FreeCAD's value, and why.
    pub(super) fn kept_value(&mut self, o: &Object, path: &str, why: &str) {
        let source = Source::property(&o.name, path);
        if let Some(b) = self.params.bindings.get(&source)
            && self.report.expressions[b.row].outcome == ExpressionOutcome::Unused
        {
            let r = &mut self.report.expressions[b.row];
            r.outcome = ExpressionOutcome::Value;
            r.note = Some(why.to_owned());
        }
    }

    /// A sketch's or a datum plane's attachment offset that turns it about
    /// its support's x axis (0) or y axis (1), without moving it, by an
    /// angle an expression drives: the axis and the angle about its
    /// positive direction (FreeCAD's value and expression).
    pub(super) fn tilt_quantity(&mut self, o: &Object) -> Option<(usize, Q)> {
        let path = "AttachmentOffset.Rotation.Angle";
        let source = Source::property(&o.name, path);
        if !self.params.names.contains_key(&source) && !self.params.bindings.contains_key(&source) {
            return None;
        }
        let Some(FcValue::Placement(p)) = o.value("AttachmentOffset") else {
            return None;
        };
        let (Some(axis), angle) = turn_of(p) else {
            return None;
        };
        let which = (0..2).find(|&k| (axis[k].abs() - 1.0).abs() < 1e-9);
        let (Some(which), true) = (which, p.position.iter().all(|v| v.abs() < 1e-9)) else {
            self.kept_value(
                o,
                path,
                "a turn of the attachment offset about another axis than the support's x or y axis, or with a shift: Mitcad's planes turn about lines",
            );
            return None;
        };
        let q = self.quantity(source, Kind::Angle, angle);
        q.text
            .is_some()
            .then(|| (which, q.signed(axis[which].signum())))
    }

    /// Whether an expression (or a parameter's name) drives an object's
    /// property path.
    pub(super) fn is_bound(&self, o: &Object, path: &str) -> bool {
        let source = Source::property(&o.name, path);
        self.params.names.contains_key(&source) || self.params.bindings.contains_key(&source)
    }

    /// The source of a sketch's or a datum's bound attachment offset along
    /// its normal, with FreeCAD's value, when an expression drives it.
    pub(super) fn offset_quantity(&mut self, o: &Object) -> Option<Q> {
        self.position_quantity(o, 2)
    }

    /// A sketch's or a datum's attachment offset along its support's x, y
    /// or z axis (`axis` 0, 1, 2), with FreeCAD's value, when an expression
    /// drives it.
    pub(super) fn position_quantity(&mut self, o: &Object, axis: usize) -> Option<Q> {
        let name = ["x", "y", "z"].get(axis)?;
        let source = Source::property(&o.name, &format!("AttachmentOffset.Base.{name}"));
        if !self.params.names.contains_key(&source) && !self.params.bindings.contains_key(&source) {
            return None;
        }
        let value = match o.value("AttachmentOffset") {
            Some(FcValue::Placement(p)) => p.position[axis],
            _ => return None,
        };
        let q = self.quantity(source, Kind::Length, value);
        q.text.is_some().then_some(q)
    }
}

/// A dimension's value made of FreeCAD's constraint's: the same, its
/// opposite (lengths: `abs`), or for angles a turn of it (the translator's
/// angle between the lines' directions); None when none of these is.
fn related(q: &Q, mitcad: f64) -> Option<Q> {
    let v = q.value;
    let tol = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    if tol(mitcad, v) {
        return Some(q.clone());
    }
    match q.kind {
        Kind::Angle => {
            let pi = std::f64::consts::PI;
            for turns in [0, 1, -1, 2, -2, 3, -3, 4, -4] {
                for sign in [1.0, -1.0] {
                    if !tol(mitcad, sign * v + f64::from(turns) * pi) {
                        continue;
                    }
                    let s = q.signed(sign);
                    if turns == 0 {
                        return Some(Q { value: mitcad, ..s });
                    }
                    let degrees = f64::from(turns) * 180.0;
                    return Some(Q {
                        value: mitcad,
                        kind: Kind::Angle,
                        text: s.text.map(|t| match t.strip_prefix('-') {
                            Some(rest) => format!("{degrees} deg - {rest}"),
                            None => format!("{degrees} deg + {}", wrap(&t)),
                        }),
                    });
                }
            }
            None
        }
        _ if tol(mitcad, -v) => Some(Q {
            value: mitcad,
            kind: q.kind,
            text: q.text.as_ref().map(|t| format!("abs({t})")),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_compound_expressions() {
        assert_eq!(wrap("Width"), "Width");
        assert_eq!(wrap("12.5"), "12.5");
        assert_eq!(wrap("sqrt(a + b)"), "sqrt(a + b)");
        assert_eq!(wrap("a + b"), "(a + b)");
        assert_eq!(wrap("(a) + (b)"), "((a) + (b))");
        assert_eq!(wrap("f(a) + g(b)"), "(f(a) + g(b))");
        assert_eq!(wrap("30 mm"), "30 mm");
        assert_eq!(wrap("a mm"), "(a mm)");
    }

    #[test]
    fn values_follow_their_expressions() {
        let q = Q {
            value: 10.0,
            kind: Kind::Length,
            text: Some("Width".into()),
        };
        assert_eq!(q.neg().text.as_deref(), Some("-Width"));
        assert_eq!(q.scale(0.5).text.as_deref(), Some("Width / 2"));
        assert_eq!(q.scale(0.5).value, 5.0);
        let n = Q::number(2.0, Kind::Length);
        let sum = Q::combine(&[&q, &n], 8.0, Kind::Length, |t| {
            format!("{} - {}", t[0], t[1])
        });
        assert_eq!(sum.text.as_deref(), Some("Width - 2 mm"));
        assert_eq!(
            Q::combine(&[&n], 2.0, Kind::Length, |t| t[0].clone()).text,
            None
        );
    }

    #[test]
    fn dimensions_relate_to_constraints() {
        let pi = std::f64::consts::PI;
        let q = Q {
            value: -0.5,
            kind: Kind::Angle,
            text: Some("a".into()),
        };
        assert_eq!(related(&q, 0.5).unwrap().text.as_deref(), Some("-a"));
        assert_eq!(
            related(&q, pi - 0.5).unwrap().text.as_deref(),
            Some("180 deg + a")
        );
        let l = Q {
            value: -3.0,
            kind: Kind::Length,
            text: Some("x".into()),
        };
        assert_eq!(related(&l, 3.0).unwrap().text.as_deref(), Some("abs(x)"));
        assert!(related(&l, 4.0).is_none());
    }
}
