// SPDX-License-Identifier: MIT
//! Typed property sets (mitcad#496): key/value properties in named sets
//! (`Identity / Mark = W-12`, `Pset_WallCommon / FireRating = EI60`) on
//! three levels: a component definition (shared by its occurrences), an
//! occurrence (its own values, which may override the definition's) and a
//! body.
//!
//! Every value has a type: one of the standard types ([`BaseType`]: text,
//! integer, number, length, area, volume, mass, angle, yes/no, date) or an
//! own type the design defines ([`PropertyType`]: a name, a base type,
//! optional allowed values, an optional unit and default, a description).
//! Quantities are kept in canonical units (mm, mm², mm³, kg, radians) and
//! shown in the design's units or the own type's unit.
//!
//! Every value records its source ([`Source`]): the user, an import
//! (`rvt`, `ifc`, `ipt`, `f3d`), a library part. An imported value the
//! user changed keeps the value its source gave (`imported`), so the
//! design shows that it differs. Computed values (volume, area, mass, a
//! BUILD member's length) and the values a library part gives are never
//! stored: the document works them out when they are asked for
//! (`document/properties.rs`), so a recompute never changes the file.
//!
//! The store is part of the definition state: its commands are undo steps
//! that recompute nothing, and the design file keeps it
//! (`property_types`, `properties`). Each owner's sets are shared between
//! undo steps until changed.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::expr::{EvalContext, Unit, format_number, with_decimal_points};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid, string_serde};

/// The set of the values the document works out (volume, area, mass, a
/// BUILD member's length): not stored, so no stored property may use it.
pub const COMPUTED_SET: &str = "Computed";

/// The set of the values a library part gives (designation, supplier, part
/// number, material): not stored; a stored value of the same key
/// overrides one.
pub const LIBRARY_SET: &str = "Library";

/// What a property belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyOwner {
    /// A component definition (`C2`; `C0` the root): shared by its
    /// occurrences.
    Component(ComponentUid),
    /// One occurrence (`O3`): its own values, over its component's.
    Occurrence(OccurrenceUid),
    /// A body (`F7.b0`).
    Body(BodyUid),
}

/// The level a property is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Component,
    Occurrence,
    Body,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Occurrence => "occurrence",
            Self::Body => "body",
        }
    }
}

impl PropertyOwner {
    pub fn level(self) -> Level {
        match self {
            Self::Component(_) => Level::Component,
            Self::Occurrence(_) => Level::Occurrence,
            Self::Body(_) => Level::Body,
        }
    }
}

impl fmt::Display for PropertyOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Component(uid) => uid.fmt(f),
            Self::Occurrence(uid) => uid.fmt(f),
            Self::Body(uid) => uid.fmt(f),
        }
    }
}

impl FromStr for PropertyOwner {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if let Ok(uid) = text.parse::<BodyUid>() {
            return Ok(Self::Body(uid));
        }
        if let Ok(uid) = text.parse::<ComponentUid>() {
            return Ok(Self::Component(uid));
        }
        if let Ok(uid) = text.parse::<OccurrenceUid>() {
            return Ok(Self::Occurrence(uid));
        }
        Err(format!(
            "“{text}” is no component (C2), occurrence (O3) or body (F7.b0)"
        ))
    }
}

string_serde!(
    PropertyOwner,
    "a component, occurrence or body id like \"C2\", \"O3\" or \"F7.b0\""
);

/// A standard type: what a value is and how it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseType {
    Text,
    Integer,
    /// A plain number, optionally with a unit label of its own (no
    /// conversion).
    Number,
    /// mm.
    Length,
    /// mm².
    Area,
    /// mm³.
    Volume,
    /// kg.
    Mass,
    /// Radians.
    Angle,
    /// Yes or no.
    Boolean,
    /// `YYYY-MM-DD`.
    Date,
}

impl BaseType {
    pub const ALL: [Self; 10] = [
        Self::Text,
        Self::Integer,
        Self::Number,
        Self::Length,
        Self::Area,
        Self::Volume,
        Self::Mass,
        Self::Angle,
        Self::Boolean,
        Self::Date,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Length => "length",
            Self::Area => "area",
            Self::Volume => "volume",
            Self::Mass => "mass",
            Self::Angle => "angle",
            Self::Boolean => "boolean",
            Self::Date => "date",
        }
    }

    /// A standard type by name (case-insensitive; `yes/no` and `bool` are
    /// `boolean`).
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        match name.as_str() {
            "yes/no" | "bool" => return Some(Self::Boolean),
            _ => {}
        }
        Self::ALL.into_iter().find(|t| t.as_str() == name)
    }

    /// A quantity kept in canonical units (shown in the design's).
    pub fn is_quantity(self) -> bool {
        matches!(
            self,
            Self::Length | Self::Area | Self::Volume | Self::Mass | Self::Angle
        )
    }

    /// A type whose values are ordered numbers.
    pub fn is_numeric(self) -> bool {
        self.is_quantity() || matches!(self, Self::Integer | Self::Number)
    }
}

/// Where a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    User,
    Rvt,
    Ifc,
    Ipt,
    F3d,
    /// A BUILD or library part's data.
    Library,
    /// Worked out from the model; never stored.
    Computed,
    /// A slicer's 3MF project or preset file (print settings, mitcad#640).
    #[serde(rename = "3mf")]
    ThreeMf,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Rvt => "rvt",
            Self::Ifc => "ifc",
            Self::Ipt => "ipt",
            Self::F3d => "f3d",
            Self::Library => "library",
            Self::Computed => "computed",
            Self::ThreeMf => "3mf",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::User,
            Self::Rvt,
            Self::Ifc,
            Self::Ipt,
            Self::F3d,
            Self::Library,
            Self::Computed,
            Self::ThreeMf,
        ]
        .into_iter()
        .find(|s| s.as_str() == name)
    }
}

fn user() -> Source {
    Source::User
}

fn is_user(source: &Source) -> bool {
    *source == Source::User
}

/// A stored value: what its type makes of it (canonical units for
/// quantities, `YYYY-MM-DD` text for dates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Text(String),
}

impl PropertyValue {
    /// The value as JSON.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Boolean(b) => json!(b),
            Self::Integer(i) => json!(i),
            Self::Number(n) => json!(n),
            Self::Text(t) => json!(t),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(i) => Some(*i as f64),
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The same value as `other` (numbers within a relative 1e-9).
    pub fn same(&self, other: &Self) -> bool {
        match (self.as_f64(), other.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0),
            _ => self == other,
        }
    }

    /// The value as its base type keeps it (an integral number of a file
    /// read as an integer, and so on), or why it does not fit.
    pub fn conform(self, base: BaseType) -> Result<Self, String> {
        let wrong = |value: &Self| {
            format!(
                "{} is not a value of type {}",
                value.to_json(),
                base.as_str()
            )
        };
        Ok(match (base, self) {
            (BaseType::Text, Self::Text(t)) => Self::Text(t),
            (BaseType::Date, Self::Text(t)) => Self::Text(check_date(&t)?),
            (BaseType::Boolean, Self::Boolean(b)) => Self::Boolean(b),
            (BaseType::Integer, Self::Integer(i)) => Self::Integer(i),
            (BaseType::Integer, Self::Number(n)) if n.fract() == 0.0 && n.abs() < 9e15 => {
                Self::Integer(n as i64)
            }
            (b, Self::Integer(i)) if b == BaseType::Number || b.is_quantity() => {
                Self::Number(i as f64)
            }
            (b, Self::Number(n)) if (b == BaseType::Number || b.is_quantity()) && n.is_finite() => {
                Self::Number(n)
            }
            (_, value) => return Err(wrong(&value)),
        })
    }
}

/// A unit of mass: its symbols and its size in kg.
const MASS_UNITS: [(&str, f64); 6] = [
    ("kg", 1.0),
    ("g", 1e-3),
    ("mg", 1e-6),
    ("t", 1e3),
    ("lb", 0.453_592_37),
    ("oz", 0.028_349_523_125),
];

/// How a quantity is shown and what a number without a unit means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayUnit {
    /// Lengths, areas, volumes and angles.
    Geometric(Unit),
    /// A unit of mass by its symbol and its size in kg.
    Mass(&'static str, f64),
}

impl DisplayUnit {
    /// The unit of a quantity type: the own type's `unit` when it has one,
    /// else the design's length or angle unit (masses in kg).
    pub fn of(
        base: BaseType,
        unit: Option<&str>,
        context: &EvalContext,
    ) -> Result<Option<Self>, String> {
        if !base.is_quantity() {
            return Ok(None);
        }
        if let Some(text) = unit.map(str::trim).filter(|u| !u.is_empty()) {
            return Self::parse(base, text).map(Some);
        }
        let length = Unit::of_length(context.default_length_unit);
        Ok(Some(match base {
            BaseType::Length => Self::Geometric(length),
            BaseType::Area => Self::Geometric(length.powi(2).expect("a square unit")),
            BaseType::Volume => Self::Geometric(length.powi(3).expect("a cubic unit")),
            BaseType::Angle => Self::Geometric(Unit::of_angle(context.default_angle_unit)),
            _ => Self::Mass("kg", 1.0),
        }))
    }

    /// A unit given as text for a quantity type (`m`, `mm^2`, `deg`, `g`).
    pub fn parse(base: BaseType, text: &str) -> Result<Self, String> {
        if base == BaseType::Mass {
            return MASS_UNITS
                .iter()
                .find(|(symbol, _)| *symbol == text)
                .map(|&(symbol, kg)| Self::Mass(symbol, kg))
                .ok_or_else(|| format!("“{text}” is not a unit of mass (kg, g, mg, t, lb, oz)"));
        }
        let unit = Unit::parse(text).map_err(|e| format!("unit “{text}”: {e}"))?;
        // `Text` is a text parameter's unit (mitcad#664), no unit of measure.
        if unit.is_text() || unit.dims() != dims_of(base) {
            return Err(format!("“{text}” is not a unit of {}", base.as_str()));
        }
        Ok(Self::Geometric(unit))
    }

    pub fn symbol(self) -> String {
        match self {
            Self::Geometric(unit) => unit.to_string(),
            Self::Mass(symbol, _) => symbol.to_owned(),
        }
    }

    pub fn to_canonical(self, value: f64) -> f64 {
        match self {
            Self::Geometric(unit) => unit.to_canonical(value),
            Self::Mass(_, kg) => value * kg,
        }
    }

    pub fn from_canonical(self, canonical: f64) -> f64 {
        match self {
            Self::Geometric(unit) => unit.canonical_to_unit(canonical),
            Self::Mass(_, kg) => canonical / kg,
        }
    }
}

fn dims_of(base: BaseType) -> crate::expr::Dims {
    use crate::expr::Dims;
    match base {
        BaseType::Length => Dims::LENGTH,
        BaseType::Area => Dims::AREA,
        BaseType::Volume => Dims::VOLUME,
        BaseType::Angle => Dims::ANGLE,
        _ => Dims::NONE,
    }
}

/// `text` as a date `YYYY-MM-DD`, or why it is not one.
fn check_date(text: &str) -> Result<String, String> {
    let text = text.trim();
    let error = || format!("“{text}” is not a date (YYYY-MM-DD)");
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(error());
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return Err(error());
    }
    let number = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (Some(y), Some(m), Some(d)) = (number(year), number(month), number(day)) else {
        return Err(error());
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(error()),
    };
    if d == 0 || d > days {
        return Err(error());
    }
    Ok(text.to_owned())
}

/// A number given as text, with a decimal comma or point.
fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim().replace(',', ".");
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// A value given in a command or a search: JSON numbers of quantities are
/// canonical (mm, mm², mm³, kg, radians); text is read in `unit` (the
/// type's or the design's) unless it names its own (`1.2 m`, `500 g`,
/// `30 deg`). Text of the other types: `yes`/`no`, integers, numbers with
/// a decimal comma or point, dates `YYYY-MM-DD`.
pub fn parse_value(
    base: BaseType,
    input: &Value,
    unit: Option<DisplayUnit>,
    context: &EvalContext,
) -> Result<PropertyValue, String> {
    let wrong = || format!("{input} is not a value of type {}", base.as_str());
    match (base, input) {
        (BaseType::Text, Value::String(t)) => Ok(PropertyValue::Text(t.clone())),
        (BaseType::Text, Value::Number(n)) => Ok(PropertyValue::Text(n.to_string())),
        (BaseType::Text, Value::Bool(b)) => Ok(PropertyValue::Text(
            if *b { "yes" } else { "no" }.to_owned(),
        )),
        (BaseType::Date, Value::String(t)) => check_date(t).map(PropertyValue::Text),
        (BaseType::Boolean, Value::Bool(b)) => Ok(PropertyValue::Boolean(*b)),
        (BaseType::Boolean, Value::String(t)) => match t.trim().to_ascii_lowercase().as_str() {
            "yes" | "true" | "1" | "y" => Ok(PropertyValue::Boolean(true)),
            "no" | "false" | "0" | "n" => Ok(PropertyValue::Boolean(false)),
            _ => Err(wrong()),
        },
        (BaseType::Integer, Value::Number(n)) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => Ok(PropertyValue::Integer(i)),
            (None, Some(f)) => PropertyValue::Number(f).conform(base),
            _ => Err(wrong()),
        },
        (BaseType::Integer, Value::String(t)) => t
            .trim()
            .parse::<i64>()
            .map(PropertyValue::Integer)
            .map_err(|_| wrong()),
        (BaseType::Number, Value::Number(n)) => {
            n.as_f64().map(PropertyValue::Number).ok_or_else(wrong)
        }
        (BaseType::Number, Value::String(t)) => {
            parse_number(t).map(PropertyValue::Number).ok_or_else(wrong)
        }
        (b, Value::Number(n)) if b.is_quantity() => n
            .as_f64()
            .filter(|f| f.is_finite())
            .map(PropertyValue::Number)
            .ok_or_else(wrong),
        (BaseType::Mass, Value::String(t)) => {
            let t = t.trim();
            let split = t.find(|c: char| c.is_alphabetic()).unwrap_or(t.len());
            let (number, symbol) = t.split_at(split);
            let number = parse_number(number).ok_or_else(wrong)?;
            let unit = match symbol.trim() {
                "" => unit.unwrap_or(DisplayUnit::Mass("kg", 1.0)),
                symbol => DisplayUnit::parse(BaseType::Mass, symbol)?,
            };
            Ok(PropertyValue::Number(unit.to_canonical(number)))
        }
        (b, Value::String(t)) if b.is_quantity() => {
            // A number alone is in the shown unit; one with its own unit
            // (or an expression of lengths) as written.
            let text = with_decimal_points(t.trim()).into_owned();
            let expected = match unit {
                Some(DisplayUnit::Geometric(u)) => u,
                _ => Unit::NONE,
            };
            let context = context.for_unit(expected);
            if let Some(number) = parse_number(&text)
                && let Some(DisplayUnit::Geometric(u)) = unit
            {
                return Ok(PropertyValue::Number(u.to_canonical(number)));
            }
            let quantity = crate::expr::evaluate(&text, &(), &context, expected)
                .map_err(|e| format!("“{t}” is not a value of type {}: {e}", b.as_str()))?;
            if quantity.dims != dims_of(b) {
                return Err(format!("“{t}” is not a value of type {}", b.as_str()));
            }
            Ok(PropertyValue::Number(quantity.value))
        }
        _ => Err(wrong()),
    }
}

/// A value as the user reads it: quantities in `unit` with its symbol,
/// plain numbers with their label, `yes`/`no`.
pub fn format_value(
    base: BaseType,
    value: &PropertyValue,
    unit: Option<DisplayUnit>,
    label: Option<&str>,
) -> String {
    match (value, unit) {
        (PropertyValue::Boolean(b), _) => if *b { "yes" } else { "no" }.to_owned(),
        (PropertyValue::Text(t), _) => t.clone(),
        (PropertyValue::Integer(i), _) => i.to_string(),
        (PropertyValue::Number(n), Some(unit)) if base.is_quantity() => {
            let shown = format_number(unit.from_canonical(*n), 6);
            format!("{shown} {}", unit.symbol())
        }
        (PropertyValue::Number(n), _) => match label.filter(|l| !l.is_empty()) {
            Some(label) => format!("{} {label}", format_number(*n, 6)),
            None => format_number(*n, 6),
        },
    }
}

/// A type the design defines: a name, a standard type under it, optional
/// allowed values (edited as a choice), unit, default and description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyType {
    pub name: String,
    pub base: BaseType,
    /// The values a property of the type may have (any when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<PropertyValue>,
    /// A quantity's unit (`m`, `deg`, `g`), or a number's label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// The value a new property of the type takes when none is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<PropertyValue>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl PropertyType {
    /// Why the type cannot be kept, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("a property type needs a name".to_owned());
        }
        if name != self.name {
            return Err(format!(
                "the type name {} starts or ends with spaces",
                self.name
            ));
        }
        if BaseType::parse(name).is_some() {
            return Err(format!("{name} is a standard type"));
        }
        if let Some(unit) = &self.unit
            && self.base.is_quantity()
        {
            DisplayUnit::parse(self.base, unit).map_err(|e| format!("{name}: {e}"))?;
        } else if self.unit.is_some() && self.base != BaseType::Number {
            return Err(format!(
                "{name}: only quantities and numbers have a unit, not {}",
                self.base.as_str()
            ));
        }
        for (i, value) in self.values.iter().enumerate() {
            value
                .clone()
                .conform(self.base)
                .map_err(|e| format!("{name}: values[{i}]: {e}"))?;
            if self.values[..i].iter().any(|v| v.same(value)) {
                return Err(format!(
                    "{name}: the value {} is listed twice",
                    value.to_json()
                ));
            }
        }
        if let Some(default) = &self.default {
            let default = default
                .clone()
                .conform(self.base)
                .map_err(|e| format!("{name}: default: {e}"))?;
            if !self.allows(&default) {
                return Err(format!(
                    "{name}: the default {} is not one of its values",
                    default.to_json()
                ));
            }
        }
        Ok(())
    }

    /// The type with its values in the form its base keeps them.
    pub fn conformed(mut self) -> Result<Self, String> {
        let base = self.base;
        self.values = self
            .values
            .into_iter()
            .map(|v| v.conform(base))
            .collect::<Result<_, _>>()?;
        self.default = self.default.map(|v| v.conform(base)).transpose()?;
        Ok(self)
    }

    /// Whether a value is allowed (any when no values are listed).
    pub fn allows(&self, value: &PropertyValue) -> bool {
        self.values.is_empty() || self.values.iter().any(|v| v.same(value))
    }
}

/// A property in a set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub key: String,
    /// A standard type's name (`text`, `length`, ...) or an own type's.
    #[serde(rename = "type")]
    pub ty: String,
    pub value: PropertyValue,
    #[serde(default = "user", skip_serializing_if = "is_user")]
    pub source: Source,
    /// The value its source gave, when the user has changed it since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<PropertyValue>,
    /// A plain number's unit label (`W`, `kN/m2`): shown, not converted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

impl Property {
    /// A property of a standard type from an importer or a library.
    pub fn new(key: &str, base: BaseType, value: PropertyValue, source: Source) -> Self {
        Self {
            key: key.to_owned(),
            ty: base.as_str().to_owned(),
            value,
            source,
            imported: None,
            unit: None,
        }
    }

    /// The same with a plain number's unit label.
    pub fn with_unit(mut self, unit: Option<String>) -> Self {
        self.unit = unit.filter(|u| !u.trim().is_empty());
        self
    }

    /// Whether the user changed an imported value.
    pub fn edited(&self) -> bool {
        self.imported.is_some()
    }
}

/// A named set of properties, in the order they were added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyGroup {
    pub name: String,
    pub properties: Vec<Property>,
}

/// The sets of one owner, as the design file lists them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerProperties {
    pub owner: PropertyOwner,
    pub sets: Vec<PropertyGroup>,
}

/// The design's own types and every owner's sets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropertyStore {
    /// Own types, in the order they were defined.
    pub types: Vec<PropertyType>,
    /// Each owner's sets, in the order they were made.
    pub owners: BTreeMap<PropertyOwner, Arc<Vec<PropertyGroup>>>,
}

/// Two names the same, whatever their case.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl PropertyStore {
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.owners.is_empty()
    }

    /// An own type by name (case-insensitive).
    pub fn own_type(&self, name: &str) -> Option<&PropertyType> {
        self.types.iter().find(|t| same_name(&t.name, name))
    }

    /// The base of a type name, standard or own.
    pub fn base_of(&self, ty: &str) -> Result<BaseType, String> {
        BaseType::parse(ty)
            .or_else(|| self.own_type(ty).map(|t| t.base))
            .ok_or_else(|| format!("there is no property type {ty}"))
    }

    /// An owner's sets (none when it has no properties).
    pub fn sets(&self, owner: PropertyOwner) -> &[PropertyGroup] {
        self.owners
            .get(&owner)
            .map_or(&[][..], |sets| sets.as_slice())
    }

    /// A property by set and key (both case-insensitive).
    pub fn get(&self, owner: PropertyOwner, set: &str, key: &str) -> Option<&Property> {
        self.sets(owner)
            .iter()
            .find(|g| same_name(&g.name, set))?
            .properties
            .iter()
            .find(|p| same_name(&p.key, key))
    }

    /// Sets a property, replacing one of the same set and key in its place
    /// (the set's and key's spelling stay the first one's).
    pub fn put(&mut self, owner: PropertyOwner, set: &str, property: Property) {
        let sets = Arc::make_mut(self.owners.entry(owner).or_default());
        let group = match sets.iter().position(|g| same_name(&g.name, set)) {
            Some(i) => &mut sets[i],
            None => {
                sets.push(PropertyGroup {
                    name: set.trim().to_owned(),
                    properties: Vec::new(),
                });
                sets.last_mut().expect("just pushed")
            }
        };
        match group
            .properties
            .iter_mut()
            .find(|p| same_name(&p.key, &property.key))
        {
            Some(old) => {
                let key = std::mem::take(&mut old.key);
                *old = Property { key, ..property };
            }
            None => group.properties.push(property),
        }
    }

    /// Removes a property; an empty set and an owner without sets go too.
    /// Whether there was one.
    pub fn remove(&mut self, owner: PropertyOwner, set: &str, key: &str) -> bool {
        let Some(sets) = self.owners.get_mut(&owner) else {
            return false;
        };
        let Some(i) = sets.iter().position(|g| same_name(&g.name, set)) else {
            return false;
        };
        let Some(j) = sets[i]
            .properties
            .iter()
            .position(|p| same_name(&p.key, key))
        else {
            return false;
        };
        let sets = Arc::make_mut(sets);
        sets[i].properties.remove(j);
        if sets[i].properties.is_empty() {
            sets.remove(i);
        }
        if sets.is_empty() {
            self.owners.remove(&owner);
        }
        true
    }

    /// Every stored property of a type, by owner and set.
    pub fn uses_of(&self, ty: &str) -> Vec<(PropertyOwner, String, String)> {
        let mut uses = Vec::new();
        for (owner, sets) in &self.owners {
            for group in sets.iter() {
                for property in &group.properties {
                    if same_name(&property.ty, ty) {
                        uses.push((*owner, group.name.clone(), property.key.clone()));
                    }
                }
            }
        }
        uses
    }

    /// Changes every property of type `ty` with `change`.
    pub fn change_type_uses(&mut self, ty: &str, mut change: impl FnMut(&mut Property)) {
        for sets in self.owners.values_mut() {
            let uses = sets
                .iter()
                .any(|g| g.properties.iter().any(|p| same_name(&p.ty, ty)));
            if !uses {
                continue;
            }
            for group in Arc::make_mut(sets).iter_mut() {
                for property in &mut group.properties {
                    if same_name(&property.ty, ty) {
                        change(property);
                    }
                }
            }
        }
    }

    /// The design file's `properties`: the owners `keep` takes, in order.
    pub fn file_owners(&self, keep: impl Fn(PropertyOwner) -> bool) -> Vec<OwnerProperties> {
        self.owners
            .iter()
            .filter(|(owner, sets)| keep(**owner) && !sets.is_empty())
            .map(|(owner, sets)| OwnerProperties {
                owner: *owner,
                sets: sets.to_vec(),
            })
            .collect()
    }

    /// The store of a design file's `property_types` and `properties`,
    /// checked: types valid and unique, values of their types (and of
    /// their own types' values), no stored set of computed values, no
    /// owner, set or key twice.
    pub fn from_file(
        types: Vec<PropertyType>,
        owners: Vec<OwnerProperties>,
    ) -> Result<Self, String> {
        let mut store = Self::default();
        for (i, ty) in types.into_iter().enumerate() {
            ty.check()
                .map_err(|e| format!("property_types[{i}]: {e}"))?;
            if store.own_type(&ty.name).is_some() {
                return Err(format!(
                    "property_types[{i}]: the type {} is defined twice",
                    ty.name
                ));
            }
            let ty = ty
                .conformed()
                .map_err(|e| format!("property_types[{i}]: {e}"))?;
            store.types.push(ty);
        }
        for (i, entry) in owners.into_iter().enumerate() {
            if store.owners.contains_key(&entry.owner) {
                return Err(format!("properties[{i}]: {} is listed twice", entry.owner));
            }
            let mut sets: Vec<PropertyGroup> = Vec::new();
            for (j, mut group) in entry.sets.into_iter().enumerate() {
                let at = format!("properties[{i}] ({}) sets[{j}]", entry.owner);
                check_set_name(&group.name).map_err(|e| format!("{at}: {e}"))?;
                if sets.iter().any(|g| same_name(&g.name, &group.name)) {
                    return Err(format!("{at}: the set {} is listed twice", group.name));
                }
                let mut properties: Vec<Property> = Vec::with_capacity(group.properties.len());
                for mut property in std::mem::take(&mut group.properties) {
                    let key = property.key.clone();
                    check_key(&key).map_err(|e| format!("{at}: {e}"))?;
                    if properties.iter().any(|p| same_name(&p.key, &key)) {
                        return Err(format!("{at}: the key {key} is listed twice"));
                    }
                    let error = |e: String| format!("{at} ({key}): {e}");
                    let base = store.base_of(&property.ty).map_err(error)?;
                    if property.source == Source::Computed {
                        return Err(error("computed values are not stored".to_owned()));
                    }
                    property.value = property.value.clone().conform(base).map_err(error)?;
                    property.imported = property
                        .imported
                        .take()
                        .map(|v| v.conform(base))
                        .transpose()
                        .map_err(error)?;
                    properties.push(property);
                }
                group.properties = properties;
                if !group.properties.is_empty() {
                    sets.push(group);
                }
            }
            if !sets.is_empty() {
                store.owners.insert(entry.owner, Arc::new(sets));
            }
        }
        Ok(store)
    }
}

/// Why a set name cannot hold stored properties, if it cannot.
pub fn check_set_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("a property set needs a name".to_owned());
    }
    if same_name(name, COMPUTED_SET) {
        return Err(format!(
            "the set {COMPUTED_SET} holds computed values, which are not stored"
        ));
    }
    Ok(())
}

/// Why a key cannot name a property, if it cannot.
pub fn check_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("a property needs a key".to_owned());
    }
    if key.contains('/') {
        return Err(format!(
            "the key {key} has a '/', which separates a set from a key in searches"
        ));
    }
    Ok(())
}

/// A key from a name another file gives (an import's): its `/`, which
/// separates a set from a key in searches, written as `∕` (division
/// slash), so that no value is left out for its name.
pub fn key_of(name: &str) -> String {
    name.trim().replace('/', "\u{2215}")
}

/// A comparison of a property search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// The key is there, whatever its value.
    Exists,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    /// Text that contains the value (case-insensitive).
    Contains,
}

/// A property search: `FireRating = EI60`, `Pset_WallCommon/FireRating`,
/// `Mass > 10 kg`, `Name ~ wall`.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyFilter {
    pub set: Option<String>,
    pub key: String,
    pub compare: Compare,
    pub value: String,
}

impl PropertyFilter {
    pub fn parse(text: &str) -> Result<Self, String> {
        const OPERATORS: [(&str, Compare); 8] = [
            ("!=", Compare::NotEqual),
            ("<=", Compare::LessOrEqual),
            (">=", Compare::GreaterOrEqual),
            ("==", Compare::Equal),
            ("=", Compare::Equal),
            ("<", Compare::Less),
            (">", Compare::Greater),
            ("~", Compare::Contains),
        ];
        let at = text.find(['=', '!', '<', '>', '~']);
        let (name, compare, value) = match at {
            None => (text, Compare::Exists, ""),
            Some(at) => {
                let rest = &text[at..];
                let (op, compare) = OPERATORS
                    .iter()
                    .find(|(op, _)| rest.starts_with(op))
                    .ok_or_else(|| format!("“{text}”: “{}” is no comparison", &rest[..1]))?;
                (&text[..at], *compare, rest[op.len()..].trim())
            }
        };
        let (set, key) = match name.rsplit_once('/') {
            Some((set, key)) => (Some(set.trim().to_owned()), key.trim()),
            None => (None, name.trim()),
        };
        if key.is_empty() {
            return Err(format!("“{text}” names no property (Key = value)"));
        }
        if compare != Compare::Exists && value.is_empty() {
            return Err(format!("“{text}” has no value after its comparison"));
        }
        Ok(Self {
            set: set.filter(|s| !s.is_empty()),
            key: key.to_owned(),
            compare,
            value: value.to_owned(),
        })
    }

    /// Whether a set and key are the ones searched for.
    pub fn names(&self, set: &str, key: &str) -> bool {
        same_name(key, &self.key) && self.set.as_deref().is_none_or(|s| same_name(s, set))
    }

    /// Whether a value of type `base` matches.
    pub fn matches(
        &self,
        base: BaseType,
        value: &PropertyValue,
        unit: Option<DisplayUnit>,
        context: &EvalContext,
    ) -> bool {
        if self.compare == Compare::Exists {
            return true;
        }
        let text = format_value(base, value, unit, None);
        if self.compare == Compare::Contains {
            return text.to_lowercase().contains(&self.value.to_lowercase());
        }
        let wanted = match parse_value(base, &Value::String(self.value.clone()), unit, context) {
            Ok(wanted) => wanted,
            // A value that is not of the type: text compared as text.
            Err(_) => PropertyValue::Text(self.value.clone()),
        };
        let order = match (value.as_f64(), wanted.as_f64()) {
            (Some(a), Some(b)) if base.is_numeric() => {
                if value.same(&wanted) {
                    std::cmp::Ordering::Equal
                } else {
                    a.total_cmp(&b)
                }
            }
            _ => match (value, &wanted) {
                (PropertyValue::Boolean(a), PropertyValue::Boolean(b)) => a.cmp(b),
                _ => text.to_lowercase().cmp(&self.value.to_lowercase()),
            },
        };
        use std::cmp::Ordering::{Equal, Greater, Less};
        match self.compare {
            Compare::Equal => order == Equal,
            Compare::NotEqual => order != Equal,
            Compare::Less => order == Less,
            Compare::LessOrEqual => order != Greater,
            Compare::Greater => order == Greater,
            Compare::GreaterOrEqual => order != Less,
            Compare::Exists | Compare::Contains => unreachable!("handled above"),
        }
    }
}
